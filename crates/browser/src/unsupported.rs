//! What a page cannot hold, said plainly: a project that needs secrets is refused, and plugins,
//! alerts, report files and the cross-run cache are left out with a notice. Hooks and
//! `command:` actions are refused when the run starts (`run.rs`).

use vc_model::Program;

use crate::BrowserError;

/// Run the project with `vc run` for what the playground leaves out.
const NATIVE: &str = "run the project with `vc run` on your machine";

/// Refuses a project that declares secrets: the page has no keychain or `.env` to load them from.
pub fn refuse(program: &Program) -> Result<(), BrowserError> {
    let declared = &program.config.secrets.declared;
    if declared.is_empty() {
        return Ok(());
    }
    let reason = format!("the project declares secrets ({}), which the playground cannot load", declared.join(", "));
    Err(BrowserError::refused(reason, Some(format!("leave `${{secret.*}}` out of the project, or {NATIVE}"))))
}

/// A notice for each part of `program`'s config the run leaves out.
pub fn notices(program: &Program) -> Vec<String> {
    let config = &program.config;
    let mut notices = Vec::new();
    if !config.plugins.is_empty() {
        let names: Vec<&str> = config.plugins.iter().filter_map(|plugin| program.str(plugin.id)).collect();
        notices.push(format!("the playground loads no plugins, so {} are left out; {NATIVE}", names.join(", ")));
    }
    if !config.alerts.is_empty() {
        notices.push(format!("the playground sends no alerts; {NATIVE}"));
    }
    if !config.report_formats.is_empty() {
        let formats = config.report_formats.join(", ");
        notices.push(format!("the playground writes no report files, so the {formats} reports are left out"));
    }
    if program.actions.iter().any(|action| action.cache.is_some()) {
        notices.push("the playground keeps no cache; `cache:` actions run every time this run".to_owned());
    }
    notices
}
