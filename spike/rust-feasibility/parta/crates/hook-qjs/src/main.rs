//! QuickJS (rquickjs) prototype of the HookRunner port.
mod marshal;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hookbench::{CheckOutcome, Deadline, ErrorKind, HookEngine, HookError, Limits, Marshal};
use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Function, Module, Object, Persistent, Runtime, Value};
use serde_json::Value as Json;
use tsstrip::StrippedModule;

struct LoadedModule {
    stripped: StrippedModule,
    namespace: Persistent<Object<'static>>,
}

// Field order matters: Rust drops fields top to bottom, and every Persistent
// handle must be released before the Context and Runtime that own the heap.
pub struct QjsEngine {
    modules: HashMap<PathBuf, LoadedModule>,
    secret_fn: Persistent<Function<'static>>,
    context: Context,
    runtime: Runtime,
    deadline: Deadline,
    timeout: Duration,
    marshal: Marshal,
}

fn qjs_err(e: &rquickjs::Error) -> HookError {
    HookError { kind: ErrorKind::Load, message: e.to_string() }
}

fn caught_text(e: &CaughtError<'_>) -> String {
    match e {
        CaughtError::Exception(ex) => format!("{}\n{}", ex.message().unwrap_or_default(), ex.stack().unwrap_or_default()),
        other => other.to_string(),
    }
}

impl QjsEngine {
    pub fn new(limits: &Limits, marshal: Marshal) -> Result<Self, HookError> {
        let runtime = Runtime::new().map_err(|e| qjs_err(&e))?;
        runtime.set_memory_limit(limits.memory_bytes);
        runtime.set_max_stack_size(512 * 1024);
        let deadline = Deadline::default();
        let watch = deadline.clone();
        // Polled by the interpreter; returning true raises an uncatchable "interrupted" error.
        runtime.set_interrupt_handler(Some(Box::new(move || watch.expired())));
        // Context::full = ECMAScript intrinsics only: no fs, net, process, timers, or module loader.
        let context = Context::full(&runtime).map_err(|e| qjs_err(&e))?;
        let secret_fn = context
            .with(|ctx| -> rquickjs::Result<Persistent<Function<'static>>> {
                let hmac = Function::new(ctx.clone(), |k: String, d: String| hookbench::hmac_sha256_hex(&k, &d))?;
                ctx.globals().set("hmacSha256", hmac)?;
                let secrets = hookbench::secrets();
                let secret = Function::new(ctx.clone(), move |ctx: Ctx<'_>, name: String| {
                    secrets.get(&name).cloned().ok_or_else(|| rquickjs::Exception::throw_message(&ctx, &format!("unknown secret {name}")))
                })?;
                Ok(Persistent::save(&ctx, secret))
            })
            .map_err(|e| qjs_err(&e))?;
        Ok(Self { runtime, context, deadline, timeout: limits.timeout, marshal, secret_fn, modules: HashMap::new() })
    }

    fn classify(&self, text: String) -> HookError {
        let kind = if text.contains("out of memory") {
            ErrorKind::OutOfMemory
        } else if text.contains("interrupted") || self.deadline.expired() {
            ErrorKind::Timeout
        } else {
            ErrorKind::Thrown
        };
        HookError { kind, message: text }
    }

    fn invoke(&mut self, module: &Path, export: &str, request: &Json, response: Option<&Json>) -> Result<Json, HookError> {
        let loaded = self.modules.get(module).ok_or_else(|| HookError { kind: ErrorKind::Load, message: format!("not loaded: {}", module.display()) })?;
        let marshal = self.marshal;
        self.deadline.arm(self.timeout);
        let result = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<Json> {
                let ns = loaded.namespace.clone().restore(&ctx)?;
                let f: Function = ns.get(export)?;
                let arg = Object::new(ctx.clone())?;
                arg.set("request", marshal::to_js(&ctx, request, marshal)?)?;
                match response {
                    Some(r) => arg.set("response", marshal::to_js(&ctx, r, marshal)?)?,
                    None => arg.set("secret", self.secret_fn.clone().restore(&ctx)?)?,
                }
                let out: Value = f.call((arg,))?;
                let settled = match out.as_promise() {
                    Some(p) => p.finish::<Value>()?,
                    None => out,
                };
                marshal::from_js(&ctx, &settled, marshal)
            };
            run().catch(&ctx).map_err(|e| loaded.stripped.remap_stack(&caught_text(&e)))
        });
        let mapped = result.map_err(|text| self.classify(text));
        self.deadline.disarm();
        if mapped.as_ref().is_err_and(|e| e.kind == ErrorKind::OutOfMemory) {
            self.runtime.run_gc();
        }
        mapped
    }
}

impl HookEngine for QjsEngine {
    fn load(&mut self, path: &Path) -> Result<(), HookError> {
        let load_err = |message: String| HookError { kind: ErrorKind::Load, message };
        let source = std::fs::read_to_string(path).map_err(|e| load_err(e.to_string()))?;
        let stripped = tsstrip::strip_types(path, &source).map_err(|e| load_err(e.to_string()))?;
        let namespace = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<Persistent<Object<'static>>> {
                let declared = Module::declare(ctx.clone(), stripped.name.clone(), stripped.code.clone())?;
                let (evaluated, done) = declared.eval()?;
                done.finish::<()>()?;
                Ok(Persistent::save(&ctx, evaluated.namespace()?))
            };
            run().catch(&ctx).map_err(|e| stripped.remap_stack(&caught_text(&e)))
        });
        let namespace = namespace.map_err(load_err)?;
        self.modules.insert(path.to_path_buf(), LoadedModule { stripped, namespace });
        Ok(())
    }

    fn before_request(&mut self, module: &Path, export: &str, request: &Json) -> Result<Json, HookError> {
        self.invoke(module, export, request, None)
    }

    fn check(&mut self, module: &Path, export: &str, request: &Json, response: &Json) -> Result<CheckOutcome, HookError> {
        self.invoke(module, export, request, Some(response)).map(|v| hookbench::check_outcome(&v))
    }
}

fn main() {
    let timeout_ms = std::env::var("HOOK_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    let limits = Limits { timeout: Duration::from_millis(timeout_ms), memory_bytes: 64 << 20 };
    hookbench::run("quickjs", |m| match QjsEngine::new(&limits, m) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}", e.message);
            std::process::exit(2)
        }
    });
}
