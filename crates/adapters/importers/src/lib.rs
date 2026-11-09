//! Importers that read foreign files (`OpenAPI`, HAR) into drafts of definition files.
//!
//! Each kind is an entry of the [`ImporterRegistry`], built by an [`ImporterFactory`] over the
//! [`vc_interfaces::SpecFetcher`] the engine passes for the documents a file refers to. The
//! importers write nothing: the engine applies their `vc_interfaces::ImportDraft` through
//! `parser::edit` and its compile guard (D107).
//!
//! ```
//! use vc_adapter_importers::ImporterRegistry;
//!
//! assert_eq!(ImporterRegistry::builtin().names().count(), 0);
//! ```

mod registry;

pub use registry::{ImporterFactory, ImporterRegistry};
