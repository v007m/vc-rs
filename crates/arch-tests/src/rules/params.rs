use syn::visit::{self, Visit};
use syn::{Ident, ImplItem, Item, UseTree};

use super::{Finding, scan_library};
use crate::{Rule, SourceFile, Violation, Workspace};

const ID: &str = "ARCH-PARAMS";

const MAX_PARAMS: usize = 3;

/// Crates outside the workspace that are always in scope.
const BUILTIN: &[&str] = &["std", "core", "alloc"];

/// A function or method takes at most three parameters, not counting `self` (D109). Methods of an
/// `impl` of a trait from outside the workspace are exempt: upstream fixes their signatures (D189).
pub struct Params;

impl Rule for Params {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |source, file| {
            let mut finder = ParamFinder { foreign: foreign_crates(workspace, source), file, found: Vec::new() };
            finder.visit_file(file);
            finder.found
        })
    }
}

/// The crate names `source` can reach that are not workspace members.
fn foreign_crates(workspace: &Workspace, source: &SourceFile) -> Vec<String> {
    let deps = workspace.package(&source.origin.package).map(|package| package.dependencies.iter());
    let external = deps.into_iter().flatten().filter(|dep| workspace.package(&dep.name).is_none());
    BUILTIN.iter().map(|name| (*name).to_owned()).chain(external.map(crate::Dependency::crate_ident)).collect()
}

struct ParamFinder<'f> {
    foreign: Vec<String>,
    file: &'f syn::File,
    found: Vec<Finding>,
}

impl ParamFinder<'_> {
    /// `path` names a trait declared outside the workspace, resolved through the file's own `use` items.
    fn is_foreign(&self, path: &syn::Path) -> bool {
        let Some(first) = path.segments.first().map(|segment| &segment.ident) else { return false };
        let mut imports = self.file.items.iter().filter_map(|item| match item {
            Item::Use(item) => Some(&item.tree),
            _ => None,
        });
        let root = imports.find_map(|tree| imported_from(tree, None, first)).unwrap_or_else(|| first.clone());
        self.foreign.contains(&root.to_string())
    }
}

/// The first segment of the `use` path in `tree` that brings `name` into scope.
fn imported_from(tree: &UseTree, root: Option<&Ident>, name: &Ident) -> Option<Ident> {
    match tree {
        UseTree::Path(path) => imported_from(&path.tree, Some(root.unwrap_or(&path.ident)), name),
        UseTree::Name(used) => (used.ident == *name).then(|| root.unwrap_or(&used.ident).clone()),
        UseTree::Rename(used) => (used.rename == *name).then(|| root.unwrap_or(&used.ident).clone()),
        UseTree::Group(group) => group.items.iter().find_map(|item| imported_from(item, root, name)),
        UseTree::Glob(_) => None,
    }
}

impl Visit<'_> for ParamFinder<'_> {
    fn visit_item_impl(&mut self, item: &syn::ItemImpl) {
        if item.trait_.as_ref().is_some_and(|(path, _)| self.is_foreign(path)) {
            let methods = item.items.iter().filter_map(|inner| match inner {
                ImplItem::Fn(method) => Some(method),
                _ => None,
            });
            methods.for_each(|method| self.visit_block(&method.block));
        } else {
            visit::visit_item_impl(self, item);
        }
    }

    fn visit_signature(&mut self, sig: &syn::Signature) {
        let count = sig.inputs.iter().filter(|arg| matches!(arg, syn::FnArg::Typed(_))).count();
        if count > MAX_PARAMS {
            let message =
                format!("`{}` takes {count} parameters (max {MAX_PARAMS}); group them in a struct", sig.ident);
            self.found.push((sig.ident.span(), message));
        }
        visit::visit_signature(self, sig);
    }
}

#[cfg(test)]
mod tests {
    use super::Params;
    use crate::fixtures::check_lib;

    #[test]
    fn accepts_three_parameters_plus_self() {
        let src = "fn f(a: u8, b: u8, c: u8) {} impl S { fn g(&self, a: u8, b: u8, c: u8) {} }";
        assert_eq!(check_lib(&Params, src), []);
        assert_eq!(check_lib(&Params, "#[test] fn t(a: u8, b: u8, c: u8, d: u8) {}"), []);
    }

    #[test]
    fn rejects_four_parameters_in_functions_methods_and_traits() {
        let src = "fn f(a: u8, b: u8, c: u8, d: u8) {}
            impl S { fn g(self, a: u8, b: u8, c: u8, d: u8) {} }
            trait T { fn h(&self, a: u8, b: u8, c: u8, d: u8); }";
        assert_eq!(check_lib(&Params, src).len(), 3);
    }

    #[test]
    fn exempts_impls_of_traits_from_outside_the_workspace() {
        let src = "use tokio::io::{AsyncRead as Read, AsyncSeek}; use std::fmt;
            impl Read for S { fn r(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl AsyncSeek for S { fn s(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl fmt::Write for S { fn w(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl ::core::hash::Hasher for S { fn h(&self, a: u8, b: u8, c: u8, d: u8) {} }";
        assert_eq!(check_lib(&Params, src), []);
    }

    #[test]
    fn still_counts_workspace_trait_impls_and_bodies_of_foreign_ones() {
        let src = "use inner::Local; use crate::model::Shared; use tokio::io::AsyncRead;
            impl Local for S { fn a(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl Shared for S { fn b(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl inner::Other for S { fn c(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl Unimported for S { fn d(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl serde::Undeclared for S { fn e(&self, a: u8, b: u8, c: u8, d: u8) {} }
            impl AsyncRead for S { fn r(&self) { fn nested(a: u8, b: u8, c: u8, d: u8) {} } }";
        let found = check_lib(&Params, src);
        let names: Vec<_> = found.iter().map(|v| v.message.split('`').nth(1).unwrap_or_default()).collect();
        assert_eq!(names, ["a", "b", "c", "d", "e", "nested"]);
    }
}
