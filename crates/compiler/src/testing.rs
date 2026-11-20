//! Helpers the compiler's unit tests share.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

/// A config with one var, `apiUrl`, and one default profile.
const CONFIG: &str =
    "syntaxVersion: 2\nproject: shop\nvars: {apiUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";

/// The code, line, column and length of each diagnostic.
pub fn spots(compiled: &Compiled) -> Vec<(Code, u32, u32, usize)> {
    let spot = |found: &Diagnostic| {
        let span = found.primary.span;
        (found.code, span.start.line, span.start.column, span.byte_range().len())
    };
    compiled.diagnostics.iter().map(spot).collect()
}

/// A source file at `path` holding `text`.
pub fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// A project of the config, the `feed` service's api file `apis` and its test file `tests`
/// (`feed.live`), compiled.
pub fn feed(apis: &str, tests: &str) -> Compiled {
    let files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/feed/feed.api.yaml", apis),
        file("services/feed/live.test.yaml", tests),
    ];
    crate::compile(CompileInput { files, ..CompileInput::default() })
}
