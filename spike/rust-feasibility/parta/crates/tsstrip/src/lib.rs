//! Load-time TypeScript -> JavaScript type stripping with oxc, plus stack-trace
//! remapping through the generated source map.
use std::fmt;
use std::path::Path;

use oxc::allocator::Allocator;
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::transformer::{TransformOptions, Transformer};
use oxc_sourcemap::SourceMap;

/// A hook module after type stripping.
pub struct StrippedModule {
    /// Name used for the module inside the JS engine (the original `.ts` path).
    pub name: String,
    pub code: String,
    pub source_map: SourceMap<'static>,
}

#[derive(Debug)]
pub struct StripError(pub String);

impl fmt::Display for StripError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StripError {}

/// Parse `source` as TypeScript, remove all type syntax (including `import type`)
/// and print ES2022+ JavaScript with a source map back to the `.ts` file.
///
/// # Errors
/// Returns the rendered oxc diagnostics if the file does not parse or transform.
pub fn strip_types(path: &Path, source: &str) -> Result<StrippedModule, StripError> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).map_err(|e| StripError(e.to_string()))?;
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(render(&parsed.diagnostics, source));
    }
    let mut program = parsed.program;
    let semantic = SemanticBuilder::new().build(&program);
    let scoping = semantic.semantic.into_scoping();
    let options = TransformOptions::default();
    let transformed = Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(render(&transformed.diagnostics, source));
    }
    let codegen_options = CodegenOptions { source_map_path: Some(path.to_path_buf()), ..CodegenOptions::default() };
    let out = Codegen::new().with_options(codegen_options).build(&program);
    let source_map = out.map.ok_or_else(|| StripError("no source map produced".into()))?.into_owned();
    Ok(StrippedModule { name: path.display().to_string(), code: out.code, source_map })
}

fn render(diags: &[oxc::diagnostics::OxcDiagnostic], source: &str) -> StripError {
    let text: Vec<String> =
        diags.iter().map(|d| format!("{:?}", d.clone().with_source_code(source.to_string()))).collect();
    StripError(text.join("\n"))
}

impl StrippedModule {
    /// Map a 1-based (line, column) in the generated JS back to the 1-based
    /// position in the original TypeScript.
    #[must_use]
    pub fn original_position(&self, line: u32, col: u32) -> Option<(u32, u32)> {
        let table = self.source_map.generate_lookup_table();
        let token = self.source_map.lookup_token_approx(&table, line.checked_sub(1)?, col.saturating_sub(1))?;
        Some((token.get_src_line() + 1, token.get_src_col() + 1))
    }

    /// Rewrite every `<name>:<line>:<col>` (or `<name>:<line>`) occurrence in a
    /// JS stack trace so it points at the original `.ts` line/column.
    #[must_use]
    pub fn remap_stack(&self, stack: &str) -> String {
        let mut out = String::with_capacity(stack.len());
        let mut rest = stack;
        while let Some(idx) = rest.find(&self.name) {
            let (before, after_name_start) = rest.split_at(idx);
            out.push_str(before);
            out.push_str(&self.name);
            let after = &after_name_start[self.name.len()..];
            let (consumed, replacement) = self.remap_suffix(after);
            out.push_str(&replacement);
            rest = &after[consumed..];
        }
        out.push_str(rest);
        out
    }

    fn remap_suffix(&self, after: &str) -> (usize, String) {
        let (line, n1) = take_num(after);
        let Some(line) = line else { return (0, String::new()) };
        let (col, n2) = take_num(&after[n1..]);
        let gen_col = col.unwrap_or(1);
        match self.original_position(line, gen_col) {
            Some((l, c)) if col.is_some() => (n1 + n2, format!(":{l}:{c}")),
            Some((l, _)) => (n1, format!(":{l}")),
            None => (0, String::new()),
        }
    }
}

/// Parse `:<digits>` at the start of `s`; returns the number and bytes consumed.
fn take_num(s: &str) -> (Option<u32>, usize) {
    let Some(tail) = s.strip_prefix(':') else { return (None, 0) };
    let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return (None, 0);
    }
    (tail[..digits].parse().ok(), digits + 1)
}
