//! One declaration per file shape (LLD, "One declaration per file shape"). The shape-check,
//! the JSON Schemas and the docs tables are all generated from these tables.

/// Declares a shape as a function returning a `'static` reference, so shapes can refer to
/// each other through `Ty::Obj(name)`, including in cycles.
macro_rules! shape {
    ($(#[$meta:meta])* $name:ident = $shape:expr) => {
        $(#[$meta])*
        pub const fn $name() -> &'static $crate::shapes::Shape {
            const SHAPE: $crate::shapes::Shape = $shape;
            &SHAPE
        }
    };
}

mod api;
mod common;
mod config;
mod load;
mod test;
mod types;

use vc_model::FileKind;

#[cfg(test)]
pub use types::key;
pub use types::{Key, Scope, Shape, ShapeFn, Ty, Unit};

pub use api::api;
pub use common::{action, expect};
pub use config::{agents, auth, generator, profile};
pub use test::test;

/// The root shape of each file kind, in [`FileKind::ALL`] order.
const ROOTS: &[(FileKind, ShapeFn)] = &[
    (FileKind::Config, config::config),
    (FileKind::Api, api::api_file),
    (FileKind::Test, test::test_file),
    (FileKind::Load, load::load_file),
];

/// The shape of a whole file of `kind`.
#[must_use]
pub fn root(kind: FileKind) -> &'static Shape {
    ROOTS.iter().find(|(k, _)| *k == kind).map_or_else(config::config, |(_, shape)| shape())
}

/// Every shape `root` reaches through its keys' types, `root` first, each once, breadth first.
#[must_use]
pub fn reachable(root: &'static Shape) -> Vec<&'static Shape> {
    let mut found = vec![root];
    let mut index = 0;
    while let Some(shape) = found.get(index).copied() {
        shape.all_keys().for_each(|key| nested(key.ty, &mut found));
        index += 1;
    }
    found
}

/// The keys of the map at `path` in a file of `kind`, in the order the shape declares them.
///
/// A path segment is a key, or `-` for a list item. `None` for a map whose keys are data
/// (`vars`, `query`, `body`), which keeps the author's order, or a path the shapes do not reach.
#[must_use]
pub fn key_order(kind: FileKind, path: &[&str]) -> Option<Vec<&'static str>> {
    let start = ROOTS.iter().find(|(k, _)| *k == kind).map_or(Ty::Obj(config::config), |(_, shape)| Ty::Obj(*shape));
    let shape = match path.iter().try_fold(start, |ty, segment| step(ty, segment))? {
        Ty::Obj(shape) => shape,
        Ty::Either(alternatives) => alternatives.iter().find_map(|ty| object(*ty))?,
        _ => return None,
    };
    Some(shape().all_keys().map(|key| key.name).collect())
}

/// The shape of `ty` when it is a map with declared keys.
const fn object(ty: Ty) -> Option<ShapeFn> {
    if let Ty::Obj(shape) = ty { Some(shape) } else { None }
}

/// The type one step into a value of type `ty`: under the key `segment`, or a list item for `-`.
fn step(ty: Ty, segment: &str) -> Option<Ty> {
    match (ty, segment == "-") {
        (Ty::Obj(shape), false) => shape().find(segment).map(|key| key.ty),
        (Ty::List(inner), true) | (Ty::MapOf(inner), false) => Some(*inner),
        (Ty::Either(alternatives), _) => alternatives.iter().find_map(|ty| step(*ty, segment)),
        _ => None,
    }
}

fn nested(ty: Ty, found: &mut Vec<&'static Shape>) {
    match ty {
        Ty::Obj(shape) if !found.iter().any(|known| known.title == shape().title) => found.push(shape()),
        Ty::List(inner) | Ty::MapOf(inner) => nested(*inner, found),
        Ty::Either(alternatives) => alternatives.iter().for_each(|ty| nested(*ty, found)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use vc_model::FileKind;

    use super::{Scope, Ty, key_order, reachable, root, step};

    #[test]
    fn orders_the_keys_of_declared_maps_only() {
        let api = key_order(FileKind::Api, &["apis", "-"]).unwrap_or_default();
        assert_eq!(api.get(..4), Some(&["id", "name", "method", "path"][..]));
        let test = key_order(FileKind::Test, &["tests", "-"]).unwrap_or_default();
        let at = |name| test.iter().position(|key| *key == name);
        assert!(at("name") < at("call") && at("call").is_some());
        let expect = key_order(FileKind::Api, &["apis", "-", "expect"]).unwrap_or_default();
        assert!(expect.contains(&"status"));
        assert!(key_order(FileKind::Api, &["apis", "-", "vars"]).is_none());
        assert!(key_order(FileKind::Api, &["apis", "-", "body", "name"]).is_none());
        assert!(key_order(FileKind::Test, &["nope"]).is_none());
        assert!(key_order(FileKind::Config, &[]).is_some_and(|keys| keys.contains(&"syntaxVersion")));
    }

    #[test]
    fn orders_the_map_alternative_of_a_key_that_takes_either() {
        assert_eq!(key_order(FileKind::Test, &["tests", "-", "skip"]), Some(vec!["reason", "until"]));
        assert!(matches!(step(super::common::SKIP, "reason"), Some(Ty::Text)));
        assert!(step(super::common::SKIP, "-").is_none());
    }

    #[test]
    fn every_shape_has_unique_documented_keys() {
        let mut titles = HashSet::new();
        for kind in FileKind::ALL {
            assert_eq!(root(kind).title, kind.name());
            let mut seen = HashSet::new();
            for shape in reachable(root(kind)) {
                assert!(seen.insert(shape.title), "{} is reached twice", shape.title);
                titles.insert(shape.title);
                let mut names = HashSet::new();
                for key in shape.all_keys() {
                    assert!(names.insert(key.name), "{} declares `{}` twice", shape.title, key.name);
                    assert!(!key.doc.is_empty(), "{}.{} has no description", shape.title, key.name);
                }
            }
        }
        assert!(titles.len() > 30, "only {} shapes reached", titles.len());
        let api: Vec<_> = reachable(root(FileKind::Api)).iter().map(|shape| shape.title).collect();
        assert!(api.contains(&"authUse"), "a shape reached only through an alternative: {api:?}");
    }

    #[test]
    fn gives_each_scope_its_own_root() {
        let all = [Scope::CALL, Scope::EXAMPLES, Scope::LOOP, Scope::FOR_EACH, Scope::ROWS];
        for (index, scope) in all.iter().enumerate() {
            assert!(!Scope::NONE.has(*scope));
            assert_eq!(scope.with(*scope), *scope);
            assert!(all[index + 1..].iter().all(|other| !scope.has(*other)));
        }
    }

    #[test]
    fn required_keys_follow_the_syntax_reference() {
        let required = |kind| root(kind).all_keys().filter(|k| k.required).map(|k| k.name).collect::<Vec<_>>();
        assert_eq!(required(FileKind::Config), ["syntaxVersion"]);
        assert_eq!(required(FileKind::Load), ["mix", "stages"]);
        assert!(required(FileKind::Api).is_empty() && required(FileKind::Test).is_empty());
    }
}
