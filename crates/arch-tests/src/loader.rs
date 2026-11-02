use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand, Target};
use syn::Item;

use crate::source::is_test;
use crate::{ArchError, DepKind, Dependency, Origin, Package, SourceFile, Workspace};

/// A module file still to read: its path, whether it is a crate root, and whether it is test code.
struct Pending {
    path: PathBuf,
    root: bool,
    test_only: bool,
}

/// Reads packages from `cargo metadata` and parses every module reachable from their targets.
pub fn load(root: &Path) -> Result<Workspace, ArchError> {
    let meta = MetadataCommand::new().manifest_path(root.join("Cargo.toml")).no_deps().exec()?;
    let base = meta.workspace_root.as_std_path();
    let mut packages = Vec::new();
    let mut sources = Vec::new();
    for package in meta.workspace_packages() {
        let pending = package.targets.iter().map(target_root).collect();
        sources.extend(load_modules(base, package.name.as_str(), pending)?);
        packages.push(Package {
            name: package.name.to_string(),
            manifest: relative(base, package.manifest_path.as_std_path()),
            dependencies: package.dependencies.iter().map(dependency).collect(),
        });
    }
    Ok(Workspace::new(packages, sources))
}

fn dependency(dep: &cargo_metadata::Dependency) -> Dependency {
    let kind = match dep.kind {
        DependencyKind::Development => DepKind::Dev,
        DependencyKind::Build => DepKind::Build,
        _ => DepKind::Normal,
    };
    Dependency { name: dep.name.clone(), kind }
}

fn target_root(target: &Target) -> Pending {
    let test_only = target.is_test() || target.is_bench() || target.is_example();
    Pending { path: target.src_path.clone().into_std_path_buf(), root: true, test_only }
}

fn load_modules(base: &Path, package: &str, mut pending: Vec<Pending>) -> Result<Vec<SourceFile>, ArchError> {
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();
    while let Some(next) = pending.pop() {
        if !seen.insert(next.path.clone()) {
            continue;
        }
        let text = std::fs::read_to_string(&next.path)
            .map_err(|source| ArchError::Read { path: next.path.clone(), source })?;
        let origin =
            Origin { package: package.to_owned(), path: relative(base, &next.path), test_only: next.test_only };
        let file = SourceFile::parse(origin, &text)?;
        pending.extend(child_modules(&next, &file.syntax.items));
        files.push(file);
    }
    Ok(files)
}

fn child_modules(parent: &Pending, items: &[Item]) -> Vec<Pending> {
    let dir = module_dir(parent);
    let declared = items.iter().filter_map(|item| match item {
        Item::Mod(module) if module.content.is_none() => Some(module),
        _ => None,
    });
    declared
        .filter_map(|module| {
            let name = module.ident.to_string();
            let path =
                [dir.join(format!("{name}.rs")), dir.join(name).join("mod.rs")].into_iter().find(|p| p.is_file())?;
            Some(Pending { path, root: false, test_only: parent.test_only || is_test(&module.attrs) })
        })
        .collect()
}

fn module_dir(parent: &Pending) -> PathBuf {
    let dir = parent.path.parent().map(Path::to_path_buf).unwrap_or_default();
    match parent.path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) if !parent.root && stem != "mod" => dir.join(stem),
        _ => dir,
    }
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}
