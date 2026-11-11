//! The first var an `examples:` template reads that needs, itself or through the vars it reads,
//! a root with no value before the run (V2105). The compiler checks the definitions without a
//! profile; this checks the ones the run's profiles give, in the scope the rows render in.

use std::collections::BTreeSet;

use vc_model::{Examples, Namespace, Program, Reference, Segment, SourceSpan, Test};
use vc_template::{PipeRegistry, row_readable};

use super::run::RunValues;
use super::scope::{Found, Scope};

/// A var `examples:` reads whose definition, or one it reads in turn, needs a root with no
/// value before the run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnreadVar {
    /// The var `examples` reads: `picked` in `${var.picked}`.
    pub read: String,
    /// The var whose definition reads `root`: `read` itself, or a var it reads in turn.
    pub var: String,
    /// The root that definition reads.
    pub root: Namespace,
    /// The `${var.<read>}` in `examples`.
    pub span: SourceSpan,
}

/// The first var `test`'s `examples:` template reads that needs a root other than `var`, `env`
/// and `dataset`, following each name as the rows' render would (the test's vars, its file's,
/// then the config's, with the run's profiles applied); `None` when every var it reads renders
/// before the run, or it has no template.
#[must_use]
pub fn unread_var(program: &Program, values: &RunValues, test: &Test) -> Option<UnreadVar> {
    let Some(Examples::Template(template)) = &test.examples else { return None };
    let walk = Walk { program, scope: Scope::of_test(values, test), pipes: values.pipes() };
    template.references().find_map(|reference| {
        let read = var_name(reference)?;
        let (var, root) = walk.needs((read, 0), &mut BTreeSet::new())?;
        Some(UnreadVar { read: read.to_owned(), var, root, span: reference.span })
    })
}

/// Follows var definitions through the scope the rows render in.
struct Walk<'p> {
    program: &'p Program,
    scope: Scope,
    pipes: &'p PipeRegistry,
}

impl Walk<'_> {
    /// The var and root the definition of `name` found from level `from` on needs: a root it
    /// reads itself, unless it is a secret read with `default`, else what a var it reads needs,
    /// as the compiler words it. A definition reading its own name continues past its level, as
    /// rendering does; a name not set, a saved value or a definition already followed needs
    /// nothing more.
    fn needs(&self, (name, from): (&str, usize), seen: &mut BTreeSet<(String, usize)>) -> Option<(String, Namespace)> {
        let (level, Found::Defined(def)) = self.scope.find(self.program, name, from)? else { return None };
        if !seen.insert((name.to_owned(), level)) {
            return None;
        }
        let reads = def.references();
        if let Some(outside) = reads.iter().find(|read| !row_readable(read, self.pipes)) {
            return Some((name.to_owned(), outside.ns));
        }
        let mut vars = reads.iter().copied().filter_map(var_name);
        vars.find_map(|next| self.needs((next, if next == name { level.saturating_add(1) } else { 0 }), seen))
    }
}

/// The var a `${var.<name>…}` reference reads.
fn var_name(reference: &Reference) -> Option<&str> {
    match (reference.ns, reference.path.segments.first()) {
        (Namespace::Var, Some(Segment::Key(name))) => Some(name),
        _ => None,
    }
}

#[cfg(test)]
#[path = "unread_tests.rs"]
mod tests;
