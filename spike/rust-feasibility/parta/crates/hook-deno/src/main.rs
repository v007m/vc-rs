//! V8 (deno_core) prototype of the HookRunner port.
mod loader;
mod marshal;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deno_core::error::JsError;
use deno_core::{JsRuntime, ModuleSpecifier, OpState, RuntimeOptions, extension, op2, v8};
use deno_error::JsErrorBox;
use hookbench::{CheckOutcome, Deadline, ErrorKind, HookEngine, HookError, Limits, Marshal};
use serde_json::Value as Json;

use crate::loader::{HookLoader, SourceMaps};

struct Secrets(HashMap<String, String>);

#[op2]
#[string]
fn op_hmac_sha256(#[string] key: String, #[string] data: String) -> String {
    hookbench::hmac_sha256_hex(&key, &data)
}

#[op2]
#[string]
fn op_secret(state: &mut OpState, #[string] name: String) -> Result<String, JsErrorBox> {
    state.borrow::<Secrets>().0.get(&name).cloned().ok_or_else(|| JsErrorBox::generic(format!("unknown secret {name}")))
}

extension!(vibranium_hooks, ops = [op_hmac_sha256, op_secret], state = |state| state.put(Secrets(hookbench::secrets())));

/// Installs host functions, captures the secret accessor, then removes every
/// deno_core global (Deno.core exposes raw ops) so hooks see plain ECMAScript.
const BOOTSTRAP: &str = r#"(() => {
  const { op_hmac_sha256, op_secret } = Deno.core.ops;
  globalThis.hmacSha256 = (key, data) => op_hmac_sha256(key, data);
  const secret = (name) => op_secret(name);
  for (const k of ["Deno", "__bootstrap", "__infra"]) delete globalThis[k];
  return secret;
})()"#;

pub struct DenoEngine {
    secret_fn: v8::Global<v8::Value>,
    namespaces: HashMap<PathBuf, v8::Global<v8::Object>>,
    runtime: JsRuntime,
    tokio: tokio::runtime::Runtime,
    maps: SourceMaps,
    deadline: Deadline,
    timeout: Duration,
    marshal: Marshal,
    timed_out: Arc<AtomicBool>,
    oom: Arc<AtomicBool>,
    stop_watchdog: Arc<AtomicBool>,
}

impl Drop for DenoEngine {
    fn drop(&mut self) {
        self.stop_watchdog.store(true, Ordering::Relaxed);
    }
}

fn err(kind: ErrorKind, message: impl Into<String>) -> HookError {
    HookError { kind, message: message.into() }
}

enum Called {
    Value(Json),
    Promise(v8::Global<v8::Value>),
}

impl DenoEngine {
    pub fn new(limits: &Limits, marshal: Marshal) -> Result<Self, HookError> {
        let maps: SourceMaps = Rc::new(RefCell::new(HashMap::new()));
        // V8 posts delayed tasks (GC, heap-limit handling) to the tokio runtime current at isolate creation.
        let tokio = tokio::runtime::Builder::new_current_thread().enable_time().build().map_err(|e| err(ErrorKind::Load, e.to_string()))?;
        let guard = tokio.enter();
        let mut runtime = JsRuntime::try_new(RuntimeOptions {
            module_loader: Some(Rc::new(HookLoader { maps: maps.clone() })),
            extensions: vec![vibranium_hooks::init()],
            create_params: Some(v8::CreateParams::default().heap_limits(0, limits.memory_bytes)),
            ..Default::default()
        })
        .map_err(|e| err(ErrorKind::Load, e.to_string()))?;
        let handle = runtime.v8_isolate().thread_safe_handle();
        let oom = Arc::new(AtomicBool::new(false));
        let (oom_flag, oom_handle) = (oom.clone(), handle.clone());
        // Without this callback V8 aborts the whole process when the heap limit is hit.
        runtime.add_near_heap_limit_callback(move |current, _initial| {
            oom_flag.store(true, Ordering::Relaxed);
            oom_handle.terminate_execution();
            current * 2 // headroom so termination can unwind
        });
        let deadline = Deadline::default();
        let timed_out = Arc::new(AtomicBool::new(false));
        let stop_watchdog = Arc::new(AtomicBool::new(false));
        spawn_watchdog(deadline.clone(), handle, timed_out.clone(), stop_watchdog.clone());
        let secret_fn = runtime.execute_script("<bootstrap>", BOOTSTRAP).map_err(|e| err(ErrorKind::Load, e.to_string()))?;
        drop(guard);
        Ok(Self { secret_fn, namespaces: HashMap::new(), runtime, tokio, maps, deadline, timeout: limits.timeout, marshal, timed_out, oom, stop_watchdog })
    }

    fn classify(&mut self, message: String) -> HookError {
        let kind = if self.oom.swap(false, Ordering::Relaxed) {
            ErrorKind::OutOfMemory
        } else if self.timed_out.swap(false, Ordering::Relaxed) {
            ErrorKind::Timeout
        } else {
            ErrorKind::Thrown
        };
        if kind != ErrorKind::Thrown {
            self.runtime.v8_isolate().cancel_terminate_execution();
        }
        err(kind, message)
    }

    /// Synchronous fast path: one V8 scope, marshal in, call, marshal out.
    fn call_sync(&mut self, ns: &v8::Global<v8::Object>, export: &str, request: &Json, response: Option<&Json>) -> Result<Called, String> {
        let marshal = self.marshal;
        let secret_fn = self.secret_fn.clone();
        deno_core::scope!(scope, &mut self.runtime);
        v8::tc_scope!(let tc, scope);
        let ns = v8::Local::new(tc, ns);
        let key = v8::String::new(tc, export).ok_or("alloc")?;
        let f: v8::Local<v8::Function> = ns.get(tc, key.into()).and_then(|v| v.try_into().ok()).ok_or_else(|| format!("no exported function {export}"))?;
        let arg = v8::Object::new(tc);
        let req = marshal::to_v8(tc, request, marshal)?;
        set(tc, arg, "request", req)?;
        match response {
            Some(r) => {
                let resp = marshal::to_v8(tc, r, marshal)?;
                set(tc, arg, "response", resp)?;
            }
            None => {
                let s = v8::Local::new(tc, secret_fn);
                set(tc, arg, "secret", s)?;
            }
        }
        let undefined = v8::undefined(tc).into();
        match f.call(tc, undefined, &[arg.into()]) {
            Some(v) if v.is_promise() => Ok(Called::Promise(v8::Global::new(tc, v))),
            Some(v) => marshal::from_v8(tc, v, marshal).map(Called::Value),
            None if tc.has_terminated() => Err("execution terminated".to_owned()),
            None => {
                let exception = tc.exception().ok_or("unknown exception")?;
                Err(JsError::from_v8_exception(tc, exception).to_string())
            }
        }
    }

    fn settle(&mut self, promise: v8::Global<v8::Value>) -> Result<Json, String> {
        let fut = self.runtime.resolve(promise);
        let settled = self.tokio.block_on(self.runtime.with_event_loop_promise(fut, Default::default())).map_err(|e| e.to_string())?;
        let marshal = self.marshal;
        deno_core::scope!(scope, &mut self.runtime);
        let local = v8::Local::new(scope, settled);
        marshal::from_v8(scope, local, marshal)
    }

    fn invoke(&mut self, module: &Path, export: &str, request: &Json, response: Option<&Json>) -> Result<Json, HookError> {
        let ns = self.namespaces.get(module).cloned().ok_or_else(|| err(ErrorKind::Load, format!("not loaded: {}", module.display())))?;
        self.deadline.arm(self.timeout);
        let result = match self.call_sync(&ns, export, request, response) {
            Ok(Called::Value(v)) => Ok(v),
            Ok(Called::Promise(p)) => self.settle(p),
            Err(e) => Err(e),
        };
        self.deadline.disarm();
        result.map_err(|m| self.classify(m))
    }
}

fn set<'s>(scope: &mut v8::PinScope<'s, '_>, obj: v8::Local<'s, v8::Object>, key: &str, value: v8::Local<'s, v8::Value>) -> Result<(), String> {
    let k = v8::String::new(scope, key).ok_or("alloc")?;
    obj.set(scope, k.into(), value).map(|_| ()).ok_or_else(|| format!("cannot set {key}"))
}

/// One long-lived thread per isolate; arming a call is a single atomic store.
fn spawn_watchdog(deadline: Deadline, handle: v8::IsolateHandle, timed_out: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(2));
            if deadline.expired() {
                deadline.disarm();
                timed_out.store(true, Ordering::Relaxed);
                handle.terminate_execution();
            }
        }
    });
}

impl HookEngine for DenoEngine {
    fn load(&mut self, path: &Path) -> Result<(), HookError> {
        let load_err = |m: String| err(ErrorKind::Load, m);
        let abs = std::fs::canonicalize(path).map_err(|e| load_err(e.to_string()))?;
        let source = std::fs::read_to_string(&abs).map_err(|e| load_err(e.to_string()))?;
        let stripped = tsstrip::strip_types(path, &source).map_err(|e| load_err(e.to_string()))?;
        let specifier = ModuleSpecifier::from_file_path(&abs).map_err(|()| load_err("bad path".into()))?;
        self.maps.borrow_mut().insert(specifier.to_string(), stripped.source_map.to_json_string().into_bytes());
        let runtime = &mut self.runtime;
        let ns = self.tokio.block_on(async {
            let id = runtime.load_side_es_module_from_code(&specifier, stripped.code).await?;
            let evaluated = runtime.mod_evaluate(id);
            runtime.run_event_loop(Default::default()).await?;
            evaluated.await?;
            runtime.get_module_namespace(id)
        });
        let ns = ns.map_err(|e| load_err(e.to_string()))?;
        self.namespaces.insert(path.to_path_buf(), ns);
        Ok(())
    }

    fn before_request(&mut self, module: &Path, export: &str, request: &Json) -> Result<Json, HookError> {
        self.invoke(module, export, request, None)
    }

    fn check(&mut self, module: &Path, export: &str, request: &Json, response: &Json) -> Result<CheckOutcome, HookError> {
        self.invoke(module, export, request, Some(response)).map(|v| hookbench::check_outcome(&v))
    }
}

fn micro() {
    let limits = Limits { timeout: Duration::from_millis(200), memory_bytes: 64 << 20 };
    let mut e = DenoEngine::new(&limits, Marshal::Direct).unwrap_or_else(|_| std::process::exit(2));
    let (req, resp) = (hookbench::sample_request(), hookbench::sample_response());
    let n = 20_000u32;
    let t = std::time::Instant::now();
    for _ in 0..n { deno_core::scope!(scope, &mut e.runtime); v8::tc_scope!(let _tc, scope); }
    println!("scope only: {:.0} ns", t.elapsed().as_nanos() as f64 / f64::from(n));
    let t = std::time::Instant::now();
    for _ in 0..n { deno_core::scope!(scope, &mut e.runtime); let _ = marshal::to_v8(scope, &req, Marshal::Direct); }
    println!("scope + to_v8(request): {:.0} ns", t.elapsed().as_nanos() as f64 / f64::from(n));
    let t = std::time::Instant::now();
    for _ in 0..n { deno_core::scope!(scope, &mut e.runtime); let _ = marshal::to_v8(scope, &resp, Marshal::Direct); }
    println!("scope + to_v8(response): {:.0} ns", t.elapsed().as_nanos() as f64 / f64::from(n));
    let t = std::time::Instant::now();
    for _ in 0..n { deno_core::scope!(scope, &mut e.runtime); if let Ok(v) = marshal::to_v8(scope, &req, Marshal::Direct) { let _ = marshal::from_v8(scope, v, Marshal::Direct); } }
    println!("scope + to_v8 + from_v8(request): {:.0} ns", t.elapsed().as_nanos() as f64 / f64::from(n));
    let t = std::time::Instant::now();
    for _ in 0..n { let _ = serde_json::to_string(&resp).ok().and_then(|s| serde_json::from_str::<Json>(&s).ok()); }
    println!("rust-only serde_json roundtrip(response): {:.0} ns", t.elapsed().as_nanos() as f64 / f64::from(n));
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("micro") {
        return micro();
    }
    let timeout_ms = std::env::var("HOOK_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let limits = Limits { timeout: Duration::from_millis(timeout_ms), memory_bytes: 64 << 20 };
    hookbench::run("v8-deno_core", |m| match DenoEngine::new(&limits, m) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}", e.message);
            std::process::exit(2)
        }
    });
}
