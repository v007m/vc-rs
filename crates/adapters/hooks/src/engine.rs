//! The `QuickJS` side, on the hook thread: one runtime with a context per entry module, a time
//! budget through the interrupt handler and a memory budget through the runtime's limit.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use rquickjs::promise::MaybePromise;
use rquickjs::{Context, Ctx, Exception, Function, Module, Object, Persistent, Runtime, Value};
use serde_json::Value as Json;
use vc_interfaces::{HookCall, HookError, HookOutput};

use crate::deadline::Deadline;
use crate::fault::{Fault, Seen, Thrown, classify};
use crate::loader::HookLoader;
use crate::marshal::{to_js, to_json};
use crate::state::CallState;

/// The most native stack a hook may use.
const STACK: usize = 512 * 1024;

/// How many entry contexts are kept before all are dropped and loaded afresh.
const MAX_ENTRIES: usize = 64;

/// How close to its limit the runtime counts as full: a thrown `null` there is an overrun.
const SLACK: usize = 64 * 1024;

/// An entry module, evaluated in its own context. The namespace is dropped before the context.
struct Loaded {
    namespace: Persistent<Object<'static>>,
    context: Context,
}

/// A `QuickJS` runtime and the entry modules loaded into it.
///
/// Field order matters: Rust drops fields top to bottom, and every context and persistent
/// handle must go before the runtime.
pub struct Engine {
    entries: HashMap<u64, Loaded>,
    state: Rc<CallState>,
    deadline: Deadline,
    limit: usize,
    runtime: Runtime,
}

impl Engine {
    /// A fresh runtime; `interrupts` arms the time budget (off only to test the watchdog).
    ///
    /// # Errors
    ///
    /// When `QuickJS` cannot allocate a runtime.
    pub fn new(interrupts: bool) -> rquickjs::Result<Self> {
        let runtime = Runtime::new()?;
        runtime.set_max_stack_size(STACK);
        let state = Rc::new(CallState::default());
        let loader = HookLoader::new(Rc::clone(&state));
        runtime.set_loader(loader.clone(), loader);
        let deadline = Deadline::new();
        if interrupts {
            let watch = deadline.clone();
            runtime.set_interrupt_handler(Some(Box::new(move || watch.interrupt())));
        }
        Ok(Self { entries: HashMap::new(), state, deadline, limit: usize::MAX, runtime })
    }

    /// Runs `call` within its budget.
    ///
    /// # Errors
    ///
    /// [`HookError`] when the hook throws, overruns, returns a value that is not JSON or is not
    /// there. After an overrun the engine must be dropped.
    pub fn call(&mut self, call: &HookCall) -> Result<HookOutput, HookError> {
        self.state.begin(call);
        self.set_limit(call.budget.memory);
        self.deadline.arm(call.budget.time);
        let result = self.run(call);
        let fired = self.deadline.disarm();
        let used = self.state.end();
        match result {
            Ok(value) => {
                let mut output = HookOutput::new(value);
                output.secrets_used = used;
                Ok(output)
            }
            Err(thrown) => {
                let memory_full = thrown.is_void() && self.is_full();
                Err(classify(thrown, call, Seen { fired, memory_full }))
            }
        }
    }

    /// Lowers or raises the memory limit, collecting garbage first when lowering it.
    fn set_limit(&mut self, memory: usize) {
        if memory < self.limit {
            self.runtime.run_gc();
        }
        self.runtime.set_memory_limit(memory);
        self.limit = memory;
    }

    fn is_full(&self) -> bool {
        let used = self.runtime.memory_usage().malloc_size;
        usize::try_from(used).is_ok_and(|used| used.saturating_add(SLACK) >= self.limit)
    }

    fn run(&mut self, call: &HookCall) -> Result<Json, Thrown> {
        if call.modules.get(&call.hook.file).is_none() {
            return Err(Thrown::Missing);
        }
        let key = key(call);
        if !self.entries.contains_key(&key) && self.entries.len() >= MAX_ENTRIES {
            self.entries.clear();
        }
        let loaded = match self.entries.entry(key) {
            Entry::Occupied(found) => found.into_mut(),
            Entry::Vacant(slot) => slot.insert(Loaded::load(&self.runtime, call, &self.state)?),
        };
        loaded.call(call)
    }
}

impl Loaded {
    /// A new context with `secret(name)`, and `call`'s entry module evaluated in it.
    fn load(runtime: &Runtime, call: &HookCall, state: &Rc<CallState>) -> Result<Self, Thrown> {
        let context = Context::full(runtime).map_err(|error| Thrown::Other(error.to_string()))?;
        let namespace =
            context.with(|ctx| evaluate(&ctx, call, Rc::clone(state)).map_err(|fault| Thrown::read(&ctx, fault)))?;
        Ok(Self { namespace, context })
    }

    /// Calls the export with the input, awaiting a returned promise.
    fn call(&self, call: &HookCall) -> Result<Json, Thrown> {
        self.context.with(|ctx| invoke(&ctx, self.namespace.clone(), call).map_err(|fault| Thrown::read(&ctx, fault)))
    }
}

fn evaluate(ctx: &Ctx<'_>, call: &HookCall, state: Rc<CallState>) -> Result<Persistent<Object<'static>>, Fault> {
    install_secret(ctx, state)?;
    let js = call.modules.get(&call.hook.file).map(|module| module.js.clone()).ok_or(Fault::Missing)?;
    let (module, evaluated) = Module::declare(ctx.clone(), call.hook.file.as_str(), js)?.eval()?;
    evaluated.finish::<()>()?;
    Ok(Persistent::save(ctx, module.namespace()?))
}

/// The global `secret(name)`, reading the running call's secrets.
fn install_secret(ctx: &Ctx<'_>, state: Rc<CallState>) -> rquickjs::Result<()> {
    let secret = Function::new(ctx.clone(), move |ctx: Ctx<'_>, name: String| {
        state.secret(&name).ok_or_else(|| Exception::throw_reference(&ctx, &format!("no secret `{name}`")))
    })?;
    ctx.globals().set("secret", secret)
}

fn invoke(ctx: &Ctx<'_>, namespace: Persistent<Object<'static>>, call: &HookCall) -> Result<Json, Fault> {
    let namespace = namespace.restore(ctx)?;
    let export: Value = namespace.get(call.hook.export.as_str())?;
    let function = export.into_function().ok_or(Fault::Missing)?;
    let returned: MaybePromise = function.call((to_js(ctx, &call.input)?,))?;
    let value: Value = returned.finish()?;
    to_json(&value)
}

/// The entry's context key: its path and the text of every module it may import.
fn key(call: &HookCall) -> u64 {
    let mut hasher = DefaultHasher::new();
    call.hook.file.hash(&mut hasher);
    for (path, module) in call.modules.iter() {
        (path, &module.js).hash(&mut hasher);
    }
    hasher.finish()
}
