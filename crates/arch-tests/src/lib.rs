//! Machine-checked dependency and design rules for the workspace.
//!
//! A [`RuleRegistry`] runs every [`Rule`] against a [`Workspace`] loaded once from
//! `cargo metadata` and a `syn` parse of every module. Each [`Violation`] prints a
//! stable rule ID and a location.

mod error;
#[cfg(test)]
mod fixtures;
mod layers;
mod loader;
mod registry;
mod rules;
mod source;
mod violation;
mod workspace;

pub use error::ArchError;
pub use registry::{Rule, RuleRegistry};
pub use source::{Origin, SourceFile};
pub use violation::Violation;
pub use workspace::{DepKind, Dependency, Package, Workspace};
