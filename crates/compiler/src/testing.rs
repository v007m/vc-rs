//! Helpers the compiler's unit tests share.

use vc_diagnostics::{Code, Diagnostic};

use crate::Compiled;

/// The code, line, column and length of each diagnostic.
pub fn spots(compiled: &Compiled) -> Vec<(Code, u32, u32, usize)> {
    let spot = |found: &Diagnostic| {
        let span = found.primary.span;
        (found.code, span.start.line, span.start.column, span.byte_range().len())
    };
    compiled.diagnostics.iter().map(spot).collect()
}
