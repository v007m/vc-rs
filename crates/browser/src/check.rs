//! `vc check` in the playground: every diagnostic of the project, then the summary line; exit 2
//! when one is an error.

use vc_adapter_system::SystemClock;
use vc_interfaces::{Clock as _, MemorySources};

use crate::compile::{self, Project};
use crate::error::Checked;
use crate::{BrowserError, Sink, print};

/// What the CLI's `vc check` checks that the playground leaves out.
const NOT_CHECKED: &str = "the playground does not check the alert channels in your user config";

/// Checks `project` and prints the report; the exit code.
pub fn check(project: &MemorySources, sink: &dyn Sink, color: bool) -> Result<u8, BrowserError> {
    let Project { program, diagnostics, files } = compile::compile(project, SystemClock.today())?;
    if program.as_ref().is_some_and(|program| !program.config.alerts.is_empty()) {
        sink.line(crate::Stream::Err, &format!("warning: {NOT_CHECKED}"));
    }
    let errors = diagnostics.has_errors();
    print::diagnostics(sink, &Checked { diagnostics, files }, color)?;
    Ok(if errors { 2 } else { 0 })
}
