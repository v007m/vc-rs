use std::fmt;
use std::path::Path;

use crate::{ArchError, SourceFile, loader};

/// How a package uses a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "internal gate crate; no external users")]
pub enum DepKind {
    /// `[dependencies]`.
    Normal,
    /// `[build-dependencies]`.
    Build,
    /// `[dev-dependencies]`.
    Dev,
}

impl fmt::Display for DepKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Normal => "normal",
            Self::Build => "build",
            Self::Dev => "dev",
        })
    }
}

/// One dependency of a workspace package, internal or external.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "internal gate crate; no external users")]
pub struct Dependency {
    /// Package name, such as `vc-model` or `serde`.
    pub name: String,
    /// Dependency section it is declared in.
    pub kind: DepKind,
    /// The key it is declared under when that differs from the package (`h = { package = "http" }`).
    pub rename: Option<String>,
}

impl Dependency {
    /// The name the dependency is imported under in Rust code (`vc-model` is `vc_model`, a rename wins).
    #[must_use]
    pub fn crate_ident(&self) -> String {
        self.rename.as_deref().unwrap_or(&self.name).replace('-', "_")
    }
}

/// A workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "internal gate crate; no external users")]
pub struct Package {
    /// Package name, such as `vc-compiler`.
    pub name: String,
    /// Workspace-relative path of its `Cargo.toml`.
    pub manifest: String,
    /// Every declared dependency.
    pub dependencies: Vec<Dependency>,
}

/// The model every rule checks: workspace packages and their parsed sources, loaded once.
#[derive(Debug, Clone)]
pub struct Workspace {
    packages: Vec<Package>,
    sources: Vec<SourceFile>,
}

impl Workspace {
    /// Loads the workspace whose root `Cargo.toml` is in `root`.
    ///
    /// # Errors
    /// When `cargo metadata` fails or a source file cannot be read or parsed.
    pub fn load(root: &Path) -> Result<Self, ArchError> {
        loader::load(root)
    }

    /// Builds a workspace from in-memory parts (fixtures).
    #[must_use]
    pub const fn new(packages: Vec<Package>, sources: Vec<SourceFile>) -> Self {
        Self { packages, sources }
    }

    /// Every workspace member.
    #[must_use]
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// The member called `name`.
    #[must_use]
    pub fn package(&self, name: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Every parsed source file.
    #[must_use]
    pub fn sources(&self) -> &[SourceFile] {
        &self.sources
    }

    /// Every dependency edge between two workspace members.
    #[must_use]
    pub fn internal_deps(&self) -> Vec<(&Package, &Dependency)> {
        let edges = self.packages.iter().flat_map(|p| p.dependencies.iter().map(move |d| (p, d)));
        edges.filter(|(_, d)| self.package(&d.name).is_some()).collect()
    }
}
