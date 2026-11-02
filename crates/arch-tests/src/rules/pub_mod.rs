use syn::visit::{self, Visit};

use super::{Finding, scan_library};
use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-PUB-MOD";

/// Modules stay private and `lib.rs` re-exports with `pub use`; only generated `vc-contracts` may `pub mod`.
pub struct PubMod;

impl Rule for PubMod {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |source, file| {
            if source.origin.package == "vc-contracts" {
                return Vec::new();
            }
            let mut finder = Finder(Vec::new());
            finder.visit_file(file);
            finder.0
        })
    }
}

struct Finder(Vec<Finding>);

impl Visit<'_> for Finder {
    fn visit_item_mod(&mut self, item: &syn::ItemMod) {
        if matches!(item.vis, syn::Visibility::Public(_)) {
            let message = format!("`pub mod {}`: keep modules private and `pub use` the items in lib.rs", item.ident);
            self.0.push((item.ident.span(), message));
        }
        visit::visit_item_mod(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::PubMod;
    use crate::fixtures::{check_files, check_lib, lib_file};

    #[test]
    fn accepts_private_and_crate_modules_and_contracts() {
        assert_eq!(check_lib(&PubMod, "mod a; pub(crate) mod b; pub use a::X;"), []);
        assert_eq!(check_files(&PubMod, vec![lib_file("vc-contracts", "pub mod generated;")]), []);
    }

    #[test]
    fn rejects_pub_mod_at_any_depth() {
        assert_eq!(check_lib(&PubMod, "pub mod a; mod b { pub mod c {} }").len(), 2);
    }
}
