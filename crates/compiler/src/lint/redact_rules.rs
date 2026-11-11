//! V2311: a `redact.rules` `regex` that does not compile. A run's redactor fails closed on it
//! and masks every text, so `vc check` reports it first.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;

/// Reports each redact `regex` the run's regex engine rejects.
pub struct RedactRules;

impl Lint for RedactRules {
    fn code(&self) -> Code {
        Code::V2311
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(config) = cx.project.config() else { return Vec::new() };
        let rules = config.root.get("redact").map(|redact| link::list(redact, "rules")).unwrap_or_default();
        rules.iter().filter_map(|rule| rule.get("regex")).filter_map(|regex| self.broken(regex)).collect()
    }
}

impl RedactRules {
    /// V2311 at a pattern that does not compile, with the regex engine's reason.
    fn broken(&self, node: &Node) -> Option<Diagnostic> {
        let error = regex::Regex::new(node.as_str()?).err()?;
        let reason = error.to_string().lines().last().unwrap_or_default().trim().to_owned();
        let diagnostic = Diagnostic::new(self.code(), node.span, format!("redact pattern does not compile: {reason}"))
            .label("not a regex")
            .note("a run masks every text while a rule fails, so reports and history show only `••••`")
            .help("fix the pattern; escape a character it should match as itself with `\\`");
        Some(diagnostic)
    }
}
