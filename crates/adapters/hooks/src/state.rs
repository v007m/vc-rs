//! What the running call shares with `QuickJS`'s callbacks on the hook thread: its modules for
//! the module loader, its secrets for `secret(name)`, and the names it read.

use std::cell::Cell;

use vc_interfaces::{HookCall, Secrets};
use vc_model::HookModules;

/// The running call's modules and secrets. One per runtime; `Cell`s, since only the hook thread
/// touches it and never while a borrow is held.
#[derive(Default)]
pub struct CallState {
    modules: Cell<HookModules>,
    secrets: Cell<Secrets>,
    used: Cell<Vec<String>>,
}

impl CallState {
    /// Starts `call`: its modules and secrets, and no secrets read yet.
    pub fn begin(&self, call: &HookCall) {
        self.modules.set(call.modules.clone());
        self.secrets.set(call.secrets.clone());
        self.used.set(Vec::new());
    }

    /// Ends the call, dropping its secrets; the names it read, in the order first read.
    pub fn end(&self) -> Vec<String> {
        self.secrets.set(Secrets::new());
        self.used.take()
    }

    /// The JavaScript of the module at `path`.
    pub fn module_js(&self, path: &str) -> Option<String> {
        let modules = self.modules.take();
        let js = modules.get(path).map(|module| module.js.clone());
        self.modules.set(modules);
        js
    }

    /// Whether there is a module at `path`.
    pub fn has_module(&self, path: &str) -> bool {
        let modules = self.modules.take();
        let found = modules.get(path).is_some();
        self.modules.set(modules);
        found
    }

    /// The secret called `name`, noting that the call read it.
    pub fn secret(&self, name: &str) -> Option<String> {
        let secrets = self.secrets.take();
        let value = secrets.get(name).map(|secret| secret.expose().to_owned());
        self.secrets.set(secrets);
        if value.is_some() {
            let mut used = self.used.take();
            if !used.iter().any(|seen| seen == name) {
                used.push(name.to_owned());
            }
            self.used.set(used);
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use vc_interfaces::{HookCall, Secret, Secrets};
    use vc_model::{HookModule, HookModules, HookRef};

    use super::CallState;

    #[test]
    fn records_each_secret_read_once_and_forgets_them_at_the_end() {
        let module = HookModule { js: "export {}".into(), lines: vec![1] };
        let modules = HookModules::new(BTreeMap::from([("hooks/a.ts".to_owned(), module)]));
        let mut secrets = Secrets::new();
        secrets.insert("KEY", Secret::new("v"));
        let hook = HookRef { file: "hooks/a.ts".into(), export: "a".into() };
        let state = CallState::default();
        state.begin(&HookCall::new(modules, hook, Value::Null).with_secrets(secrets));
        assert!(state.has_module("hooks/a.ts") && !state.has_module("hooks/b.ts"));
        assert_eq!(state.module_js("hooks/a.ts").as_deref(), Some("export {}"));
        assert_eq!((state.secret("KEY").as_deref(), state.secret("KEY").as_deref()), (Some("v"), Some("v")));
        assert_eq!(state.secret("NOPE"), None);
        assert_eq!(state.end(), ["KEY"]);
        assert_eq!(state.secret("KEY"), None, "the secrets end with the call");
    }
}
