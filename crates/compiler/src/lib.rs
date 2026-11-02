//! Pure compiler: source files to an immutable Program.
//!
//! The engine reads the files and hands them over as [`SourceFile`](vc_model::SourceFile)
//! values with the previous [`CompileCache`]; [`compile`] returns the program (when there are
//! no errors), every diagnostic, the file table the diagnostics point into, and the cache to
//! persist for the next run (D132). The compiler does no I/O.
//!
//! ```
//! use vc_compiler::{CompileCache, CompileInput, compile};
//! use vc_model::{ContentHash, SourceFile, SourcePath};
//!
//! let path = SourcePath::new("vibranium.config.yaml").unwrap();
//! let file = SourceFile { path, text: "syntaxVersion: 2\n".to_owned(), hash: ContentHash::new([0; 32]) };
//! let compiled = compile(CompileInput { files: vec![file], previous: CompileCache::new() });
//! assert!(compiled.program.is_some() && !compiled.diagnostics.has_errors());
//! let cache = CompileCache::from_bytes(&compiled.cache.to_bytes());
//! assert_eq!(cache, compiled.cache);
//! ```

mod cache;
mod compile;
mod schema;

pub use cache::CompileCache;
pub use compile::{CompileInput, Compiled, compile};
pub use schema::{FileSchema, schemas};
