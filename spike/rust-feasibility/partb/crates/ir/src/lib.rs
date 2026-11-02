//! Compiled, immutable test program.
//!
//! A [`Program`] is built once by the compiler and then shared read-only
//! (`Arc<Program>`). Tests refer to each other only through [`TestId`]
//! indices into the program's arena, never through references.
mod program;
mod values;

pub use program::{IrError, Program, ProgramBuilder};
pub use values::{
    Assertion, Extract, FieldName, FileId, FileName, HeaderName, HookRef, Method, RequestSpec, Role, Scope, SetupRef,
    StatusCode, Template, Test, TestDraft, TestId, TestName, UrlPath, VarName, Vars,
};
