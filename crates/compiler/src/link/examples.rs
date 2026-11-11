//! What an `examples:` template reads through its vars. The template is rendered before the run
//! starts, so every var it reads, and every var those read in turn, must be defined in `vars`
//! from `var`, `env` and `dataset` alone: a var only a save sets, or one that reads a secret,
//! `run` or a response, has no value then (V2105 at `examples`). The vars are followed as the
//! engine renders the rows: from the test's vars out to the config's, and a definition that reads
//! its own name reads the next level out. Only the definitions every profile shares are followed
//! here; the engine checks the active profiles' ones before it renders the rows.

use std::collections::HashSet;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Namespace, Reference, Segment};
use vc_parser::{Node, Value};
use vc_template::{ROW_ROOTS, row_root_label, row_var_unread};

use super::layer::{Origin, Scope};
use super::project::Source;
use super::uses::{Checker, piece_refs};

/// What a var only a save sets needs.
const SAVED: &str = "a save, which runs after it is read";

/// The message of V2105 when the `$file` path of `examples` holds a template.
const FILE_TEMPLATED: &str = "the `$file` path of `examples` holds a template";

/// Its help.
const FILE_HELP: &str = "`examples` is read before the run starts, from a path written as it is; to choose \
                         the rows by profile, read them from a var or a dataset: `examples: ${var.rows}`";

/// A var the rows read, and what it needs that is not there before the run.
struct Needed {
    var: String,
    root: String,
}

/// The vars already followed, each with the level its lookup starts at.
type Seen = HashSet<(String, usize)>;

impl Checker<'_, '_> {
    /// V2105 at `test`'s `examples`, when it is a `$file` whose path holds a template, or when it
    /// is one template or a `$file` and a var it reads, directly or through the vars that one
    /// reads, has no value before the run.
    pub fn examples(&mut self, source: &Source<'_>, test: &Node, scope: &Scope<'_>) {
        let Some(rows) = test.get("examples").filter(|rows| !matches!(rows.value, Value::Seq(_))) else { return };
        let mut found = Vec::new();
        source.pieces(rows.span, &[]).for_each(|piece| piece_refs(piece, &mut found));
        if is_file(rows) && !found.is_empty() {
            let diagnostic = Diagnostic::new(Code::V2105, rows.span, FILE_TEMPLATED).help(FILE_HELP);
            self.diagnostics.push(diagnostic);
            return;
        }
        let mut seen = Seen::new();
        let needed = found.into_iter().find_map(|reference| {
            let needed = self.needed((reference, 0), scope, &mut seen)?;
            Some((var_name(reference)?.to_owned(), needed))
        });
        let Some((read, Needed { var, root })) = needed else { return };
        let (message, help) = row_var_unread(&read, &var, &root);
        self.diagnostics.push(Diagnostic::new(Code::V2105, rows.span, message).help(help));
    }

    /// What the var `reference` names needs that is not there before the run, looked up from the
    /// level `from` out and followed through the vars its definition reads; `None` for another
    /// root, one no level sets (V2101 says so), one only a save sets read with `default`, which
    /// then gives its default, or one already followed. A defined var read with `default` is
    /// still followed, since the rows render its definition.
    fn needed(&self, (reference, from): (&Reference, usize), scope: &Scope<'_>, seen: &mut Seen) -> Option<Needed> {
        if reference.ns != Namespace::Var {
            return None;
        }
        let name = var_name(reference)?;
        if !seen.insert((name.to_owned(), from)) {
            return None;
        }
        match scope.origin(name, from) {
            Origin::Unset => None,
            Origin::Saved if self.takes_missing(reference) => None,
            Origin::Saved => Some(Needed { var: name.to_owned(), root: SAVED.to_owned() }),
            Origin::Defined { reads, at } => self.defined((name, reads, at), scope, seen),
        }
    }

    /// What the definition of the var `name` at the level `at`, reading `reads`, needs that is
    /// not there before the run: a root it reads itself, else what a var it reads needs. A read
    /// of `name` itself starts at the next level out, any other name at the innermost, as the
    /// engine looks them up.
    fn defined(
        &self,
        (name, reads, at): (&str, &[Reference], usize),
        scope: &Scope<'_>,
        seen: &mut Seen,
    ) -> Option<Needed> {
        let outside = reads.iter().find(|read| !ROW_ROOTS.contains(&read.ns));
        if let Some(read) = outside {
            return Some(Needed { var: name.to_owned(), root: row_root_label(read.ns) });
        }
        reads.iter().find_map(|read| {
            let from = if var_name(read) == Some(name) { at + 1 } else { 0 };
            self.needed((read, from), scope, seen)
        })
    }
}

/// Whether `rows` is a `$file` map, whose path is all it holds.
fn is_file(rows: &Node) -> bool {
    matches!(rows.entries(), [entry] if entry.key == "$file")
}

/// The var a `${var.…}` reference names.
fn var_name(reference: &Reference) -> Option<&str> {
    let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
    Some(name.as_str())
}

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;
