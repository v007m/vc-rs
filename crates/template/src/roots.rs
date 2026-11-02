use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Namespace, Reference, Segment, StringTable, Sym};

use crate::codes;
use crate::parse::root_name;

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
/// only where the builder methods add them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots {
    added: Vec<Namespace>,
    in_expect: bool,
}

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

    /// Inside `expect`: `response` and `request`, and bare `status`, `headers` and `body`
    /// short for `response.…`.
    #[must_use]
    pub fn in_expect(self) -> Self {
        Self { in_expect: true, ..self.with_call() }
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

    fn with(mut self, roots: &[Namespace]) -> Self {
        self.added.extend_from_slice(roots);
        self
    }

    /// Checks that the reference's root is available here.
    ///
    /// # Errors
    ///
    /// V2102 for `item`, `example` or `iteration` outside its block, V2105 for any other root
    /// not available here, including a bare `status`, `headers` or `body` outside `expect`. Whether a step id names a step at all is the linker's check.
    pub fn check(&self, reference: &Reference, strings: &StringTable) -> Result<(), Box<Diagnostic>> {
        if reference.response_alias && !self.in_expect {
            return Err(Box::new(alias_outside_expect(reference)));
        }
        let ns = reference.ns;
        if EVERYWHERE.contains(&ns) || self.added.contains(&ns) {
            return Ok(());
        }
        let (code, needs) = LIMITED
            .iter()
            .find(|(root, ..)| *root == ns)
            .map_or((codes::ROOT_UNAVAILABLE, "only after that action, in the same test or block"), |(_, c, n)| {
                (*c, *n)
            });
        let name = root_name(ns).or_else(|| step_name(ns, strings)).unwrap_or("step");
        let diagnostic = Diagnostic::new(code, reference.span, format!("`{name}` is not available here"));
        Err(Box::new(diagnostic.help(format!("`{name}` is available {needs}"))))
    }
}

fn step_name(ns: Namespace, strings: &StringTable) -> Option<&str> {
    match ns {
        Namespace::Step(id) => strings.resolve(id),
        _ => None,
    }
}

fn alias_outside_expect(reference: &Reference) -> Diagnostic {
    let name = match reference.path.segments.first() {
        Some(Segment::Key(name)) => name.as_str(),
        _ => "body",
    };
    let message = format!("bare `{name}` is short for `response.{name}` only inside expect");
    Diagnostic::new(codes::ROOT_UNAVAILABLE, reference.span, message).help(format!("write `response.{name}`"))
}
