use super::{pub_api, scan_library};
use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-ADAPTER-NAME";

/// `vc-engine`'s public API never names an adapter; only `EngineBuilder`'s private code does.
pub struct AdapterName;

impl Rule for AdapterName {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |source, file| {
            if source.origin.package != "vc-engine" {
                return Vec::new();
            }
            let renamed = adapter_idents(workspace);
            let named = pub_api::named_crates(file).into_iter();
            let named = named.filter(|(_, root)| root.starts_with("vc_adapter_") || renamed.contains(root));
            named
                .map(|(span, root)| {
                    (span, format!("public API names `{root}`; keep adapters private to EngineBuilder"))
                })
                .collect()
        })
    }
}

/// What `vc-engine` imports each adapter dependency as, renames included.
fn adapter_idents(workspace: &Workspace) -> Vec<String> {
    let deps = workspace.package("vc-engine").map(|p| p.dependencies.as_slice()).unwrap_or_default();
    deps.iter().filter(|dep| dep.name.starts_with("vc-adapter-")).map(crate::Dependency::crate_ident).collect()
}

#[cfg(test)]
mod tests {
    use super::AdapterName;
    use crate::fixtures::{check_files, lib_file, package, workspace};
    use crate::{DepKind, Rule};

    #[test]
    fn accepts_private_adapter_use_and_other_crates() {
        let src = "use vc_adapter_http::HttpTransport; fn build() -> HttpTransport { t() }
            pub fn run(p: vc_model::Plan) {}";
        assert_eq!(check_files(&AdapterName, vec![lib_file("vc-engine", src)]), []);
        let other = "pub use vc_adapter_http::HttpTransport;";
        assert_eq!(check_files(&AdapterName, vec![lib_file("vc-cli", other)]), []);
    }

    #[test]
    fn rejects_adapters_in_the_public_api() {
        let src = "use vc_adapter_http::HttpTransport;
            pub use vc_adapter_system::Clock;
            pub fn transport() -> HttpTransport { t() }";
        assert_eq!(check_files(&AdapterName, vec![lib_file("vc-engine", src)]).len(), 2);
    }

    #[test]
    fn rejects_a_renamed_adapter_in_the_public_api() {
        let mut engine = package("vc-engine", &[("vc-adapter-http", DepKind::Normal)]);
        engine.dependencies[0].rename = Some("net".to_owned());
        let src = "pub fn transport() -> net::HttpTransport { t() }";
        let found = AdapterName.check(&workspace(vec![engine], vec![lib_file("vc-engine", src)]));
        assert_eq!(found.len(), 1);
    }
}
