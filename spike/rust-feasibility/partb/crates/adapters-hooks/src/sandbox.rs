//! The `QuickJS` sandbox: ECMAScript intrinsics only, no fs/net/process, no module
//! loader, a per-call deadline and a heap cap.
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use ir::{FileName, VarName};
use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Function, Module, Object, Persistent, Runtime, Value};
use serde_json::Value as Json;
use sha2::Sha256;
use tsstrip::StrippedModule;

use crate::HookConfig;
use crate::marshal::{js_to_json, json_to_js};

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: usize,
}

pub fn hmac_sha256_hex(key: &str, data: &str) -> String {
    let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()) else { return String::new() };
    mac.update(data.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Nanoseconds since `epoch` after which the running hook is interrupted; 0 = disarmed.
#[derive(Clone)]
struct Deadline {
    epoch: Instant,
    at: Arc<AtomicU64>,
}

impl Deadline {
    fn new() -> Self {
        Self { epoch: Instant::now(), at: Arc::new(AtomicU64::new(0)) }
    }
    fn now(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    fn arm(&self, budget: Duration) {
        let budget = u64::try_from(budget.as_nanos()).unwrap_or(u64::MAX);
        self.at.store(self.now().saturating_add(budget), Ordering::Relaxed);
    }
    fn disarm(&self) {
        self.at.store(0, Ordering::Relaxed);
    }
    fn expired(&self) -> bool {
        let at = self.at.load(Ordering::Relaxed);
        at != 0 && self.now() > at
    }
}

struct Loaded {
    stripped: StrippedModule,
    namespace: Persistent<Object<'static>>,
}

// Field order is load-bearing: Rust drops fields top to bottom and every
// Persistent handle must be released before the Context and Runtime.
pub struct Sandbox {
    modules: HashMap<FileName, Loaded>,
    secret_fn: Persistent<Function<'static>>,
    context: Context,
    _runtime: Runtime,
    deadline: Deadline,
    timeout: Duration,
}

fn caught_text(e: &CaughtError<'_>) -> String {
    match e {
        CaughtError::Exception(ex) => {
            format!("{}\n{}", ex.message().unwrap_or_default(), ex.stack().unwrap_or_default())
        }
        other => other.to_string(),
    }
}

fn install_globals(ctx: &Ctx<'_>, secrets: HashMap<String, String>) -> rquickjs::Result<Persistent<Function<'static>>> {
    let hmac = Function::new(ctx.clone(), |k: String, d: String| hmac_sha256_hex(&k, &d))?;
    ctx.globals().set("hmacSha256", hmac)?;
    let secret = Function::new(ctx.clone(), move |ctx: Ctx<'_>, name: String| {
        secrets
            .get(&name)
            .cloned()
            .ok_or_else(|| rquickjs::Exception::throw_message(&ctx, &format!("unknown secret {name}")))
    })?;
    Ok(Persistent::save(ctx, secret))
}

impl Sandbox {
    pub fn load_all(config: &HookConfig) -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_memory_limit(config.limits.memory_bytes);
        runtime.set_max_stack_size(512 * 1024);
        let deadline = Deadline::new();
        let watch = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || watch.expired())));
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        let secret_fn = context.with(|ctx| install_globals(&ctx, config.secrets.clone())).map_err(|e| e.to_string())?;
        let mut sandbox = Self {
            modules: HashMap::new(),
            secret_fn,
            context,
            _runtime: runtime,
            deadline,
            timeout: config.limits.timeout,
        };
        for name in &config.modules {
            sandbox.load(config, name)?;
        }
        Ok(sandbox)
    }

    fn load(&mut self, config: &HookConfig, name: &FileName) -> Result<(), String> {
        let path = config.root.join(name.as_str());
        let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let stripped = tsstrip::strip_types(&path, &source).map_err(|e| e.to_string())?;
        let namespace = self.context.with(|ctx| {
            let evaluate = || -> rquickjs::Result<Persistent<Object<'static>>> {
                let (module, done) =
                    Module::declare(ctx.clone(), stripped.name.clone(), stripped.code.clone())?.eval()?;
                done.finish::<()>()?;
                Ok(Persistent::save(&ctx, module.namespace()?))
            };
            evaluate().catch(&ctx).map_err(|e| stripped.remap_stack(&caught_text(&e)))
        })?;
        self.modules.insert(name.clone(), Loaded { stripped, namespace });
        Ok(())
    }

    /// Call `export({ request, secret })` and return the JSON result.
    pub fn call(&self, module: &FileName, export: &VarName, request: &Json) -> Result<Json, String> {
        let loaded = self.modules.get(module).ok_or_else(|| format!("hook module not loaded: {module}"))?;
        self.deadline.arm(self.timeout);
        let result = self.context.with(|ctx| {
            let invoke = || -> rquickjs::Result<Json> {
                let f: Function = loaded.namespace.clone().restore(&ctx)?.get(export.as_str())?;
                let arg = Object::new(ctx.clone())?;
                arg.set("request", json_to_js(&ctx, request)?)?;
                arg.set("secret", self.secret_fn.clone().restore(&ctx)?)?;
                let out: Value = f.call((arg,))?;
                js_to_json(&ctx, &out)
            };
            invoke().catch(&ctx).map_err(|e| loaded.stripped.remap_stack(&caught_text(&e)))
        });
        self.deadline.disarm();
        result
    }
}
