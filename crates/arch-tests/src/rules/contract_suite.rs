use std::collections::BTreeSet;

use syn::visit::{self, Visit};

use super::{Finding, scan_library};
use crate::{Rule, SourceFile, Violation, Workspace};

const ID: &str = "ARCH-CONTRACT-SUITE";

/// Every adapter or testkit impl of a `vc-interfaces` trait runs that trait's contract suite (D112).
pub struct ContractSuite;

impl Rule for ContractSuite {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let traits = interface_traits(workspace);
        if traits.is_empty() {
            return Vec::new();
        }
        scan_library(workspace, ID, |source, file| {
            let package = source.origin.package.as_str();
            if !(package.starts_with("vc-adapter-") || package == "vc-testkit") {
                return Vec::new();
            }
            let called = suites_called(workspace, package);
            let mut impls = Impls { traits: &traits, found: Vec::new() };
            impls.visit_file(file);
            impls
                .found
                .into_iter()
                .filter(|(_, suite)| !called.contains(suite))
                .map(|(span, suite)| {
                    (span, format!("implements an interface trait but no test calls `vc_testkit::contract::{suite}`"))
                })
                .collect()
        })
    }
}

/// Names of the traits `vc-interfaces` defines.
fn interface_traits(workspace: &Workspace) -> BTreeSet<String> {
    let mut traits = Traits(BTreeSet::new());
    let files = workspace.sources().iter().filter(|s| s.origin.package == "vc-interfaces");
    files.filter_map(SourceFile::library).for_each(|file| traits.visit_file(file));
    traits.0
}

/// The last segment of every path in `package`, test code included, that names a `*_suite` function.
fn suites_called(workspace: &Workspace, package: &str) -> BTreeSet<String> {
    let mut calls = SuiteCalls(BTreeSet::new());
    let files = workspace.sources().iter().filter(|s| s.origin.package == package);
    files.for_each(|source| calls.visit_file(&source.syntax));
    calls.0
}

struct Traits(BTreeSet<String>);

impl Visit<'_> for Traits {
    fn visit_item_trait(&mut self, item: &syn::ItemTrait) {
        self.0.insert(item.ident.to_string());
    }
}

struct SuiteCalls(BTreeSet<String>);

impl Visit<'_> for SuiteCalls {
    fn visit_path(&mut self, path: &syn::Path) {
        let last = path.segments.last().map(|s| s.ident.to_string());
        self.0.extend(last.filter(|name| name.ends_with("_suite")));
        visit::visit_path(self, path);
    }
}

/// Impls of interface traits, as the suite each one must call.
struct Impls<'a> {
    traits: &'a BTreeSet<String>,
    found: Vec<Finding>,
}

impl Visit<'_> for Impls<'_> {
    fn visit_item_impl(&mut self, item: &syn::ItemImpl) {
        let name = item.trait_.as_ref().and_then(|(path, _)| path.segments.last()).map(|s| &s.ident);
        if let Some(ident) = name.filter(|ident| self.traits.contains(&ident.to_string())) {
            self.found.push((ident.span(), format!("{}_suite", snake_case(&ident.to_string()))));
        }
        visit::visit_item_impl(self, item);
    }
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (index, c) in name.chars().enumerate() {
        if c.is_uppercase() && index > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::ContractSuite;
    use crate::fixtures::{check_files, file, lib_file};

    const TRAIT: &str = "pub trait ConfigSource { fn load(&self); }";
    const IMPL: &str = "impl vc_interfaces::ConfigSource for Env { fn load(&self) {} } impl Clone for Env {}";

    #[test]
    fn is_vacuous_without_interface_traits() {
        assert_eq!(check_files(&ContractSuite, vec![lib_file("vc-adapter-system", IMPL)]), []);
    }

    #[test]
    fn accepts_an_impl_whose_suite_runs_in_a_test() {
        let test = file(
            "vc-adapter-system",
            "tests/contract.rs",
            true,
            "#[test] fn t() { vc_testkit::contract::config_source_suite(&Env); }",
        );
        let sources = vec![lib_file("vc-interfaces", TRAIT), lib_file("vc-adapter-system", IMPL), test];
        assert_eq!(check_files(&ContractSuite, sources), []);
    }

    #[test]
    fn rejects_an_impl_without_its_suite() {
        let found = check_files(&ContractSuite, vec![lib_file("vc-interfaces", TRAIT), lib_file("vc-testkit", IMPL)]);
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("config_source_suite"), "{}", found[0]);
    }
}
