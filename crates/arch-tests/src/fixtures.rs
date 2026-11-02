use crate::{DepKind, Dependency, Origin, Package, Rule, SourceFile, Violation, Workspace};

pub fn package(name: &str, deps: &[(&str, DepKind)]) -> Package {
    let dependencies =
        deps.iter().map(|(dep, kind)| Dependency { name: (*dep).to_owned(), kind: *kind, rename: None }).collect();
    Package { name: name.to_owned(), manifest: format!("{}/Cargo.toml", dir(name)), dependencies }
}

pub fn workspace(packages: Vec<Package>, sources: Vec<SourceFile>) -> Workspace {
    Workspace::new(packages, sources)
}

pub fn file(package: &str, name: &str, test_only: bool, text: &str) -> SourceFile {
    let origin = Origin { package: package.to_owned(), path: format!("{}/{name}", dir(package)), test_only };
    SourceFile::parse(origin, text).unwrap()
}

pub fn lib_file(package: &str, text: &str) -> SourceFile {
    file(package, "src/lib.rs", false, text)
}

/// Runs `rule` over one library file of `vc-model`.
pub fn check_lib(rule: &dyn Rule, text: &str) -> Vec<Violation> {
    check_files(rule, vec![lib_file("vc-model", text)])
}

pub fn check_files(rule: &dyn Rule, sources: Vec<SourceFile>) -> Vec<Violation> {
    let ws = workspace(vec![package("vc-model", &[("tokio", DepKind::Normal)])], sources);
    rule.check(&ws)
}

fn dir(package: &str) -> String {
    format!("crates/{}", package.trim_start_matches("vc-"))
}
