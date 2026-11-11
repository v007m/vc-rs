//! What a spot reads: vars set at some level for every profile (V2101), pipes that fit the
//! var's type (V2304), and step ids that ran earlier in the same test or block (V2105).

use std::collections::{HashMap, HashSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{Matcher, MatcherArg, Namespace, Reference, Segment, SourceSpan};
use vc_template::{PipeRegistry, Roots};

use super::layer::Scope;
use super::project::{self, Project, Source};
use crate::parts::Piece;
use crate::suggest;

/// The part of a file a check covers: a span minus some holes, and where it was reached from.
pub struct Spot<'s, 'a> {
    /// The file.
    pub source: &'s Source<'a>,
    /// The span checked.
    pub span: SourceSpan,
    /// Spans inside it checked elsewhere.
    pub holes: &'s [SourceSpan],
    /// The call that reached an api's templates.
    pub site: Option<SourceSpan>,
}

/// Checks the references of every spot it is given, reporting each one at most once.
pub struct Checker<'p, 'a> {
    /// The project.
    pub project: &'p Project<'a>,
    /// The profiles every var is looked up for; one unnamed profile when there are none.
    pub profiles: Vec<String>,
    pipes: PipeRegistry,
    /// Where each reference reported is in `found`.
    reported: HashMap<SourceSpan, usize>,
    /// What the references were found to get wrong.
    found: Vec<Diagnostic>,
    /// What else was found.
    pub diagnostics: Diagnostics,
}

impl<'p, 'a> Checker<'p, 'a> {
    /// A checker over the config's profiles.
    pub fn new(project: &'p Project<'a>, profiles: Vec<String>) -> Self {
        let profiles = if profiles.is_empty() { vec![String::new()] } else { profiles };
        let pipes = PipeRegistry::builtin();
        Self { project, profiles, pipes, reported: HashMap::new(), found: Vec::new(), diagnostics: Diagnostics::new() }
    }

    /// Checks every reference in `spot` against `scope`.
    pub fn check(&mut self, spot: &Spot<'_, '_>, scope: &Scope<'_>) {
        let mut found = Vec::new();
        for piece in spot.source.pieces(spot.span, spot.holes) {
            piece_refs(piece, &mut found);
        }
        for reference in found {
            let diagnostic = match reference.ns {
                Namespace::Var => self.var(spot, reference, scope),
                Namespace::Step(id) => self.step((spot.source, reference), id, scope),
                _ => None,
            };
            if let Some(diagnostic) = diagnostic {
                self.report(reference.span, diagnostic);
            }
        }
    }

    /// Everything found.
    pub fn finish(mut self) -> Diagnostics {
        self.diagnostics.extend(self.found);
        self.diagnostics
    }

    /// Keeps `diagnostic`, or, when one was already reported at `span`, adds its call sites to
    /// that one: an api's unset var names every call that reaches it, so `vc check` on any of
    /// those test files reports it.
    fn report(&mut self, span: SourceSpan, diagnostic: Diagnostic) {
        let Some(&at) = self.reported.get(&span) else {
            self.reported.insert(span, self.found.len());
            self.found.push(diagnostic);
            return;
        };
        let Some(first) = self.found.get_mut(at).filter(|first| first.code == diagnostic.code) else { return };
        let sites: Vec<_> = diagnostic.secondary.into_iter().filter(|site| !first.secondary.contains(site)).collect();
        first.secondary.extend(sites);
    }

    /// V2105 for a step id that did not run earlier: "not available here" when an action in the
    /// file or the config declares it, else "not a root or a step id".
    fn step(
        &self,
        (source, reference): (&Source<'_>, &Reference),
        id: vc_model::Sym,
        scope: &Scope<'_>,
    ) -> Option<Diagnostic> {
        let name = self.project.strings.resolve(id)?;
        if scope.ran(name) {
            return None;
        }
        let unavailable = *Roots::new().check(reference, self.project.strings).err()?;
        let mut declared = std::iter::once(source).chain(self.project.config()).flat_map(project::actions);
        if declared.any(|action| project::text(action, "id") == Some(name)) {
            return Some(unavailable);
        }
        Some(Roots::undeclared(reference, name, &scope.steps).unwrap_or(unavailable))
    }

    fn var(&self, spot: &Spot<'_, '_>, reference: &Reference, scope: &Scope<'_>) -> Option<Diagnostic> {
        let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
        let missing: Vec<&str> =
            self.profiles.iter().map(String::as_str).filter(|profile| !scope.sets(name, profile)).collect();
        if !missing.is_empty() {
            return (!self.takes_missing(reference)).then(|| self.unset(spot, (reference, name), (&missing, scope)));
        }
        let ty = scope.ty(name).filter(|_| reference.path.segments.len() == 1 && !reference.pipes.is_empty())?;
        let problem = self.pipes.check_chain(&reference.pipes, ty).err()?;
        (problem.code == Code::V2304).then_some(*problem)
    }

    /// The pipes templates may call.
    pub const fn pipes(&self) -> &PipeRegistry {
        &self.pipes
    }

    /// Whether the reference's first pipe also runs on a missing value (`default`), so an
    /// unset var is not an error.
    pub fn takes_missing(&self, reference: &Reference) -> bool {
        let first = reference.pipes.first().and_then(|call| self.pipes.get(&call.name));
        first.is_some_and(|pipe| pipe.takes_missing)
    }

    /// V2101: `name` is set at no level for the `missing` profiles.
    fn unset(
        &self,
        spot: &Spot<'_, '_>,
        (reference, name): (&Reference, &str),
        (missing, scope): (&[&str], &Scope<'_>),
    ) -> Diagnostic {
        let message = format!("${{var.{name}}} is used but never set");
        let diagnostic = Diagnostic::new(Code::V2101, reference.span, message).label(self.label(name, missing));
        let set: Vec<&str> =
            self.profiles.iter().map(String::as_str).filter(|profile| !missing.contains(profile)).collect();
        let diagnostic = if set.is_empty() {
            diagnostic
        } else {
            diagnostic.note(format!("profile {} sets it; {} doesn't", set.join(", "), missing.join(", ")))
        };
        let diagnostic = match spot.site {
            Some(site) => diagnostic.secondary(site, "called from here"),
            None => diagnostic,
        };
        let first = missing.first().copied().unwrap_or_default();
        let place =
            if set.is_empty() { "in the config's vars".to_owned() } else { format!("under profiles.{first}.vars") };
        let diagnostic = diagnostic.help(format!("set it in this test's vars, in the file's vars, or {place}"));
        did_you_mean(diagnostic, (spot.source, reference, name), &scope.names(first))
    }

    fn label(&self, name: &str, missing: &[&str]) -> String {
        match missing {
            [""] => format!("no level sets {name}"),
            all if all.len() == self.profiles.len() && all.len() > 1 => format!("no level sets {name} for any profile"),
            some => format!("no level sets {name} for profile {}", some.join(", ")),
        }
    }
}

/// Offers the known names close to the name: one as a certain fix when the name's span is exact.
pub fn did_you_mean(
    diagnostic: Diagnostic,
    (source, reference, name): (&Source<'_>, &Reference, &str),
    known: &[&str],
) -> Diagnostic {
    offer(diagnostic, (source, reference, name), &suggest::close(name, known.iter().copied()))
}

/// Offers the `close` names: one as a certain fix when the name's span is exact, several as help.
pub fn offer(
    diagnostic: Diagnostic,
    (source, reference, name): (&Source<'_>, &Reference, &str),
    close: &[&str],
) -> Diagnostic {
    let one = match *close {
        [] => return diagnostic,
        [one] => one,
        ref several => return diagnostic.help(format!("did you mean one of {}?", suggest::quoted(several))),
    };
    let diagnostic = diagnostic.help(format!("did you mean `{one}`?"));
    let at = reference.root_span.end;
    let exact = source.text.get(at.offset..=at.offset) == Some(".");
    let Some(range) = source.word_at(at.offset + 1, name).filter(|_| exact) else { return diagnostic };
    let edit = TextEdit { file: source.file, range, new_text: one.to_owned() };
    diagnostic.fix(Fix::certain(format!("Replace with {one}"), vec![edit]))
}

/// Every reference to `ns` in `source` outside CEL, once per spot, in file order.
pub fn references<'a>(source: &Source<'a>, ns: Namespace) -> Vec<&'a Reference> {
    let mut found = Vec::new();
    let pieces = source.pieces(source.root.span, &[]).filter(|piece| !matches!(piece, Piece::Cel(_)));
    pieces.for_each(|piece| piece_refs(piece, &mut found));
    let mut seen = HashSet::new();
    found.retain(|reference| reference.ns == ns && seen.insert(reference.span));
    found
}

/// Every reference in a piece, in order.
pub fn piece_refs<'p>(piece: &'p Piece, found: &mut Vec<&'p Reference>) {
    match piece {
        Piece::Template(template) => found.extend(template.references()),
        Piece::Value(value) => value.collect_references(found),
        Piece::Matcher(matcher) => matcher_refs(matcher, found),
        Piece::Cel(cel) => found.extend(&cel.refs),
        Piece::Duration(_) | Piece::Size(_) => {}
    }
}

fn matcher_refs<'p>(matcher: &'p Matcher, found: &mut Vec<&'p Reference>) {
    match matcher {
        Matcher::Equals(value) | Matcher::StatusOf(value) => value.collect_references(found),
        Matcher::All(calls) => calls.iter().for_each(|call| arg_refs(&call.arg, found)),
        Matcher::Status(_) => {}
    }
}

fn arg_refs<'p>(arg: &'p MatcherArg, found: &mut Vec<&'p Reference>) {
    match arg {
        MatcherArg::Value(value) => value.collect_references(found),
        MatcherArg::Range(pair) => pair.iter().for_each(|arg| arg_refs(arg, found)),
        MatcherArg::Matcher(matcher) => matcher_refs(matcher, found),
        MatcherArg::Matchers(matchers) => matchers.iter().for_each(|matcher| matcher_refs(matcher, found)),
        MatcherArg::Duration(_) | MatcherArg::Size(_) => {}
    }
}
