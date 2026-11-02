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
            let named = pub_api::named_crates(file).into_iter().filter(|(_, root)| root.starts_with("vc_adapter_"));
            named
                .map(|(span, root)| {
                    (span, format!("public API names `{root}`; keep adapters private to EngineBuilder"))
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AdapterName;
    use crate::fixtures::{check_files, lib_file};

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
}
