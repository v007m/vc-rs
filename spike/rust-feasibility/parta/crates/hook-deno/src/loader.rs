//! Module loader that serves source maps for stripped hooks and refuses every import.
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use deno_core::{ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse, ModuleLoader, ModuleResolveResponse, ModuleSpecifier, ResolutionKind};
use deno_error::JsErrorBox;

/// Shared with the engine, which inserts maps after the loader is handed to V8.
pub type SourceMaps = Rc<RefCell<HashMap<String, Vec<u8>>>>;

pub struct HookLoader {
    pub maps: SourceMaps,
}

impl ModuleLoader for HookLoader {
    fn resolve(&self, specifier: &str, _referrer: &str, _kind: ResolutionKind) -> ModuleResolveResponse {
        // Only the hook files the engine itself registered resolve; everything else is denied.
        if self.maps.borrow().contains_key(specifier) {
            return ModuleSpecifier::parse(specifier).map_err(JsErrorBox::from_err);
        }
        Err(JsErrorBox::generic(format!("imports are disabled in hooks: '{specifier}'")))
    }

    fn load(&self, specifier: &ModuleSpecifier, _referrer: Option<&ModuleLoadReferrer>, _options: ModuleLoadOptions) -> ModuleLoadResponse {
        ModuleLoadResponse::Sync(Err(JsErrorBox::generic(format!("imports are disabled in hooks: '{specifier}'"))))
    }

    fn get_source_map(&self, file_name: &str) -> Option<Cow<'_, [u8]>> {
        self.maps.borrow().get(file_name).map(|m| Cow::Owned(m.clone()))
    }
}
