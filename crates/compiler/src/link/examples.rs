//! What an `examples:` template reads through its vars. The template is rendered before the run
//! starts, so every var it reads, and every var those read in turn, must be defined in `vars`
//! from `var`, `env` and `dataset` alone: a var only a save sets, or one that reads a secret,
//! `run` or a response, has no value then (V2105 at `examples`).

use std::collections::HashSet;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Namespace, Reference, Segment};
use vc_parser::{Node, Value};

use super::layer::{Origin, Scope};
use super::project::Source;
use super::uses::{Checker, piece_refs};

/// The roots a value read before the run may read, as `examples` itself (V2105).
const BEFORE_RUN: &[Namespace] = &[Namespace::Var, Namespace::Env, Namespace::Dataset];

/// The second half of the help.
const WHY: &str =
    "`examples` is read before the run starts, so the vars it reads may read only `var`, `env` and `dataset`";

/// A var the rows read, and what it needs that is not there before the run.
struct Needed {
    var: String,
    root: String,
}

impl Checker<'_, '_> {
    /// V2105 at `test`'s `examples`, when it is one template or a `$file` and a var it reads,
    /// directly or through the vars that one reads, has no value before the run.
    pub fn examples(&mut self, source: &Source<'_>, test: &Node, scope: &Scope<'_>) {
        let Some(rows) = test.get("examples").filter(|rows| !matches!(rows.value, Value::Seq(_))) else { return };
        let mut found = Vec::new();
        source.pieces(rows.span, &[]).for_each(|piece| piece_refs(piece, &mut found));
        let mut seen = HashSet::new();
        let needed = found.into_iter().find_map(|reference| {
            let needed = self.needed(reference, scope, &mut seen)?;
            Some((var_name(reference)?.to_owned(), needed))
        });
        let Some((read, Needed { var, root })) = needed else { return };
        let message = format!("`examples` reads `var.{read}`, which has no value before the run");
        let help = format!("`var.{var}` needs {root}; {WHY}");
        self.diagnostics.push(Diagnostic::new(Code::V2105, rows.span, message).help(help));
    }

    /// What the var `reference` names needs that is not there before the run, followed through
    /// the vars its definition reads; `None` for another root, a var read with `default`, one
    /// no level sets (V2101 says so) or one already followed.
    fn needed(&self, reference: &Reference, scope: &Scope<'_>, seen: &mut HashSet<String>) -> Option<Needed> {
        if reference.ns != Namespace::Var || self.takes_missing(reference) {
            return None;
        }
        let name = var_name(reference)?;
        if !seen.insert(name.to_owned()) {
            return None;
        }
        match scope.origin(name) {
            Origin::Unset => None,
            Origin::Saved => {
                Some(Needed { var: name.to_owned(), root: "a save, which runs after it is read".to_owned() })
            }
            Origin::Defined(reads) => self.defined((name, reads), scope, seen),
        }
    }

    /// What the definition of the var `name`, reading `reads`, needs that is not there before
    /// the run: a root it reads itself, else what a var it reads needs.
    fn defined(
        &self,
        (name, reads): (&str, &[Reference]),
        scope: &Scope<'_>,
        seen: &mut HashSet<String>,
    ) -> Option<Needed> {
        let outside = reads.iter().find(|read| !BEFORE_RUN.contains(&read.ns));
        if let Some(read) = outside {
            return Some(Needed { var: name.to_owned(), root: root_label(read.ns) });
        }
        reads.iter().find_map(|read| self.needed(read, scope, seen))
    }
}

/// The var a `${var.…}` reference names.
fn var_name(reference: &Reference) -> Option<&str> {
    let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
    Some(name.as_str())
}

/// `` `secret` ``, or for an action id, that action's response.
fn root_label(ns: Namespace) -> String {
    vc_template::root_name(ns).map_or_else(|| "an action's response".to_owned(), |root| format!("`{root}`"))
}

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;
