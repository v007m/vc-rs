//! Shared setup for the tests that build a program from one `vibranium.config.yaml`.

use std::sync::Arc;

use vc_model::{ContentHash, Program, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

/// `config` compiled alone, with `schema` as the `.env.schema`.
pub(super) fn compiled(config: &str, schema: &str) -> Compiled {
    let file = |path: &str, text: &str| SourceFile {
        path: SourcePath::new(path).unwrap(),
        text: text.to_owned(),
        hash: ContentHash::new([0; 32]),
    };
    let files = vec![file("vibranium.config.yaml", config)];
    let secret_schema = Some(file(".env.schema", schema));
    crate::compile(CompileInput { files, secret_schema, ..CompileInput::default() })
}

/// The program `config` builds with `schema`; fails the test on any error.
pub(super) fn program(config: &str, schema: &str) -> Arc<Program> {
    let compiled = compiled(config, schema);
    assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
    compiled.program.unwrap()
}
