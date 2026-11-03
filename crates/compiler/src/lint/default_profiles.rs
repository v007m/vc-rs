//! V3102: more than one profile is marked `default: true`.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::{flag, list, text};
use crate::shapes;

/// Reports every default profile after the first.
pub struct DefaultProfiles;

impl Lint for DefaultProfiles {
    fn code(&self) -> Code {
        Code::V3102
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(config) = cx.project.config() else { return Vec::new() };
        let defaults: Vec<&Node> =
            list(config.root, "profiles").iter().filter(|node| flag(shapes::profile(), node, "default")).collect();
        let [first, rest @ ..] = &defaults[..] else { return Vec::new() };
        rest.iter().map(|profile| self.again(first, profile)).collect()
    }
}

impl DefaultProfiles {
    fn again(&self, first: &Node, profile: &Node) -> Diagnostic {
        let at = |node: &Node| node.entry("default").map_or(node.span, |entry| entry.value.span);
        let (name, first_name) = (text(profile, "id").unwrap_or_default(), text(first, "id").unwrap_or_default());
        let message = format!("profile `{name}` is a second default profile");
        Diagnostic::new(self.code(), at(profile), message)
            .label("also `default: true`")
            .secondary(at(first), format!("`{first_name}` is the default"))
            .help("mark one profile `default: true`; pick another for a run with `--profile`")
    }
}
