//! Load files as [`LoadScenario`]s: the traffic mix, the stages and the thresholds.

use indexmap::IndexMap;
use vc_model::{Duration, FileId, LoadScenario, LoadStage, Matcher, MixEntry, TestId};
use vc_parser::Node;

use super::{Build, Built, read};
use crate::link::project;
use crate::link::project::Source;
use crate::parts::Piece;

impl<'a> Build<'_, 'a> {
    /// A load file; the link checks have already reported a `mix` test that does not exist (V2003).
    pub fn load_file(&mut self, source: &Source<'a>) -> Built<()> {
        let root = source.root;
        let id = FileId::new(self.sym(source.id.as_deref().unwrap_or_default())?);
        let mut mix = Vec::new();
        for entry in project::list(root, "mix") {
            let Some(test) = project::text(entry, "test") else { continue };
            mix.push(MixEntry {
                test: TestId::new(self.sym(&source.resolve(test))?),
                weight: read::number(entry, "weight").unwrap_or_default(),
                thresholds: thresholds(source, entry.get("thresholds")),
            });
        }
        self.builder.load(LoadScenario {
            id,
            name: read::owned(root, "name"),
            mix,
            stages: project::list(root, "stages").iter().filter_map(|node| stage(source, node)).collect(),
            think_time: think_time(source, root),
            full_checks: project::text(root, "checks") == Some("full"),
            thresholds: thresholds(source, root.get("thresholds")),
            abort_on: read::texts(root, "abortOn"),
        });
        Ok(())
    }
}

fn stage(source: &Source<'_>, node: &Node) -> Option<LoadStage> {
    Some(LoadStage { duration: read::duration(source, node, "duration")?, users: read::number(node, "users")? })
}

/// `thinkTime`: one duration for both ends, or `min` and `max`; no pause when left out.
fn think_time(source: &Source<'_>, root: &Node) -> (Duration, Duration) {
    if let Some(fixed) = read::duration(source, root, "thinkTime") {
        return (fixed, fixed);
    }
    let zero = Duration::new(std::time::Duration::ZERO);
    let Some(range) = root.get("thinkTime") else { return (zero, zero) };
    let min = read::duration(source, range, "min").unwrap_or(zero);
    (min, read::duration(source, range, "max").unwrap_or(min))
}

/// The matchers of a `thresholds` map, by metric.
fn thresholds(source: &Source<'_>, node: Option<&Node>) -> IndexMap<String, Matcher> {
    let entries = node.map(Node::entries).unwrap_or_default();
    let matcher = |node: &Node| match source.piece(node)? {
        Piece::Matcher(matcher) => Some(matcher.clone()),
        _ => None,
    };
    entries.iter().filter_map(|entry| Some((entry.key.clone(), matcher(&entry.value)?))).collect()
}
