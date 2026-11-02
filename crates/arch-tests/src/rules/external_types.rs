use std::collections::BTreeSet;

use super::{pub_api, scan_library};
use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-EXTERNAL-TYPES";

/// The crates whose public API is restricted (D111).
const CHECKED: &[&str] = &["vc-model", "vc-interfaces"];

/// External crates their public API may name (D111); std, core, alloc and workspace crates are always fine.
const ALLOWED: &[&str] = &["serde", "serde_json", "indexmap", "smol_str", "bytes", "http"];

/// Public items in `vc-model` and `vc-interfaces` name only std and the D111 allow-list.
pub struct ExternalTypes;

impl Rule for ExternalTypes {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |source, file| {
            let Some(package) =
                workspace.package(&source.origin.package).filter(|p| CHECKED.contains(&p.name.as_str()))
            else {
                return Vec::new();
            };
            let external: BTreeSet<String> = package.dependencies.iter().map(crate::Dependency::crate_ident).collect();
            let named = pub_api::named_crates(file).into_iter();
            named
                .filter(|(_, root)| {
                    external.contains(root) && !ALLOWED.contains(&root.as_str()) && !root.starts_with("vc_")
                })
                .map(|(span, root)| {
                    (span, format!("public API names `{root}`; D111 allows only std, {}", ALLOWED.join(", ")))
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ExternalTypes;
    use crate::fixtures::check_lib;

    #[test]
    fn accepts_std_allowed_crates_and_private_use() {
        let src = "use std::sync::Arc; use serde_json::Value;
            pub fn f(a: Arc<Value>, b: http::Method) -> Local { g() }
            fn private(x: tokio::fs::File) {}";
        assert_eq!(check_lib(&ExternalTypes, src), []);
    }

    #[test]
    fn rejects_other_external_crates() {
        let src = "use tokio::sync::mpsc::Sender;\npub struct S { pub tx: Sender<u8> }";
        let found = check_lib(&ExternalTypes, src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].location, "crates/model/src/lib.rs:2");
    }
}
