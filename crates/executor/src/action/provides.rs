//! `provides:` on `function:` and `test:` actions (D128): each declared name must be saved.

use vc_diagnostics::Code;
use vc_model::{Action, Failure, Program};

use crate::Saved;

/// V5109 naming what `action` declares in `provides` but did not save; `None` when it saved all.
pub fn unsaved(program: &Program, action: &Action, saved: &Saved) -> Option<Failure> {
    let names = action.provides.iter().filter_map(|name| program.str(name.sym()));
    let missing: Vec<String> =
        names.filter(|name| !saved.contains_key(*name)).map(|name| format!("`{name}`")).collect();
    if missing.is_empty() {
        return None;
    }
    let message = format!("declared in `provides` but not saved: {}", missing.join(", "));
    Some(Failure::new(message).with_code(Code::V5109.to_string()))
}
