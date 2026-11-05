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
pub use config::{auth, profile};
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

    use super::{Scope, reachable, root};

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
        let all = [Scope::CALL, Scope::EXAMPLES, Scope::LOOP, Scope::FOR_EACH];
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
