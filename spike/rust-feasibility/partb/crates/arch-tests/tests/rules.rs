use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use arch_tests::{dependency_violations, primitive_obsession, static_items, workspace_graph};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn workspace_respects_dependency_rules() {
    let graph = workspace_graph(&root().join("Cargo.toml")).unwrap();
    assert!(graph.contains_key("engine") && graph.contains_key("ir"), "metadata looks wrong: {graph:?}");
    let violations = dependency_violations(&graph);
    assert!(violations.is_empty(), "architecture violations:\n{}", violations.join("\n"));
}

#[test]
fn rule_checker_detects_forbidden_edges() {
    let graph: BTreeMap<String, BTreeSet<String>> = [
        ("engine".to_owned(), BTreeSet::from(["ports".to_owned(), "adapters-http".to_owned()])),
        ("ir".to_owned(), BTreeSet::from(["ports".to_owned()])),
        ("brand-new".to_owned(), BTreeSet::new()),
    ]
    .into();
    let v = dependency_violations(&graph);
    assert_eq!(v.len(), 3, "{v:?}");
}

fn rust_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(rust_files(&p)?);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    Ok(out)
}

#[test]
fn domain_crates_avoid_primitive_obsession() {
    let mut found = Vec::new();
    for krate in ["ir", "ports"] {
        for file in rust_files(&root().join("crates").join(krate).join("src")).unwrap() {
            let src = std::fs::read_to_string(&file).unwrap();
            found.extend(primitive_obsession(&src).unwrap().into_iter().map(|f| format!("{}: {f}", file.display())));
        }
    }
    assert!(found.is_empty(), "primitive-typed domain fields:\n{}", found.join("\n"));
}

#[test]
fn primitive_obsession_heuristic_flags_examples() {
    let src = "pub struct Req { pub user_id: String, pub status: u16, pub path: UrlPath, note: String }";
    assert_eq!(primitive_obsession(src).unwrap(), vec!["Req.user_id: String", "Req.status: u16"]);
}

#[test]
fn no_global_statics_anywhere() {
    let mut found = Vec::new();
    for file in rust_files(&root().join("crates")).unwrap() {
        let src = std::fs::read_to_string(&file).unwrap();
        found.extend(static_items(&src).unwrap().into_iter().map(|s| format!("{}: static {s}", file.display())));
    }
    assert!(found.is_empty(), "global statics:\n{}", found.join("\n"));
}

#[test]
fn static_detector_flags_examples() {
    let src = "static A: AtomicU32 = AtomicU32::new(0); static mut B: u8 = 0; const C: u8 = 1;";
    assert_eq!(static_items(src).unwrap(), vec!["A", "B"]);
}
