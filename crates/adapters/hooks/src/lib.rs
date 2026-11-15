//! Hook runner: TypeScript hooks, stripped to JavaScript by the compiler, run in `QuickJS`.
//!
//! [`QuickJsHooks`] implements `vc_interfaces::HookRunner`. Calls cross a channel to a dispatcher
//! thread, which hands each to an idle one of `hooks.threads` supervisor threads (D325); each
//! supervisor runs its calls one at a time on a hook thread that owns its own `QuickJS` runtime, so
//! module state is per runtime. Each entry module gets its own context, kept while its text is
//! unchanged; it may import `vibranium:std` (HMAC-SHA256, SHA-256, base64 and URL coding) and other
//! hook files by relative path, and read secrets through `secret(name)`. Time is enforced by the
//! interrupt handler and memory by the runtime's limit; a call stuck past its budget plus a grace
//! period is abandoned and the hook thread restarted. Errors carry the TypeScript line.

mod deadline;
mod dispatch;
mod engine;
mod fault;
mod loader;
mod marshal;
mod runner;
mod state;
mod stdlib;
mod worker;

pub use runner::QuickJsHooks;
