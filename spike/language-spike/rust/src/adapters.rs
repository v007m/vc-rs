//! Adapters implementing the ports with concrete crates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use cel::common::value::{CowVal, Val};
use cel::context::VariableResolver;
use cel::{Context, Env, Program as CelProgram, Value as CelValue};
use serde_json::Value;

use crate::ports::{
    CompiledExpr, ExprEngine, HttpRequest, HttpResponse, HttpTransport, Reporter, RunSummary, SchemaValidator, TestResult, TransportError,
};

/// reqwest (hyper) client with a connection pool sized for the concurrency limit.
/// reqwest has no hard per-host connection cap, so an optional semaphore bounds in-flight requests.
pub struct ReqwestTransport {
    client: reqwest::Client,
    in_flight: Option<tokio::sync::Semaphore>,
}

impl ReqwestTransport {
    pub fn new(max_idle: usize) -> Self {
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(max_idle)
            .timeout(std::time::Duration::from_secs(30))
            .no_proxy()
            .build()
            .expect("reqwest client");
        Self { client, in_flight: None }
    }

    pub fn with_limit(max_conns: usize) -> Self {
        Self { in_flight: Some(tokio::sync::Semaphore::new(max_conns)), ..Self::new(max_conns) }
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, req: HttpRequest) -> Result<HttpResponse, TransportError> {
        let _permit = match &self.in_flight {
            Some(s) => Some(s.acquire().await.map_err(|e| TransportError::Io(e.to_string()))?),
            None => None,
        };
        let method = reqwest::Method::from_bytes(req.method.as_bytes()).map_err(|e| TransportError::Io(e.to_string()))?;
        let mut rb = self.client.request(method, &req.url).header("content-type", "application/json");
        if let Some(body) = req.body {
            rb = rb.body(body);
        }
        let res = rb.send().await.map_err(|e| TransportError::Io(e.to_string()))?;
        let status = res.status().as_u16();
        let bytes = res.bytes().await.map_err(|e| TransportError::Io(e.to_string()))?;
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        Ok(HttpResponse { status, body })
    }
}

/// cel (cel-rust, formerly `cel-interpreter`).
pub struct CelRustEngine {
    env: Arc<Env>,
}

impl Default for CelRustEngine {
    fn default() -> Self {
        Self { env: Arc::new(Env::stdlib()) }
    }
}

struct CelExpr {
    program: CelProgram,
    env: Arc<Env>,
}

/// Converts a JSON body into a CEL value once per evaluation.
pub fn json_to_cel(v: &Value) -> CelValue {
    match v {
        Value::Null => CelValue::Null,
        Value::Bool(b) => CelValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => CelValue::Int(i),
            None => CelValue::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => CelValue::String(Arc::new(s.clone())),
        Value::Array(a) => CelValue::List(Arc::new(a.iter().map(json_to_cel).collect())),
        Value::Object(o) => {
            let map: HashMap<String, CelValue> = o.iter().map(|(k, v)| (k.clone(), json_to_cel(v))).collect();
            CelValue::Map(map.into())
        }
    }
}

/// Variables for one evaluation, converted to CEL's `Val` form once and then
/// lent to the interpreter (cel-rust's recommended hot path: no per-lookup copies).
pub struct CelVars {
    body: Box<dyn Val>,
    status: Box<dyn Val>,
}

impl CelVars {
    pub fn new(body: CelValue, status: u16) -> Result<Self, String> {
        let conv = |v: CelValue| Box::<dyn Val>::try_from(v).map_err(|e| e.to_string());
        Ok(Self { body: conv(body)?, status: conv(CelValue::Int(i64::from(status)))? })
    }
}

impl VariableResolver for CelVars {
    fn resolve<'b>(&'b self, name: &str) -> Option<CowVal<'b, 'b>> {
        match name {
            "body" => Some(CowVal::Borrowed(self.body.as_ref())),
            "status" => Some(CowVal::Borrowed(self.status.as_ref())),
            _ => None,
        }
    }
}

impl CelExpr {
    pub fn eval_vars(&self, vars: &CelVars) -> Result<bool, String> {
        let mut ctx = Context::with_env(self.env.clone());
        ctx.set_variable_resolver(vars);
        match self.program.execute(&ctx) {
            Ok(CelValue::Bool(b)) => Ok(b),
            Ok(other) => Err(format!("expression returned {other:?}, not bool")),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl CompiledExpr for CelExpr {
    fn eval_bool(&self, body: &Value, status: u16) -> Result<bool, String> {
        self.eval_vars(&CelVars::new(json_to_cel(body), status)?)
    }
}

impl CelRustEngine {
    /// Bench helper: evaluates against variables converted once up front (the "parsed body").
    pub fn compile_concrete(&self, source: &str) -> Result<impl Fn(&CelVars) -> Result<bool, String>, String> {
        let expr = CelExpr { program: self.env.compile(source).map_err(|e| e.to_string())?, env: self.env.clone() };
        Ok(move |vars: &CelVars| expr.eval_vars(vars))
    }
}

impl ExprEngine for CelRustEngine {
    fn compile(&self, source: &str) -> Result<Arc<dyn CompiledExpr>, String> {
        let program = self.env.compile(source).map_err(|e| e.to_string())?;
        Ok(Arc::new(CelExpr { program, env: self.env.clone() }))
    }
}

/// `jsonschema` crate with a compile cache.
#[derive(Default)]
pub struct JsonSchemaValidator {
    cache: Mutex<HashMap<String, Arc<jsonschema::Validator>>>,
}

impl SchemaValidator for JsonSchemaValidator {
    fn validate(&self, schema: &Value, value: &Value) -> Result<Vec<String>, String> {
        let key = schema.to_string();
        let validator = {
            let mut cache = self.cache.lock().expect("poisoned");
            match cache.get(&key) {
                Some(v) => v.clone(),
                None => {
                    let v = Arc::new(jsonschema::validator_for(schema).map_err(|e| e.to_string())?);
                    cache.insert(key, v.clone());
                    v
                }
            }
        };
        Ok(validator.iter_errors(value).map(|e| format!("{} {e}", display_path(&e.instance_path().to_string()))).collect())
    }
}

fn display_path(p: &str) -> &str {
    if p.is_empty() { "/" } else { p }
}

pub struct ConsoleReporter {
    pub verbose: bool,
}

impl Reporter for ConsoleReporter {
    fn test_finished(&self, r: &TestResult) {
        if r.passed && !self.verbose {
            return;
        }
        let suffix = if r.iteration > 0 { format!("#{}", r.iteration) } else { String::new() };
        let mut line = format!("{} {}{suffix} ({:.1} ms)", if r.passed { "PASS" } else { "FAIL" }, r.id, r.duration_ms);
        if !r.failures.is_empty() {
            line.push_str(&format!("\n  - {}", r.failures.join("\n  - ")));
        }
        println!("{line}");
    }

    fn run_finished(&self, s: &RunSummary) {
        println!("tests: {} total, {} passed, {} failed in {:.1} ms", s.total, s.passed, s.failed, s.duration_ms);
    }
}
