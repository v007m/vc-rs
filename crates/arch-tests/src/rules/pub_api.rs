//! Best-effort scan of the crates a file's public API names: pub fn signatures, pub struct fields,
//! pub enum variants, pub type aliases, pub trait methods and `pub use`.

use std::collections::BTreeMap;

use syn::visit::{self, Visit};
use syn::{Fields, ItemUse, Signature, Type, UseTree, Visibility};

use super::Finding;

/// Imported name to the crate (or `crate`, `self`, `super`) its `use` path starts from.
type Uses = BTreeMap<String, String>;

/// The crate each path in `file`'s public API is rooted in, resolved through the file's `use` items.
/// Single-segment paths that are not imported (locals, generics, the prelude) are left out.
pub fn named_crates(file: &syn::File) -> Vec<Finding> {
    let mut uses = UseCollector(Uses::new());
    uses.visit_file(file);
    let mut api = PubApi { uses: uses.0, found: Vec::new() };
    api.visit_file(file);
    api.found
}

struct UseCollector(Uses);

impl Visit<'_> for UseCollector {
    fn visit_item_use(&mut self, item: &ItemUse) {
        collect_uses(&item.tree, None, &mut self.0);
    }
}

fn collect_uses(tree: &UseTree, root: Option<&str>, out: &mut Uses) {
    match tree {
        UseTree::Path(path) => {
            let first = path.ident.to_string();
            collect_uses(&path.tree, Some(root.unwrap_or(&first)), out);
        }
        UseTree::Name(name) => {
            let ident = name.ident.to_string();
            out.insert(ident.clone(), root.map_or(ident, str::to_owned));
        }
        UseTree::Rename(rename) => {
            let root = root.map_or_else(|| rename.ident.to_string(), str::to_owned);
            out.insert(rename.rename.to_string(), root);
        }
        UseTree::Group(group) => group.items.iter().for_each(|item| collect_uses(item, root, out)),
        UseTree::Glob(_) => {}
    }
}

struct PubApi {
    uses: Uses,
    found: Vec<Finding>,
}

impl PubApi {
    fn record(&mut self, path: &syn::Path) {
        let Some(first) = path.segments.first() else { return };
        let ident = first.ident.to_string();
        let root = match self.uses.get(&ident) {
            Some(root) => root.clone(),
            None if path.segments.len() > 1 || path.leading_colon.is_some() => ident,
            None => return,
        };
        self.found.push((first.ident.span(), root));
    }

    fn signature(&mut self, sig: &Signature) {
        PathWalker(self).visit_signature(sig);
    }

    fn ty(&mut self, ty: &Type) {
        PathWalker(self).visit_type(ty);
    }

    fn fields(&mut self, fields: &Fields, only_pub: bool) {
        let named = fields.iter().filter(|f| !only_pub || matches!(f.vis, Visibility::Public(_)));
        named.for_each(|field| self.ty(&field.ty));
    }
}

/// Records every path inside the node it walks.
struct PathWalker<'a>(&'a mut PubApi);

impl Visit<'_> for PathWalker<'_> {
    fn visit_path(&mut self, path: &syn::Path) {
        self.0.record(path);
        visit::visit_path(self, path);
    }
}

const fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

impl Visit<'_> for PubApi {
    fn visit_item_fn(&mut self, item: &syn::ItemFn) {
        if is_pub(&item.vis) {
            self.signature(&item.sig);
        }
    }

    fn visit_impl_item_fn(&mut self, item: &syn::ImplItemFn) {
        if is_pub(&item.vis) {
            self.signature(&item.sig);
        }
    }

    fn visit_item_trait(&mut self, item: &syn::ItemTrait) {
        let methods = item.items.iter().filter_map(|i| if let syn::TraitItem::Fn(f) = i { Some(&f.sig) } else { None });
        methods.filter(|_| is_pub(&item.vis)).for_each(|sig| self.signature(sig));
    }

    fn visit_item_struct(&mut self, item: &syn::ItemStruct) {
        if is_pub(&item.vis) {
            self.fields(&item.fields, true);
        }
    }

    fn visit_item_enum(&mut self, item: &syn::ItemEnum) {
        let variants = item.variants.iter().filter(|_| is_pub(&item.vis));
        variants.for_each(|variant| self.fields(&variant.fields, false));
    }

    fn visit_item_type(&mut self, item: &syn::ItemType) {
        if is_pub(&item.vis) {
            self.ty(&item.ty);
        }
    }

    fn visit_item_use(&mut self, item: &ItemUse) {
        if let (true, UseTree::Path(path)) = (is_pub(&item.vis), &item.tree) {
            let root = path.ident.to_string();
            self.found.push((path.ident.span(), self.uses.get(&root).cloned().unwrap_or(root)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::named_crates;

    fn roots(src: &str) -> Vec<String> {
        named_crates(&syn::parse_file(src).unwrap()).into_iter().map(|(_, root)| root).collect()
    }

    #[test]
    fn resolves_imports_and_qualified_paths() {
        let src = "use tokio::sync::mpsc::{Sender as Tx}; use std::sync::Arc;
            pub fn f<T>(a: Tx<T>, b: Arc<hyper::Body>, c: Local) -> T { inner::g() }";
        assert_eq!(roots(src), ["tokio", "std", "hyper"]);
    }

    #[test]
    fn covers_public_items_only() {
        let src = "use reqwest::Client;
            pub struct S { pub a: Client, b: Client }
            struct P { pub a: Client }
            pub enum E { A(Client), B { x: Client } }
            pub type T = Client;
            pub trait Tr { fn m(&self) -> Client; }
            pub use reqwest::Url;
            fn private(c: Client) {}
            impl S { pub fn new(c: Client) -> Self { todo() } fn hidden(c: Client) {} }";
        assert_eq!(roots(src).len(), 7);
    }
}
