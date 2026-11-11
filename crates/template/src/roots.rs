use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{Namespace, Reference, StringTable, Sym};

use crate::codes;
use crate::parse;

/// Roots available everywhere (D125).
const EVERYWHERE: &[Namespace] =
    &[Namespace::Var, Namespace::Env, Namespace::Secret, Namespace::Fake, Namespace::Dataset, Namespace::Run];

/// The code and the explanation for each root that is only sometimes available (D125, D137).
const LIMITED: &[(Namespace, Code, &str)] = &[
    (Namespace::Response, codes::ROOT_UNAVAILABLE, "once a call is made: in expect, save, poll and cleanup"),
    (Namespace::Request, codes::ROOT_UNAVAILABLE, "once a call is made: in expect, save, poll and cleanup"),
    (Namespace::Item, codes::LOOP_ROOT, "only inside forEach"),
    (Namespace::Iteration, codes::LOOP_ROOT, "only inside repeat, forEach or a repeated test"),
    (Namespace::Example, codes::LOOP_ROOT, "only in a test with examples"),
];

/// Which template and CEL roots are available at one spot in a file (D125).
///
/// `var`, `env`, `secret`, `fake`, `dataset` and `run` are available everywhere; the others
/// only where the builder methods add them. A spot read before the run starts takes only the
/// roots [`Roots::only`] names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots {
    added: Vec<Namespace>,
    only: Option<(&'static [Namespace], &'static str)>,
}

/// Parts of `response` written without it; a bare one is a step id unless reported (D150).
const RESPONSE_FIELDS: &[&str] = &["status", "headers", "body"];

impl Roots {
    /// Only the roots available everywhere.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// After a call: `response` and `request` (in `expect`, `save`, `poll`, an api's `cleanup`).
    #[must_use]
    pub fn with_call(self) -> Self {
        self.with(&[Namespace::Response, Namespace::Request])
    }

    /// Inside `forEach`: `item` and `iteration`.
    #[must_use]
    pub fn in_for_each(self) -> Self {
        self.with(&[Namespace::Item, Namespace::Iteration])
    }

    /// Inside `repeat` or a repeated test: `iteration`.
    #[must_use]
    pub fn in_loop(self) -> Self {
        self.with(&[Namespace::Iteration])
    }

    /// In a test with `examples`: `example`.
    #[must_use]
    pub fn with_examples(self) -> Self {
        self.with(&[Namespace::Example])
    }

    /// After the action or step `id`, in the same test or block.
    #[must_use]
    pub fn with_step(self, id: Sym) -> Self {
        self.with(&[Namespace::Step(id)])
    }

    /// Only `roots`, whatever else is added; any other root is V2105 with `why` as its help, such
    /// as "`examples` is read before the run starts, so it reads only `var`, `env` and `dataset`".
    #[must_use]
    pub fn only(self, roots: &'static [Namespace], why: &'static str) -> Self {
        Self { only: Some((roots, why)), ..self }
    }

    fn with(mut self, roots: &[Namespace]) -> Self {
        self.added.extend_from_slice(roots);
        self
    }

    /// Checks that the reference's root is available here.
    ///
    /// # Errors
    ///
    /// V2102 for `item`, `example` or `iteration` outside its block, V2105 for any other root
    /// not available here. A bare `status`, `headers` or `body` that is not an available step
    /// is V2105 with a certain fix to `response.…` (D150). Whether a step id names a step at all
    /// is the linker's check.
    pub fn check(&self, reference: &Reference, strings: &StringTable) -> Result<(), Box<Diagnostic>> {
        let ns = reference.ns;
        if let Some((roots, why)) = self.only {
            return if roots.contains(&ns) { Ok(()) } else { Err(Box::new(outside(reference, strings, why))) };
        }
        if EVERYWHERE.contains(&ns) || self.added.contains(&ns) {
            return Ok(());
        }
        let step = step_name(ns, strings);
        if let Some(name) = step.filter(|name| RESPONSE_FIELDS.contains(name)) {
            return Err(Box::new(bare_response_field(reference, name)));
        }
        let (code, needs) = LIMITED
            .iter()
            .find(|(root, ..)| *root == ns)
            .map_or((codes::ROOT_UNAVAILABLE, "only after that action, in the same test or block"), |(_, c, n)| {
                (*c, *n)
            });
        let name = parse::root_name(ns).or(step).unwrap_or("step");
        let diagnostic = Diagnostic::new(code, reference.span, format!("`{name}` is not available here"));
        Err(Box::new(diagnostic.help(format!("`{name}` is available {needs}"))))
    }
}

/// V2105 for a root a [`Roots::only`] spot does not read.
fn outside(reference: &Reference, strings: &StringTable, why: &str) -> Diagnostic {
    let name = parse::root_name(reference.ns).or_else(|| step_name(reference.ns, strings)).unwrap_or("step");
    Diagnostic::new(codes::ROOT_UNAVAILABLE, reference.span, format!("`{name}` is not available here")).help(why)
}

fn step_name(ns: Namespace, strings: &StringTable) -> Option<&str> {
    match ns {
        Namespace::Step(id) => strings.resolve(id),
        _ => None,
    }
}

impl Roots {
    /// V2105 for a step id that no action in reach declares, where [`Roots::check`] says only
    /// that it is not available: the roots there are, the `steps` that ran earlier, and the close
    /// names. `None` for a bare `status`, `headers` or `body`, which [`Roots::check`] reports.
    #[must_use]
    pub fn undeclared(reference: &Reference, name: &str, steps: &[&str]) -> Option<Diagnostic> {
        if RESPONSE_FIELDS.contains(&name) {
            return None;
        }
        let roots = parse::root_names().collect::<Vec<_>>().join(", ");
        let message = format!("`{name}` is not a root or a step id");
        let diagnostic = Diagnostic::new(codes::ROOT_UNAVAILABLE, reference.span, message).label("unknown root").help(
            format!("start it with a root ({roots}) or the id of an action that ran earlier in the same test or block"),
        );
        let diagnostic = match steps {
            [] => diagnostic,
            some => diagnostic.note(format!("ran earlier here: {}", some.join(", "))),
        };
        Some(closest(diagnostic, (reference, name), steps))
    }
}

/// `diagnostic` with the roots and `steps` close to `name`: as help, and as a certain fix when
/// exactly one is and the root's span is exact.
fn closest(diagnostic: Diagnostic, (reference, name): (&Reference, &str), steps: &[&str]) -> Diagnostic {
    let limit = (name.chars().count() / 3).max(1);
    let distance = |known: &str| strsim::damerau_levenshtein(&name.to_lowercase(), &known.to_lowercase());
    let close: Vec<&str> = parse::root_names().chain(steps.iter().copied()).filter(|k| distance(k) <= limit).collect();
    let one = match close.as_slice() {
        [] => return diagnostic,
        [one] => *one,
        several => return diagnostic.help(format!("did you mean one of {}?", several.join(", "))),
    };
    let diagnostic = diagnostic.help(format!("did you mean `{one}`?"));
    if reference.root_span.byte_range().len() != name.len() {
        return diagnostic;
    }
    diagnostic.fix(Fix::certain(format!("Replace with {one}"), vec![TextEdit::replace(reference.root_span, one)]))
}

fn bare_response_field(reference: &Reference, name: &str) -> Diagnostic {
    let message = format!("`{name}` is not a root; did you mean `response.{name}`?");
    let diagnostic = Diagnostic::new(codes::ROOT_UNAVAILABLE, reference.span, message);
    let diagnostic = diagnostic.help(format!("write `response.{name}`"));
    let root = reference.root_span;
    if root.byte_range().len() != name.len() {
        return diagnostic;
    }
    let edit = TextEdit::replace(root, format!("response.{name}"));
    diagnostic.fix(Fix::certain(format!("Write response.{name}"), vec![edit]))
}
