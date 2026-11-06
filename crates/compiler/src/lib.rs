//! Pure compiler: source files to an immutable Program.
//!
//! The engine reads the files and hands them over as [`SourceFile`](vc_model::SourceFile)
//! values with the previous [`CompileCache`]; [`compile`] returns the program (when there are
//! no errors), every diagnostic, the file table the diagnostics point into, and the cache to
//! persist for the next run (D132). The compiler does no I/O.
//!
//! Each file goes through the per-file stages, parse → shape-check → compile parts, and the
//! cache keeps the result for files that did not change. The shapes the shape-check uses also
//! generate the editors' JSON Schemas ([`schemas`]) and the docs' keys tables ([`key_tables`]).
//!
//! ```
//! use vc_compiler::{CompileCache, CompileInput, compile};
//! use vc_model::{ContentHash, SourceFile, SourcePath};
//!
//! let path = SourcePath::new("vibranium.config.yaml").unwrap();
//! let file = SourceFile { path, text: "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n".to_owned(), hash: ContentHash::new([0; 32]) };
//! let compiled = compile(CompileInput { files: vec![file], previous: CompileCache::new(), ..CompileInput::default() });
//! assert!(compiled.program.is_some() && !compiled.diagnostics.has_errors());
//! let cache = CompileCache::from_bytes(&compiled.cache.to_bytes());
//! assert_eq!(cache, compiled.cache);
//! ```

mod cache;
mod compile;
mod docs;
mod link;
mod lint;
mod matchers;
mod naming;
mod parts;
mod paths;
mod schema;
mod shape_check;
mod shapes;
mod stage;
mod suggest;
mod value;
mod version;

pub use cache::CompileCache;
pub use compile::{CompileInput, Compiled, compile};
pub use docs::key_tables;
pub use link::DataWanted;
pub use naming::{camel_id, file_id, is_camel_id, is_file_id};
pub use paths::SourceGlobs;
pub use schema::{FileSchema, schema_line, schemas};
