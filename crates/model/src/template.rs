use indexmap::IndexMap;
use serde_json::{Number, Value};

use crate::{SourceSpan, Sym};

/// A string that may hold `${…}` references, parsed once at compile time:
/// `"/users/${var.userId}"` is a literal part and a reference part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Template {
    /// The literal text and references, in order; adjacent literals are merged.
    pub parts: Vec<Part>,
}

impl Template {
    /// Whether the value is exactly one reference and nothing else, so rendering keeps the
    /// referenced value's type (number, list, object) instead of producing a string.
    #[must_use]
    pub const fn preserves_type(&self) -> bool {
        matches!(self.parts.as_slice(), [Part::Ref(_)])
    }

    /// The reference when the template is exactly one, see [`Template::preserves_type`].
    #[must_use]
    pub const fn single_ref(&self) -> Option<&Reference> {
        match self.parts.as_slice() {
            [Part::Ref(reference)] => Some(reference),
            _ => None,
        }
    }
}

/// One piece of a [`Template`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// Text copied as is; `$${` has already been unescaped to `${`.
    Literal(String),
    /// A `${…}` reference.
    Ref(Reference),
}

/// A `${namespace.path(args) | pipe | pipe(args)}` reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The root: `var`, `response`, a step id…
    pub ns: Namespace,
    /// The path after the root: `.body.items[0].id`.
    pub path: Path,
    /// Literal arguments of a generator call, `${fake.lorem.words(5)}`; empty otherwise.
    pub args: Vec<Value>,
    /// The pipes applied left to right: `| int | default('x')`.
    pub pipes: Vec<PipeCall>,
    /// The whole `${…}`.
    pub span: SourceSpan,
}

/// The root of a reference (D125).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// `var`: vars, call values, profile values and saved values.
    Var,
    /// `env`: operating-system environment variables.
    Env,
    /// `secret`: values from the secrets provider.
    Secret,
    /// `response`: the current call's response.
    Response,
    /// `request`: the current call's request as sent.
    Request,
    /// An action or step id: that action's last response.
    Step(Sym),
    /// `example`: the current row of `examples`.
    Example,
    /// `item`: the current element of a `forEach` loop.
    Item,
    /// `iteration`: `index` and `count` in a loop or repeated test.
    Iteration,
    /// `fake`: generated data.
    Fake,
    /// `dataset`: one of the project's datasets.
    Dataset,
    /// `run`: `id`, `startedAt`, `profiles`.
    Run,
}

/// A path into a JSON value: `.body.items[-1]["x-id"]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    /// The steps, in order; empty means the value itself.
    pub segments: Vec<Segment>,
}

/// One step of a [`Path`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// `.key` or `["quoted key"]`.
    Key(String),
    /// `[n]`; negative counts from the end.
    Index(i64),
    /// `[*]`: every element of a list.
    All,
}

/// One pipe in a reference: `take(10)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipeCall {
    /// The registered name: `take`.
    pub name: String,
    /// The literal arguments.
    pub args: Vec<Value>,
    /// The name alone, for did-you-mean fixes.
    pub name_span: SourceSpan,
    /// The name and its arguments.
    pub span: SourceSpan,
}

/// The source of a `$cel` computed value; the compiled program is added in a later milestone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CelSource {
    /// The expression text.
    pub text: String,
    /// Where it was written.
    pub span: SourceSpan,
}

/// A value that may hold templates anywhere inside: bodies, vars, saves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(Number),
    /// A string, possibly with references.
    String(Template),
    /// A list.
    List(Vec<Self>),
    /// A map, in written order.
    Map(IndexMap<String, Self>),
    /// A `$cel` computed value.
    Cel(CelSource),
    /// `$extend` a base value with `changes`, then delete the `$remove` paths.
    Extend {
        /// The value extended.
        base: Template,
        /// Deep-merged into the base.
        changes: Box<Self>,
        /// Deleted after the merge.
        remove: Vec<Path>,
    },
}

/// An expected status: one code (`201`) or a class (`2xx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusPattern {
    /// Exactly this code, 100 to 599.
    Code(u16),
    /// Any code in this hundred: 2 for `2xx`.
    Class(u8),
}

impl StatusPattern {
    /// Whether `status` matches.
    #[must_use]
    pub fn matches(self, status: u16) -> bool {
        match self {
            Self::Code(code) => code == status,
            Self::Class(class) => status / 100 == u16::from(class),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Namespace, Part, Path, Reference, StatusPattern, Template};
    use crate::{Pos, SourceFileId, SourceSpan};

    fn reference() -> Reference {
        let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START);
        Reference { ns: Namespace::Var, path: Path::default(), args: Vec::new(), pipes: Vec::new(), span }
    }

    #[test]
    fn only_a_lone_reference_preserves_type() {
        let lone = Template { parts: vec![Part::Ref(reference())] };
        assert!(lone.preserves_type());
        assert_eq!(lone.single_ref(), Some(&reference()));
        let mixed = Template { parts: vec![Part::Literal("u-".into()), Part::Ref(reference())] };
        assert!(!mixed.preserves_type());
        assert_eq!(mixed.single_ref(), None);
        assert!(!Template::default().preserves_type());
    }

    #[test]
    fn status_patterns_match_codes_and_classes() {
        assert!(StatusPattern::Code(201).matches(201));
        assert!(!StatusPattern::Code(201).matches(200));
        assert!(StatusPattern::Class(2).matches(299));
        assert!(!StatusPattern::Class(2).matches(300));
        assert!(!StatusPattern::Class(2).matches(199));
    }
}
