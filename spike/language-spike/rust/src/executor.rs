//! Runs compiled programs: API calls, given memoization (singleflight), assertions.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use base64::Engine as _;
use futures::future::{join_all, try_join_all};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{OnceCell, Semaphore};

use crate::model::{Api, Given, Program, Scope, Test};
use crate::ports::{
    CompiledExpr, Exposed, ExprEngine, GivenResolver, GivenScope, HttpRequest, HttpResponse, HttpTransport, Reporter, RunSummary,
    SchemaValidator, TestResult,
};
use crate::runtime::{canonical, json_path, render_map, render_path, render_value, Vars};

pub struct ApiCaller {
    pub http: Arc<dyn HttpTransport>,
    pub base_url: String,
}

pub struct CallResult {
    pub res: HttpResponse,
    pub outputs: BTreeMap<String, Value>,
}

impl ApiCaller {
    pub async fn call(&self, api: &Api, inputs: &Vars) -> Result<CallResult, String> {
        let body = match &api.body {
            Some(b) => Some(serde_json::to_vec(&render_value(b, inputs)).map_err(|e| e.to_string())?),
            None => None,
        };
        let req = HttpRequest { method: api.method.as_str(), url: format!("{}{}", self.base_url, render_path(&api.path, inputs)), body };
        let res = self.http.send(req).await.map_err(|e| e.to_string())?;
        let outputs = api
            .outputs
            .iter()
            .map(|(name, path)| (name.clone(), json_path(&res.body, path).cloned().unwrap_or(Value::Null)))
            .collect();
        Ok(CallResult { res, outputs })
    }
}

/// Memoizes per (givenId, scopeKey, hash(inputs)). Each key owns a `OnceCell`;
/// concurrent callers await the same initialization (singleflight). A failed
/// initialization leaves the cell empty, so it is not cached.
pub struct MemoGivenResolver {
    program: Arc<Program>,
    caller: Arc<ApiCaller>,
    cells: Mutex<HashMap<String, Arc<OnceCell<Exposed>>>>,
}

impl MemoGivenResolver {
    pub fn new(program: Arc<Program>, caller: Arc<ApiCaller>) -> Self {
        Self { program, caller, cells: Mutex::new(HashMap::new()) }
    }

    async fn run(&self, g: &Given, inputs: &Vars) -> Result<Exposed, String> {
        let api = &self.program.apis[&g.api.value].def;
        let r = self.caller.call(api, inputs).await.map_err(|e| format!("given {}: {e}", g.id.value))?;
        if let Some(guards) = &api.guards
            && !guards.status.matches(r.res.status)
        {
            return Err(format!("given {}: guard status {} failed (got {})", g.id.value, guards.status, r.res.status));
        }
        let exposed = g.expose.iter().map(|(name, out)| (name.clone(), r.outputs.get(out).cloned().unwrap_or(Value::Null))).collect();
        Ok(Arc::new(exposed))
    }
}

fn scope_key<'a>(g: &Given, s: &'a GivenScope) -> &'a str {
    match g.scope {
        Scope::Run => "run",
        Scope::File => &s.file,
        Scope::Test => &s.instance,
    }
}

fn hash_inputs(inputs: &Vars) -> String {
    let digest = Sha256::digest(canonical(&Value::Object(inputs.clone())).as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest[..12])
}

#[async_trait]
impl GivenResolver for MemoGivenResolver {
    async fn resolve(&self, given_id: &str, scope: &GivenScope) -> Result<Exposed, String> {
        let g = &self.program.givens.get(given_id).ok_or_else(|| format!("unknown given {given_id}"))?.def;
        let deps = resolve_uses(self, &g.uses, scope).await?;
        let inputs = render_map(&g.with_, &deps);
        let key = format!("{}\0{}\0{}", g.id.value, scope_key(g, scope), hash_inputs(&inputs));
        let cell = self.cells.lock().expect("poisoned").entry(key).or_default().clone();
        cell.get_or_try_init(|| self.run(g, &inputs)).await.cloned()
    }
}

async fn resolve_uses(
    resolver: &(impl GivenResolver + ?Sized),
    uses: &BTreeMap<String, serde_saphyr::Spanned<String>>,
    scope: &GivenScope,
) -> Result<Vars, String> {
    let resolved = try_join_all(uses.iter().map(|(alias, id)| async move {
        let exposed = resolver.resolve(&id.value, scope).await?;
        let obj: serde_json::Map<String, Value> = exposed.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        Ok::<_, String>((alias.clone(), Value::Object(obj)))
    }))
    .await?;
    Ok(resolved.into_iter().collect())
}

pub struct Executor {
    pub program: Arc<Program>,
    pub caller: Arc<ApiCaller>,
    pub givens: Arc<dyn GivenResolver>,
    pub exprs: Arc<dyn ExprEngine>,
    pub schemas: Arc<dyn SchemaValidator>,
    pub reporter: Arc<dyn Reporter>,
    pub expr_cache: Mutex<HashMap<String, Arc<dyn CompiledExpr>>>,
}

impl Executor {
    pub async fn run(self: &Arc<Self>, concurrency: usize) -> RunSummary {
        let start = Instant::now();
        let sem = Arc::new(Semaphore::new(concurrency));
        let mut handles = Vec::new();
        for idx in 0..self.program.tests.len() {
            for iteration in 0..self.program.tests[idx].def.repeat {
                let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
                let this = self.clone();
                handles.push(tokio::spawn(async move {
                    let r = this.run_one(idx, iteration).await;
                    drop(permit);
                    r
                }));
            }
        }
        let results: Vec<TestResult> = join_all(handles).await.into_iter().map(|r| r.expect("task panicked")).collect();
        let passed = results.iter().filter(|r| r.passed).count();
        let summary =
            RunSummary { total: results.len(), passed, failed: results.len() - passed, duration_ms: start.elapsed().as_secs_f64() * 1e3 };
        self.reporter.run_finished(&summary);
        summary
    }

    async fn run_one(&self, idx: usize, iteration: u32) -> TestResult {
        let start = Instant::now();
        let located = &self.program.tests[idx];
        let t = &located.def;
        let scope = GivenScope { file: located.file.clone(), instance: format!("{}#{iteration}", t.id.value) };
        let failures = match resolve_uses(self.givens.as_ref(), &t.uses, &scope).await {
            Err(e) => vec![e],
            Ok(deps) => {
                let api = &self.program.apis[&t.api.value].def;
                // Tests run assertions only: api guards are for givens/load, so a test may assert a 404.
                match self.caller.call(api, &render_map(&t.with_, &deps)).await {
                    Err(e) => vec![e],
                    Ok(r) => self.assert(t, &r.res, &deps),
                }
            }
        };
        let result = TestResult {
            id: t.id.value.clone(),
            iteration,
            passed: failures.is_empty(),
            failures,
            duration_ms: start.elapsed().as_secs_f64() * 1e3,
        };
        self.reporter.test_finished(&result);
        result
    }

    fn assert(&self, t: &Test, res: &HttpResponse, deps: &Vars) -> Vec<String> {
        let mut failures = Vec::new();
        let exp = &t.expect;
        if let Some(status) = exp.status
            && status != res.status
        {
            failures.push(format!("status: expected {status}, got {}", res.status));
        }
        for (path, want) in &exp.r#match {
            let want = render_value(want, deps);
            let got = json_path(&res.body, path).cloned().unwrap_or(Value::Null);
            if canonical(&got) != canonical(&want) {
                failures.push(format!("match {path}: expected {}, got {}", canonical(&want), canonical(&got)));
            }
        }
        if let Some(schema) = &exp.schema {
            match self.schemas.validate(schema, &res.body) {
                Ok(msgs) => failures.extend(msgs.into_iter().map(|m| format!("schema: {m}"))),
                Err(e) => failures.push(format!("schema: {e}")),
            }
        }
        if let Some(cel) = &exp.cel {
            match self.compiled(&cel.value).and_then(|p| p.eval_bool(&res.body, res.status)) {
                Ok(true) => {}
                Ok(false) => failures.push(format!("cel: `{}` was false", cel.value)),
                Err(e) => failures.push(format!("cel: {e}")),
            }
        }
        failures
    }

    fn compiled(&self, src: &str) -> Result<Arc<dyn CompiledExpr>, String> {
        let mut cache = self.expr_cache.lock().expect("poisoned");
        if let Some(p) = cache.get(src) {
            return Ok(p.clone());
        }
        let p = self.exprs.compile(src)?;
        cache.insert(src.to_owned(), p.clone());
        Ok(p)
    }
}
