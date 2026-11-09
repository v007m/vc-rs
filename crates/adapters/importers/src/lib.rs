//! Importers that read foreign files (`OpenAPI`, curl commands, HAR) into drafts of definition files.
//!
//! Each kind is an entry of the [`ImporterRegistry`], built by an [`ImporterFactory`] over the
//! [`vc_interfaces::SpecFetcher`] the engine passes for the documents a file refers to. The
//! importers write nothing: the engine applies their `vc_interfaces::ImportDraft` through
//! `parser::edit` and its compile guard (D107). [`migrate_v1`] converts a whole v1 project for
//! `vc migrate v1` (D200).
//!
//! ```
//! use vc_adapter_importers::ImporterRegistry;
//!
//! assert_eq!(ImporterRegistry::builtin().names().collect::<Vec<_>>(), ["curl", "har", "openapi"]);
//! ```

mod body;
mod curl;
mod har;
mod ids;
mod naming;
mod openapi;
mod origin;
mod path_vars;
mod registry;
mod secret;
mod secrets;
mod url;
mod v1;

pub use curl::CurlImporter;
pub use har::HarImporter;
pub use openapi::OpenApiImporter;
pub use registry::{ImporterFactory, ImporterRegistry};
pub use v1::{MigratedFile, V1Migration, migrate_v1};
