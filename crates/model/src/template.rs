use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

use crate::{SourceSpan, Sym};

/// A string that may hold `${…}` references, parsed once at compile time:
/// `"/users/${var.userId}"` is a literal part and a reference part.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
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

    /// Its references, in written order.
    pub fn references(&self) -> impl Iterator<Item = &Reference> {
        self.parts.iter().filter_map(|part| match part {
            Part::Ref(reference) => Some(reference),
            Part::Literal(_) => None,
        })
    }
}

/// One piece of a [`Template`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Part {
    /// Text copied as is; `$${` has already been unescaped to `${`.
    Literal(String),
    /// A `${…}` reference.
    Ref(Reference),
}

/// A `${namespace.path(args) | pipe | pipe(args)}` reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Reference {
    /// The root: `var`, `response`, a step id…
    pub ns: Namespace,
    /// The path after the root: `.body.items[0].id`.
    pub path: Path,
    /// Literal arguments of a generator call, `${fake.lorem.words(5)}`; empty otherwise.
    #[serde(serialize_with = "crate::json_text::out", deserialize_with = "crate::json_text::read")]
    pub args: Vec<Value>,
    /// The pipes applied left to right: `| int | default('x')`.
    pub pipes: Vec<PipeCall>,
    /// The whole `${…}`.
    pub span: SourceSpan,
    /// The root as written: `var` in `${ var.x }`; the whole scalar where spans are approximate.
    pub root_span: SourceSpan,
}

/// How a YAML scalar is written, so positions in its decoded text can be mapped back to the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(clippy::exhaustive_enums, reason = "closed set (D160)")]
pub enum ScalarStyle {
    /// Unquoted.
    Plain,
    /// `'…'`; `escaped` when the source holds a `''`.
    SingleQuoted {
        /// The source holds a `''`.
        escaped: bool,
    },
    /// `"…"`; `escaped` when the source holds a backslash.
    DoubleQuoted {
        /// The source holds a `\`.
        escaped: bool,
    },
    /// A `|` or `>` block.
    Block,
}

/// Where a scalar is written and how, the input to template parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ScalarSource {
    /// The whole scalar in the file, quotes included.
    pub span: SourceSpan,
    /// How it is written.
    pub style: ScalarStyle,
}

/// The root of a reference (D125).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Path {
    /// The steps, in order; empty means the value itself.
    pub segments: Vec<Segment>,
}

/// One step of a [`Path`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Segment {
    /// `.key` or `["quoted key"]`.
    Key(String),
    /// `[n]`; negative counts from the end.
    Index(i64),
    /// `[*]`: every element of a list.
    All,
}

/// One pipe in a reference: `take(10)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct PipeCall {
    /// The registered name: `take`.
    pub name: String,
    /// The literal arguments.
    #[serde(serialize_with = "crate::json_text::out", deserialize_with = "crate::json_text::read")]
    pub args: Vec<Value>,
    /// The name alone, for did-you-mean fixes.
    pub name_span: SourceSpan,
    /// The name and its arguments.
    pub span: SourceSpan,
}

/// The source of a `$cel` value or a CEL check, as written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct CelSource {
    /// The expression text.
    pub text: String,
    /// Where it was written.
    pub span: SourceSpan,
    /// How the scalar is written, so positions in the text can be mapped back to the file.
    pub style: ScalarStyle,
}

/// A CEL expression checked at compile time: it parses, and every root it reads is available
/// where it is written (D125). `vc-template` prepares it into an evaluator at run start.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct CelProgram {
    /// The expression as written.
    pub source: CelSource,
    /// Each root the expression reads, with the fields and literal indexes selected after it
    /// (`response.body.items[0]`), in source order; the linker checks step ids against these.
    pub refs: Vec<Reference>,
}

/// A value that may hold templates anywhere inside: bodies, vars, saves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum TemplateValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number.
    Number(#[serde(serialize_with = "crate::json_text::out", deserialize_with = "crate::json_text::read")] Number),
    /// A string, possibly with references.
    String(Template),
    /// A list.
    List(Vec<Self>),
    /// A map, in written order.
    Map(IndexMap<String, Self>),
    /// A `$cel` computed value, checked at compile time.
    Cel(CelProgram),
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

impl TemplateValue {
    /// Every reference the value makes, anywhere inside it, in written order; a `$cel` value
    /// gives the roots its expression reads.
    #[must_use]
    pub fn references(&self) -> Vec<&Reference> {
        let mut found = Vec::new();
        self.collect_references(&mut found);
        found
    }

    fn collect_references<'v>(&'v self, found: &mut Vec<&'v Reference>) {
        match self {
            Self::Null | Self::Bool(_) | Self::Number(_) => {}
            Self::String(template) => found.extend(template.references()),
            Self::Cel(program) => found.extend(&program.refs),
            Self::List(items) => items.iter().for_each(|item| item.collect_references(found)),
            Self::Map(map) => map.values().for_each(|item| item.collect_references(found)),
            Self::Extend { base, changes, .. } => {
                found.extend(base.references());
                changes.collect_references(found);
            }
        }
    }
}

/// An expected status: one code (`201`) or a class (`2xx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
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
    use indexmap::IndexMap;

    use super::{Namespace, Part, Path, Reference, StatusPattern, Template, TemplateValue};
    use crate::{Pos, SourceFileId, SourceSpan};

    fn reference() -> Reference {
        let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::START);
        Reference {
            ns: Namespace::Var,
            path: Path::default(),
            args: Vec::new(),
            pipes: Vec::new(),
            span,
            root_span: span,
        }
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
    fn a_value_gives_the_references_anywhere_inside_it_in_order() {
        let mut env = reference();
        env.ns = Namespace::Env;
        let read =
            |reference: &Reference| TemplateValue::String(Template { parts: vec![Part::Ref(reference.clone())] });
        let map = IndexMap::from([("a".to_owned(), read(&env)), ("b".to_owned(), TemplateValue::Null)]);
        let changes = Box::new(TemplateValue::List(vec![read(&reference()), TemplateValue::Map(map)]));
        let base = Template { parts: vec![Part::Literal("x".into()), Part::Ref(env.clone())] };
        let value = TemplateValue::Extend { base, changes, remove: Vec::new() };
        let namespaces: Vec<Namespace> = value.references().iter().map(|reference| reference.ns).collect();
        assert_eq!(namespaces, [Namespace::Env, Namespace::Var, Namespace::Env]);
        assert!(TemplateValue::Bool(true).references().is_empty());
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
