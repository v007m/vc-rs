use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand, Target};
use syn::{Attribute, Expr, ExprLit, Item, ItemMod, Lit, Meta, MetaNameValue};

use crate::source::is_test;
use crate::{ArchError, DepKind, Dependency, Origin, Package, SourceFile, Workspace};

/// A module file still to read: its path, whether it owns its directory (a crate root, `mod.rs` or
/// `#[path]` file), and whether it is test code.
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
    Dependency { name: dep.name.clone(), kind, rename: dep.rename.clone() }
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
        pending.extend(child_modules(&next, &file.syntax.items)?);
        files.push(file);
    }
    Ok(files)
}

/// Where the `mod` items of one file or inline module block resolve.
struct Scope {
    /// Directory that `mod name;` looks in.
    dir: PathBuf,
    /// Directory that a `#[path]` attribute is relative to.
    path_base: PathBuf,
    test_only: bool,
}

fn child_modules(parent: &Pending, items: &[Item]) -> Result<Vec<Pending>, ArchError> {
    let path_base = parent.path.parent().map(Path::to_path_buf).unwrap_or_default();
    let scope = Scope { dir: module_dir(parent, &path_base), path_base, test_only: parent.test_only };
    let mut found = Vec::new();
    collect_modules(&scope, items, &mut found)?;
    Ok(found)
}

fn collect_modules(scope: &Scope, items: &[Item], found: &mut Vec<Pending>) -> Result<(), ArchError> {
    let modules = items.iter().filter_map(|item| if let Item::Mod(module) = item { Some(module) } else { None });
    for module in modules {
        let test_only = scope.test_only || is_test(&module.attrs);
        if let Some((_, inner)) = &module.content {
            let dir = path_attr(&module.attrs)
                .map_or_else(|| scope.dir.join(module.ident.to_string()), |p| scope.path_base.join(p));
            collect_modules(&Scope { path_base: dir.clone(), dir, test_only }, inner, found)?;
        } else {
            found.push(module_file(scope, module, test_only)?);
        }
    }
    Ok(())
}

/// The file `mod name;` loads: its `#[path]`, else `name.rs`, else `name/mod.rs`.
fn module_file(scope: &Scope, module: &ItemMod, test_only: bool) -> Result<Pending, ArchError> {
    if let Some(path) = path_attr(&module.attrs) {
        // A #[path] file owns its directory like a mod.rs file does.
        return Ok(Pending { path: scope.path_base.join(path), root: true, test_only });
    }
    let name = module.ident.to_string();
    let candidates = [scope.dir.join(format!("{name}.rs")), scope.dir.join(&name).join("mod.rs")];
    let path = candidates.into_iter().find(|p| p.is_file());
    let path = path.ok_or_else(|| ArchError::Module { dir: scope.dir.clone(), name })?;
    Ok(Pending { path, root: false, test_only })
}

fn path_attr(attrs: &[Attribute]) -> Option<String> {
    attrs.iter().find(|attr| attr.path().is_ident("path")).and_then(|attr| match &attr.meta {
        Meta::NameValue(MetaNameValue { value: Expr::Lit(ExprLit { lit: Lit::Str(path), .. }), .. }) => {
            Some(path.value())
        }
        _ => None,
    })
}

fn module_dir(parent: &Pending, file_dir: &Path) -> PathBuf {
    match parent.path.file_stem().and_then(|s| s.to_str()) {
        Some(stem) if !parent.root && stem != "mod" => file_dir.join(stem),
        _ => file_dir.to_path_buf(),
    }
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Pending, load_modules};
    use crate::{ArchError, Origin};

    /// A throwaway crate under the system temp dir, removed on drop.
    struct Crate(PathBuf);

    impl Crate {
        fn new(name: &str, files: &[(&str, &str)]) -> Self {
            let dir = std::env::temp_dir().join(format!("vc-arch-loader-{name}-{}", std::process::id()));
            for (path, text) in files {
                let path = dir.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Self(dir)
        }

        fn load(&self) -> Result<Vec<Origin>, ArchError> {
            let root = Pending { path: self.0.join("src/lib.rs"), root: true, test_only: false };
            let files = load_modules(&self.0, "vc-x", vec![root])?;
            let mut found: Vec<_> = files.into_iter().map(|f| f.origin).collect();
            found.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(found)
        }
    }

    impl Drop for Crate {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn paths(found: &[Origin]) -> Vec<&str> {
        found.iter().map(|origin| origin.path.as_str()).collect()
    }

    #[test]
    fn follows_path_attributes_and_inline_modules() {
        let krate = Crate::new(
            "follow",
            &[
                (
                    "src/lib.rs",
                    "mod a; mod outer { mod b; } #[path = \"gen/c_impl.rs\"] mod c; #[cfg(test)] mod t { mod d; }",
                ),
                ("src/a.rs", "mod e;"),
                ("src/a/e.rs", ""),
                ("src/outer/b.rs", ""),
                ("src/gen/c_impl.rs", "mod f;"),
                ("src/gen/f.rs", ""),
                ("src/t/d.rs", ""),
            ],
        );
        let found = krate.load().unwrap();
        let expected = [
            "src/a.rs",
            "src/a/e.rs",
            "src/gen/c_impl.rs",
            "src/gen/f.rs",
            "src/lib.rs",
            "src/outer/b.rs",
            "src/t/d.rs",
        ];
        assert_eq!(paths(&found), expected);
        let test_only: Vec<_> = found.iter().filter(|origin| origin.test_only).map(|o| o.path.as_str()).collect();
        assert_eq!(test_only, ["src/t/d.rs"]);
    }

    #[test]
    fn rejects_a_module_without_a_file() {
        let krate = Crate::new("missing", &[("src/lib.rs", "mod gone;")]);
        let error = krate.load().unwrap_err();
        assert!(matches!(error, ArchError::Module { name, .. } if name == "gone"));
        assert!(Path::new(&krate.0).is_dir());
    }
}
