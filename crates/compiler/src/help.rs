//! What the YAML editor's completion and hover show (D681), read from the file shapes: the
//! keys a map takes, one key's docs, the words a value takes, and where matchers go.

use vc_model::FileKind;

use crate::shapes::{self, Key, Ty};

/// One declared key, as completion lists it and hover shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyHelp {
    /// The key as written.
    pub name: String,
    /// What it is for.
    pub doc: String,
    /// Its value's type in the docs' words: "duration", "one of GET, POST".
    pub ty: String,
    /// The default, as the docs show it.
    pub default: Option<String>,
    /// An example value, as YAML.
    pub example: Option<String>,
    /// Whether it must be written.
    pub required: bool,
    /// Whether its value can be a reference to another item by ID.
    pub reference: bool,
}

/// The keys the map at `path` in a file of `kind` takes, in docs order; none for a map whose
/// keys are data or a path the shapes do not reach. A path segment is a key, or `-` for a
/// list item.
#[must_use]
pub fn keys_at(kind: FileKind, path: &[&str]) -> Vec<KeyHelp> {
    shapes::shape_at(kind, path).map(|shape| shape.all_keys().map(help).collect()).unwrap_or_default()
}

/// The key that `path`'s last segment names, in the map the rest of it reaches.
#[must_use]
pub fn key_help(kind: FileKind, path: &[&str]) -> Option<KeyHelp> {
    let (name, parent) = path.split_last()?;
    shapes::shape_at(kind, parent)?.find(name).map(help)
}

/// The words the value at `path` can be: an enum's, or `true` and `false`.
#[must_use]
pub fn value_words(kind: FileKind, path: &[&str]) -> Vec<&'static str> {
    shapes::ty_at(kind, path).map(words).unwrap_or_default()
}

/// Whether the value at `path` can be a map of matchers (`{lt: 500ms}`).
#[must_use]
pub fn takes_matchers(kind: FileKind, path: &[&str]) -> bool {
    shapes::ty_at(kind, path).is_some_and(|ty| any(ty, |ty| matches!(ty, Ty::Matcher(_) | Ty::Status)))
}

/// Every matcher's name, in docs order.
#[must_use]
pub fn matcher_names() -> Vec<&'static str> {
    crate::matchers::names().collect()
}

fn help(key: &Key) -> KeyHelp {
    KeyHelp {
        name: key.name.to_owned(),
        doc: key.doc.to_owned(),
        ty: key.ty.describe(),
        default: key.default.map(str::to_owned),
        example: key.example.map(str::to_owned),
        required: key.required,
        reference: any(key.ty, |ty| matches!(ty, Ty::Ref)),
    }
}

fn words(ty: Ty) -> Vec<&'static str> {
    match ty {
        Ty::Enum(words) => words.to_vec(),
        Ty::Bool => vec!["true", "false"],
        Ty::Either(alternatives) => alternatives.iter().flat_map(|ty| words(*ty)).collect(),
        _ => Vec::new(),
    }
}

/// Whether `ty`, or one of its alternatives, is one that `is` picks.
fn any(ty: Ty, is: fn(Ty) -> bool) -> bool {
    is(ty) || matches!(ty, Ty::Either(alternatives) if alternatives.iter().any(|ty| is(*ty)))
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::{key_help, keys_at, matcher_names, takes_matchers, value_words};

    #[test]
    fn lists_a_maps_keys_with_their_docs_and_types() {
        let keys = keys_at(FileKind::Api, &["apis", "-"]);
        let method = keys.iter().find(|key| key.name == "method").unwrap();
        assert!(method.ty.starts_with("one of") && !method.doc.is_empty(), "{method:?}");
        assert!(keys_at(FileKind::Api, &["apis", "-", "vars"]).is_empty(), "a vars map's keys are data");
        let call = key_help(FileKind::Test, &["tests", "-", "call"]).unwrap();
        assert!(call.reference && !call.required, "{call:?}");
        assert_eq!(key_help(FileKind::Test, &["tests", "-", "nothing"]), None);
        assert_eq!(key_help(FileKind::Test, &[]), None);
    }

    #[test]
    fn knows_the_words_a_value_takes_and_where_matchers_go() {
        assert!(value_words(FileKind::Api, &["apis", "-", "method"]).contains(&"POST"));
        assert_eq!(value_words(FileKind::Test, &["tests", "-", "name"]), Vec::<&str>::new());
        assert!(takes_matchers(FileKind::Api, &["apis", "-", "expect", "status"]));
        assert!(!takes_matchers(FileKind::Api, &["apis", "-", "method"]));
        assert!(matcher_names().contains(&"between"));
    }
}
