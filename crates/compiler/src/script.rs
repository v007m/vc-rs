//! The per-file stage for a hook file (D172): oxc parses it, strips its types and reads what it
//! exports and imports. Pure: the text comes in, JavaScript with a line map comes out.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions, CodegenReturn};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::{ExportEntry, ExportExportName, ModuleRecord};
use oxc_transformer::{TransformOptions, Transformer};
use serde::{Deserialize, Serialize};
use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{HookModule, Pos, SourceFile, SourceFileId, SourceSpan};

/// The suffixes of a hook file: ES modules in TypeScript or JavaScript.
const SUFFIXES: [&str; 4] = [".ts", ".mts", ".js", ".mjs"];

/// A hook file after the per-file stage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    /// The JavaScript, with its line map.
    pub module: HookModule,
    /// The names of its value exports; `default` for the default export.
    pub exports: Vec<String>,
    /// Whether it re-exports everything from another file (`export * from`), so its exports are
    /// not all known here.
    pub stars: bool,
    /// What it still imports once types are stripped.
    pub imports: Vec<Import>,
}

/// One import of a hook file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// The module named, as written.
    pub specifier: String,
    /// Where it is named.
    pub span: SourceSpan,
    /// Whether it is an `import()` expression.
    pub dynamic: bool,
}

/// Whether the file at `path` is a hook file.
pub fn is_script(path: &str) -> bool {
    SUFFIXES.iter().any(|suffix| path.ends_with(suffix))
}

/// Strips `source`, numbered `id`; V1013 when it does not parse.
pub fn strip(id: SourceFileId, source: &SourceFile) -> (Option<Script>, Diagnostics) {
    let path = Path::new(source.path.as_str());
    let at = Spot { id, text: &source.text };
    let Ok(source_type) = SourceType::from_path(path) else { return (None, Diagnostics::new()) };
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &source.text, source_type).parse();
    if let Some(error) = parsed.diagnostics.first() {
        return (None, std::iter::once(at.invalid(error)).collect());
    }
    let (exports, stars) = exports(&parsed.module_record);
    let dynamic: Vec<Import> =
        parsed.module_record.dynamic_imports.iter().map(|found| at.dynamic(found.module_request)).collect();
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();
    let options = TransformOptions::default();
    let stripped = Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(error) = stripped.diagnostics.first() {
        return (None, std::iter::once(at.invalid(error)).collect());
    }
    let mut imports = at.imports(&program);
    imports.extend(dynamic);
    let options = CodegenOptions { source_map_path: Some(path.to_path_buf()), ..CodegenOptions::default() };
    let module = module(Codegen::new().with_options(options).build(&program));
    (Some(Script { module, exports, stars, imports }), Diagnostics::new())
}

/// The value exports by name, and whether there is an `export *`.
fn exports(record: &ModuleRecord<'_>) -> (Vec<String>, bool) {
    let entries = record.local_export_entries.iter().chain(&record.indirect_export_entries);
    let mut names: Vec<String> = entries.filter(|entry| !entry.is_type).filter_map(export_name).collect();
    names.sort();
    names.dedup();
    let stars = record.star_export_entries.iter().any(|entry| !entry.is_type && entry.export_name.is_null());
    (names, stars)
}

/// The name an export entry exports under.
fn export_name(entry: &ExportEntry<'_>) -> Option<String> {
    match &entry.export_name {
        ExportExportName::Name(name) => Some(name.name.to_string()),
        ExportExportName::Default(_) => Some("default".to_owned()),
        ExportExportName::Null => None,
    }
}

/// The JavaScript, with the TypeScript line of the first mapped token on each line.
fn module(generated: CodegenReturn<'_>) -> HookModule {
    let mut lines = vec![0; generated.code.lines().count()];
    for token in generated.map.iter().flat_map(|map| map.get_tokens()) {
        let slot = usize::try_from(token.get_dst_line()).ok().and_then(|line| lines.get_mut(line));
        if let Some(slot) = slot.filter(|slot| **slot == 0) {
            *slot = token.get_src_line().saturating_add(1);
        }
    }
    HookModule { js: generated.code, lines }
}

/// A file being stripped, for spans.
struct Spot<'t> {
    id: SourceFileId,
    text: &'t str,
}

impl Spot<'_> {
    /// The span of bytes `start..end`.
    fn span(&self, start: u32, end: u32) -> SourceSpan {
        let at = |offset: u32| position(self.text, usize::try_from(offset).unwrap_or(usize::MAX));
        SourceSpan::new(self.id, at(start), at(end))
    }

    /// V1013 for an oxc error, at its first label.
    fn invalid(&self, error: &OxcDiagnostic) -> Diagnostic {
        let label = error.labels.first();
        let start = label.map_or(0, oxc_span::LabeledSpan::offset);
        let end = label.map_or(start, |label| start.saturating_add(label.len()));
        let message = format!("the hook file is not valid TypeScript or JavaScript: {}", error.message);
        let label = label.and_then(|label| label.label()).unwrap_or("here").to_owned();
        let diagnostic = Diagnostic::new(Code::V1013, self.span(start, end), message).label(label);
        diagnostic.help(error.help.as_deref().unwrap_or("hooks are ES modules; `tsc --noEmit` reports the same error"))
    }

    /// The `import()` naming the module at `request`.
    fn dynamic(&self, request: Span) -> Import {
        let specifier = slice(self.text, request).trim_matches(['"', '\'', '`']).to_owned();
        Import { specifier, span: self.span(request.start, request.end), dynamic: true }
    }

    /// The modules the stripped program still imports or re-exports from.
    fn imports(&self, program: &Program<'_>) -> Vec<Import> {
        let named = program.body.iter().filter_map(|statement| match statement {
            Statement::ImportDeclaration(declaration) => Some(&declaration.source),
            Statement::ExportFromDeclaration(declaration) => Some(&declaration.source),
            Statement::ExportAllDeclaration(declaration) => Some(&declaration.source),
            _ => None,
        });
        let import = |source: &oxc_ast::ast::StringLiteral<'_>| Import {
            specifier: source.value.as_str().unwrap_or_default().to_owned(),
            span: self.span(source.span.start, source.span.end),
            dynamic: false,
        };
        named.map(import).collect()
    }
}

/// The text `span` covers; empty when it is off the text.
fn slice(text: &str, span: Span) -> &str {
    let at = |offset: u32| usize::try_from(offset).unwrap_or(usize::MAX);
    text.get(at(span.start)..at(span.end)).unwrap_or_default()
}

/// The line and column of byte `offset` in `text`; the end of the text when it is past it.
fn position(text: &str, offset: usize) -> Pos {
    let offset = (0..=offset.min(text.len())).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
    let before = text.get(..offset).unwrap_or_default();
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or_default().chars().count() + 1;
    Pos::new(u32::try_from(line).unwrap_or(u32::MAX), u32::try_from(column).unwrap_or(u32::MAX), offset)
}

#[cfg(test)]
#[path = "script_tests.rs"]
mod tests;
