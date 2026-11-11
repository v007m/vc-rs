//! The lint stage: lifecycle rules (V3xxx) over a linked project whose program was built. Each
//! rule is a [`Lint`] in a file of its own, listed once in [`LintRegistry::builtin`].

mod body_on_read;
#[cfg(test)]
mod convention_tests;
mod default_profiles;
mod deprecated_calls;
mod expired_skips;
mod header_names;
mod item_ids;
mod load_ranges;
mod lone_keys;
mod names;
mod placeholders;
mod reuse_keys;
mod shared_writes;
mod skip_reasons;
mod test_cache;
#[cfg(test)]
mod tests;
mod unused_defaults;
mod urls;

use std::ops::Range;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{Date, FileKind, Program};
use vc_parser::{Entry, Node};

use crate::link;
use crate::link::{Project, Source};

/// What a lint reads: the project's trees and the program built from them.
pub struct LintCx<'p, 'a> {
    /// The linked files.
    pub project: &'p Project<'a>,
    /// The program built from them.
    pub program: &'p Program,
    /// Today's date in UTC, when the caller knows it.
    pub today: Option<Date>,
}

/// One lifecycle rule.
pub trait Lint {
    /// The code of every diagnostic it reports.
    fn code(&self) -> Code;
    /// What it finds.
    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic>;
}

/// The lints the stage runs, in order.
pub struct LintRegistry {
    lints: Vec<Box<dyn Lint>>,
}

impl LintRegistry {
    /// Every built-in lint.
    pub fn builtin() -> Self {
        let lints: Vec<Box<dyn Lint>> = vec![
            Box::new(default_profiles::DefaultProfiles),
            Box::new(skip_reasons::SkipReasons),
            Box::new(expired_skips::ExpiredSkips),
            Box::new(shared_writes::SharedWrites),
            Box::new(test_cache::TestCache),
            Box::new(deprecated_calls::DeprecatedCalls),
            Box::new(body_on_read::BodyOnRead),
            Box::new(unused_defaults::UnusedDefaults),
            Box::new(reuse_keys::ReuseKeys),
            Box::new(lone_keys::LoneKeys),
            Box::new(load_ranges::LoadRanges),
            Box::new(header_names::HeaderNames),
        ];
        Self { lints }
    }

    /// The naming-convention lints (V4, D180): warnings, each a registry entry like the rest.
    pub fn conventions() -> Self {
        let lints: Vec<Box<dyn Lint>> = vec![
            Box::new(item_ids::TestIds),
            Box::new(item_ids::ApiIds),
            Box::new(names::FileNames),
            Box::new(names::Labels),
            Box::new(urls::ApiUrls),
            Box::new(placeholders::PathPlaceholders),
        ];
        Self { lints }
    }

    /// Runs every lint over `cx`.
    pub fn run(&self, cx: &LintCx<'_, '_>) -> Diagnostics {
        self.lints.iter().flat_map(|lint| lint.check(cx)).collect()
    }
}

impl<'a> LintCx<'_, 'a> {
    /// Every test file.
    fn test_files(&self) -> impl Iterator<Item = &Source<'a>> {
        self.project.sources.iter().filter(|source| source.kind == FileKind::Test)
    }

    /// Every test, with its file.
    fn tests(&self) -> impl Iterator<Item = (&Source<'a>, &'a Node)> {
        self.test_files().flat_map(|source| link::list(source.root, "tests").iter().map(move |test| (source, test)))
    }

    /// The api `node`'s `call` names, written in `source`.
    fn api(&self, source: &Source<'_>, node: &Node) -> Option<&'a Node> {
        let call = link::text(node, "call")?;
        self.project.apis.get(&source.resolve(call)).map(|item| item.node)
    }
}

/// The lines `entry` is written on, newline included, when nothing else shares its first line.
fn whole_lines(text: &str, entry: &Entry) -> Option<Range<usize>> {
    let start = entry.key_span.start.offset;
    let line = text.get(..start)?.rfind('\n').map_or(0, |at| at + 1);
    if !text.get(line..start)?.trim().is_empty() {
        return None;
    }
    let end = entry.value.span.end.offset;
    let after = text.get(end..)?.find('\n').map_or(text.len(), |at| end + at + 1);
    Some(line..after)
}
