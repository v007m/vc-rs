//! The config's `schedules` (D520, D700): each `cron` is five fields the scheduler and GitHub
//! Actions both read (V2317). Their IDs are checked with the config's other IDs (V2001) and
//! their profiles with every other profile reference (V2002).

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::Cron;
use vc_parser::Node;

use super::project;
use super::project::Project;

/// Checks the config's `schedules`.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let Some(config) = project.config() else { return Diagnostics::default() };
    project::schedules(config.root).iter().filter_map(|schedule| schedule.get("cron")).filter_map(cron).collect()
}

/// V2317 when the written `cron` is not five cron fields, labelled with what is wrong.
fn cron(node: &Node) -> Option<Diagnostic> {
    let written = node.as_str()?;
    let reason = Cron::parse(written).err()?;
    let help = "write minute, hour, day of the month, month and day of the week in UTC, such as `0 2 * * 1-5` \
                (02:00 on weekdays); each is `*`, a number, `a-b`, `*/n` or a list joined with `,`";
    let diagnostic = Diagnostic::new(Code::V2317, node.span, format!("`{written}` is not a cron")).label(reason);
    Some(diagnostic.help(help))
}
