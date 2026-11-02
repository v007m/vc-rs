//! Runs every registered rule against the real workspace.

use std::path::Path;

use vc_arch_tests::{RuleRegistry, Workspace};

#[test]
fn workspace_follows_every_architecture_rule() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace = Workspace::load(&root).unwrap();
    assert!(workspace.package("vc-model").is_some(), "cargo metadata returned no workspace packages");
    assert!(workspace.sources().iter().any(|s| s.origin.path == "crates/arch-tests/src/lib.rs"), "sources not loaded");
    let violations = RuleRegistry::standard().check(&workspace);
    let report: Vec<String> = violations.iter().map(ToString::to_string).collect();
    assert!(violations.is_empty(), "{} architecture violation(s):\n{}", report.len(), report.join("\n"));
}
