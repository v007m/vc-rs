use vc_diagnostics::Code;

/// The type of one key's value, as the shape-check, the JSON Schemas and the docs see it.
#[derive(Clone, Copy, Debug)]
pub enum Ty {
    /// Plain text, no templates: names, reasons, globs.
    Text,
    /// A camelCase ID.
    Id,
    /// A reference to another item by ID (`users#create`, `shopOauth`).
    Ref,
    /// Text that may hold `${…}` templates.
    Template,
    /// A whole number.
    Int,
    /// `true` or `false`.
    Bool,
    /// A duration with its unit: `250ms`, `2s`.
    Duration,
    /// A size with its unit: `4kb` (binary, D146).
    Size,
    /// A calendar day, `YYYY-MM-DD`.
    Date,
    /// A status code, a class such as `2xx`, a list of them, or a matcher.
    Status,
    /// One of a fixed list of words.
    Enum(&'static [&'static str]),
    /// Any value; text inside it may hold templates, `{$cel: …}` computes one.
    Value,
    /// A literal (equals) or a map of matchers, comparing in this unit.
    Matcher(Unit),
    /// A CEL expression.
    Cel,
    /// A JSON Schema, inline or `$file`.
    Schema,
    /// A list of values of one type.
    List(&'static Self),
    /// A map with free keys and values of one type.
    MapOf(&'static Self),
    /// A map with declared keys.
    Obj(ShapeFn),
    /// The first alternative that fits the value's kind (scalar, list or map).
    Either(&'static [Self]),
}

/// A function returning a shape, so shapes can refer to each other (and to themselves).
pub type ShapeFn = fn() -> &'static Shape;

/// The unit that `eq`, `ne`, `gt`, `lt`, `between` and a bare literal compare in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Plain values.
    Plain,
    /// Durations: `{ lt: 500ms }`.
    Duration,
    /// Sizes: `{ lt: 100kb }`.
    Size,
    /// Rates, as a share from 0 to 1: `{ lt: 1% }` or `{ lt: 0.01 }`.
    Rate,
}

/// Roots a key makes available to the templates under it (D125).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scope(u8);

impl Scope {
    /// Nothing beyond the roots available everywhere.
    pub const NONE: Self = Self(0);
    /// `response` and `request`: expect, save, poll and cleanup.
    pub const CALL: Self = Self(1);
    /// `example`: a test with `examples`.
    pub const EXAMPLES: Self = Self(1 << 1);
    /// `iteration`: `repeat` on an action or a test.
    pub const LOOP: Self = Self(1 << 2);
    /// `item` and `iteration`: `forEach`.
    pub const FOR_EACH: Self = Self(1 << 3);

    /// Both scopes.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every root of `other` is in this scope.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// One declared key.
#[derive(Clone, Copy, Debug)]
pub struct Key {
    /// The key as written.
    pub name: &'static str,
    /// Its value's type.
    pub ty: Ty,
    /// Whether it must be written.
    pub required: bool,
    /// The code when it is required and missing: V1005, or V1003 for `syntaxVersion`.
    pub missing: Code,
    /// The default, as the docs show it.
    pub default: Option<&'static str>,
    /// What it is for: the docs, the schema descriptions and editor hovers.
    pub doc: &'static str,
    /// A value, as YAML.
    pub example: Option<&'static str>,
    /// Roots it adds for the templates under it.
    pub scope: Scope,
}

/// Starts a key declaration: `key("path", Ty::Template).required().doc("…")`.
#[must_use]
pub const fn key(name: &'static str, ty: Ty) -> Key {
    Key { name, ty, required: false, missing: Code::V1005, default: None, doc: "", example: None, scope: Scope::NONE }
}

impl Key {
    /// Must be written.
    #[must_use]
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Must be written, and is reported as `code` when it is not.
    #[must_use]
    pub const fn missing(mut self, code: Code) -> Self {
        self.required = true;
        self.missing = code;
        self
    }

    /// The default when it is not written.
    #[must_use]
    pub const fn default(mut self, value: &'static str) -> Self {
        self.default = Some(value);
        self
    }

    /// What it is for.
    #[must_use]
    pub const fn doc(mut self, doc: &'static str) -> Self {
        self.doc = doc;
        self
    }

    /// An example value.
    #[must_use]
    pub const fn example(mut self, example: &'static str) -> Self {
        self.example = Some(example);
        self
    }

    /// Roots it adds for the templates under it.
    #[must_use]
    pub const fn scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }
}

/// Keys of which exactly one must be written, and the code when that does not hold.
#[derive(Clone, Copy, Debug)]
pub struct OneOf {
    /// The keys.
    pub keys: &'static [&'static str],
    /// V2201 for `call` or `steps`.
    pub code: Code,
}

/// The declared keys of one kind of map.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    /// Its name in messages, schema `$defs` and docs headings.
    pub title: &'static str,
    /// What it is.
    pub doc: &'static str,
    /// Its own keys, in docs order.
    pub keys: &'static [Key],
    /// Shapes whose keys it also takes.
    pub mixins: &'static [ShapeFn],
    /// Keys of which exactly one is needed.
    pub one_of: Option<OneOf>,
    /// A key that, when written, adds roots to every sibling (`examples`, `forEach`).
    pub scopes: &'static [(&'static str, Scope)],
}

impl Shape {
    /// A shape with only keys.
    #[must_use]
    pub const fn new(title: &'static str, doc: &'static str, keys: &'static [Key]) -> Self {
        Self { title, doc, keys, mixins: &[], one_of: None, scopes: &[] }
    }

    /// Takes the keys of `mixins` too.
    #[must_use]
    pub const fn mixins(mut self, mixins: &'static [ShapeFn]) -> Self {
        self.mixins = mixins;
        self
    }

    /// Exactly one of `keys`, else `code`.
    #[must_use]
    pub const fn one_of(mut self, keys: &'static [&'static str], code: Code) -> Self {
        self.one_of = Some(OneOf { keys, code });
        self
    }

    /// Sibling keys that add roots.
    #[must_use]
    pub const fn scopes(mut self, scopes: &'static [(&'static str, Scope)]) -> Self {
        self.scopes = scopes;
        self
    }

    /// Every key, own keys first, then each mixin's in order.
    pub fn all_keys(&self) -> impl Iterator<Item = &'static Key> + use<'_> {
        let own = self.keys.iter();
        own.chain(self.mixins.iter().flat_map(|mixin| mixin().keys.iter()))
    }

    /// The key called `name`.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&'static Key> {
        self.all_keys().find(|key| key.name == name)
    }
}

impl Ty {
    /// How the docs and messages name it: "duration", "list of text".
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::List(item) => format!("list of {}", item.describe()),
            Self::MapOf(value) => format!("map of {}", value.describe()),
            Self::Obj(shape) => shape().title.to_owned(),
            Self::Enum(words) => format!("one of {}", words.join(", ")),
            Self::Either(alternatives) => alternatives.iter().map(|ty| ty.describe()).collect::<Vec<_>>().join(" or "),
            Self::Matcher(_) => "value or matcher".to_owned(),
            simple => simple_name(simple).to_owned(),
        }
    }
}

/// Whether a type is the one a name stands for.
type IsType = fn(Ty) -> bool;

/// The names of the types that take no parameters.
const SIMPLE_NAMES: &[(&str, IsType)] = &[
    ("text", |ty| matches!(ty, Ty::Text)),
    ("ID", |ty| matches!(ty, Ty::Id)),
    ("reference", |ty| matches!(ty, Ty::Ref)),
    ("template", |ty| matches!(ty, Ty::Template)),
    ("integer", |ty| matches!(ty, Ty::Int)),
    ("boolean", |ty| matches!(ty, Ty::Bool)),
    ("duration", |ty| matches!(ty, Ty::Duration)),
    ("size", |ty| matches!(ty, Ty::Size)),
    ("date (YYYY-MM-DD)", |ty| matches!(ty, Ty::Date)),
    ("status", |ty| matches!(ty, Ty::Status)),
    ("value", |ty| matches!(ty, Ty::Value)),
    ("CEL expression", |ty| matches!(ty, Ty::Cel)),
    ("JSON Schema", |ty| matches!(ty, Ty::Schema)),
];

fn simple_name(ty: Ty) -> &'static str {
    SIMPLE_NAMES.iter().find(|(_, is)| is(ty)).map_or("value", |(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::{Scope, Shape, Ty, Unit, key};
    use vc_diagnostics::Code;

    fn inner() -> &'static Shape {
        const SHAPE: Shape = Shape::new("inner", "", &[key("x", Ty::Int)]);
        &SHAPE
    }

    #[test]
    fn keys_come_from_the_shape_and_its_mixins() {
        const KEYS: &[super::Key] = &[key("a", Ty::Text).required().default("1").doc("d").example("e")];
        let shape = Shape::new("outer", "", KEYS).mixins(&[inner]).one_of(&["a", "x"], Code::V2201);
        assert_eq!(shape.all_keys().map(|k| k.name).collect::<Vec<_>>(), ["a", "x"]);
        assert!(shape.find("x").is_some() && shape.find("y").is_none());
        let a = shape.find("a").unwrap();
        assert!(a.required && a.default == Some("1") && a.doc == "d" && a.example == Some("e"));
        assert_eq!(shape.one_of.unwrap().keys, ["a", "x"]);
    }

    #[test]
    fn types_describe_themselves() {
        assert_eq!(Ty::List(&Ty::Text).describe(), "list of text");
        assert_eq!(Ty::MapOf(&Ty::Template).describe(), "map of template");
        assert_eq!(Ty::Obj(inner).describe(), "inner");
        assert_eq!(Ty::Enum(&["api", "full"]).describe(), "one of api, full");
        assert_eq!(Ty::Either(&[Ty::Int, Ty::Duration]).describe(), "integer or duration");
        assert_eq!(Ty::Matcher(Unit::Plain).describe(), "value or matcher");
        assert_eq!(Ty::Cel.describe(), "CEL expression");
        assert_eq!(Ty::Status.describe(), "status");
        assert_eq!(Ty::Date.describe(), "date (YYYY-MM-DD)");
    }

    #[test]
    fn scopes_combine() {
        let scope = Scope::CALL.with(Scope::LOOP);
        assert!(scope.has(Scope::CALL) && scope.has(Scope::LOOP) && !scope.has(Scope::EXAMPLES));
        assert!(Scope::NONE.has(Scope::NONE));
    }
}
