//! V2302: load values that are each well formed but do not fit together: no stages, or a
//! `thinkTime` whose `min` is longer than its `max`.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::FileKind;

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;

/// Reports load files with no stage, or a pause range that runs backwards.
pub struct LoadRanges;

impl Lint for LoadRanges {
    fn code(&self) -> Code {
        Code::V2302
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let loads = cx.project.sources.iter().filter(|source| source.kind == FileKind::Load);
        loads.flat_map(|source| self.stages(source).into_iter().chain(self.think_time(source))).collect()
    }
}

impl LoadRanges {
    /// `stages: []`: the run would send no traffic.
    fn stages(&self, source: &Source<'_>) -> Option<Diagnostic> {
        let stages = source.root.get("stages").filter(|stages| stages.items().is_empty())?;
        let diagnostic = Diagnostic::new(self.code(), stages.span, "`stages` needs at least one stage");
        let diagnostic = diagnostic.label("no stage");
        Some(diagnostic.help("add a stage: `stages: [{duration: 1m, users: 10}]`"))
    }

    /// `thinkTime: {min: 2s, max: 1s}`.
    fn think_time(&self, source: &Source<'_>) -> Option<Diagnostic> {
        let range = source.root.get("thinkTime")?;
        let (min, max) = (link::duration(source, range, "min")?, link::duration(source, range, "max")?);
        if min <= max {
            return None;
        }
        let span = range.get("min").map_or(range.span, |node| node.span);
        let diagnostic = Diagnostic::new(self.code(), span, "`thinkTime.min` is longer than `thinkTime.max`");
        let diagnostic = diagnostic.label("longer than `max`");
        Some(diagnostic.help("swap them, or write one duration for a fixed pause: `thinkTime: 1s`"))
    }
}
