//! Importers that read foreign files (`OpenAPI`, curl commands, HAR, `.http`, Postman and Bruno
//! collections) into drafts of definition files.
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
//! assert_eq!(ImporterRegistry::builtin().names().collect::<Vec<_>>(), ["bruno", "curl", "har", "http", "openapi", "postman"]);
//! ```

mod body;
mod bruno;
mod collected;
mod curl;
mod har;
mod http;
mod ids;
mod naming;
mod openapi;
mod origin;
mod path_vars;
mod postman;
mod registry;
mod secret;
mod secrets;
#[cfg(test)]
mod shop_sample;
mod url;
mod v1;

pub use bruno::BrunoImporter;
pub use curl::CurlImporter;
pub use har::HarImporter;
pub use http::HttpImporter;
pub use openapi::OpenApiImporter;
pub use postman::PostmanImporter;
pub use registry::{ImporterFactory, ImporterRegistry};
pub use v1::{MigratedFile, V1Migration, migrate_v1};
