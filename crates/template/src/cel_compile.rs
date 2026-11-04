use std::fmt;
use std::sync::Arc;

use cel::common::ast::{IdedExpr, SourceInfo};
use cel::{Env, ParseError, ParseErrors};
use vc_diagnostics::Diagnostic;
use vc_model::{CelProgram, CelSource, Path, Reference, ScalarSource, StringTable};

use crate::cel_eval::{self, CelExpr};
use crate::cel_lex::{self, VAR_ALIAS};
use crate::cel_refs::{self, Problem, Root};
use crate::expr::Expr;
use crate::parse;
use crate::span_map::SpanMap;
use crate::{Roots, TemplateError, codes};

/// Roots of `${…}` templates that CEL cannot read (D125: CEL's activation has no such roots).
const TEMPLATE_ONLY: &[&str] = &["secret", "fake", "dataset", "run"];

/// Checks CEL expressions at compile time and prepares them for evaluation.
///
/// ```
/// use serde_json::json;
/// use vc_model::{CelSource, Namespace, Pos, ScalarStyle, SourceFileId, SourceSpan, StringTable};
/// use vc_template::{Activation, CelCompiler, Roots};
///
/// let text = "response.body.items.map(i, i.price).sum()";
/// let span = SourceSpan::new(SourceFileId::new(0), Pos::new(1, 7, 6), Pos::new(1, 48, 47));
/// let source = CelSource { text: text.into(), span, style: ScalarStyle::Plain };
/// let compiler = CelCompiler::new();
/// let program = compiler.check(&source, &Roots::new().with_call(), &mut StringTable::new())?;
/// let expr = compiler.prepare(&program)?;
/// let response = json!({"body": {"items": [{"price": 2}, {"price": 3}]}});
/// assert_eq!(expr.eval(&Activation::new().with(Namespace::Response, response)), Ok(json!(5)));
/// # Ok::<(), vc_template::TemplateError>(())
/// ```
pub struct CelCompiler {
    env: Arc<Env>,
}

impl CelCompiler {
    /// A compiler with CEL's standard library, plus `sum()`.
    #[must_use]
    pub fn new() -> Self {
        Self { env: Arc::new(Env::stdlib()) }
    }

    /// Checks that `source` parses and that every root it reads is available where it is written.
    /// Pure: used by the compiler for `$cel` values and CEL checks.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Syntax`] with V2305 when the expression does not parse or calls
    /// `timestamp()` or `duration()`, which are not supported yet; V2306 for a literal regex
    /// given to `matches()` that does not compile; V2105 or V2102 for a root not available
    /// here, as for templates, and V2105 for `secret`, `fake`, `dataset` and `run`, which CEL
    /// cannot read. [`TemplateError::Internal`] when `strings` is full.
    pub fn check(
        &self,
        source: &CelSource,
        roots: &Roots,
        strings: &mut StringTable,
    ) -> Result<CelProgram, TemplateError> {
        let map = SpanMap::new(&source.text, ScalarSource { span: source.span, style: source.style });
        let (expr, info, text) = self.parse(source, &map)?;
        let found = cel_refs::roots(&expr, &info, &text).map_err(|problem| reject(problem, &map))?;
        let mut refs = Vec::with_capacity(found.len());
        for root in found {
            let reference = reference(root, &map, strings)?;
            roots.check(&reference, strings).map_err(TemplateError::Syntax)?;
            refs.push(reference);
        }
        Ok(CelProgram { source: source.clone(), refs })
    }

    /// Prepares a checked program for evaluation; the executor does this once per run.
    ///
    /// # Errors
    ///
    /// As [`CelCompiler::check`] for a program it did not produce.
    pub fn prepare(&self, program: &CelProgram) -> Result<Arc<dyn Expr>, TemplateError> {
        let source = &program.source;
        let map = SpanMap::new(&source.text, ScalarSource { span: source.span, style: source.style });
        let (expr, info, text) = self.parse(source, &map)?;
        let found = cel_refs::roots(&expr, &info, &text).map_err(|problem| reject(problem, &map))?;
        let mut bindings: Vec<_> = Vec::with_capacity(found.len());
        for (root, reference) in found.into_iter().zip(&program.refs) {
            let name = if root.name == "var" { VAR_ALIAS.to_owned() } else { root.name };
            if !bindings.iter().any(|(bound, _)| *bound == name) {
                bindings.push((name, reference.ns));
            }
        }
        let mut expr = expr;
        cel_eval::instrument(&mut expr);
        Ok(Arc::new(CelExpr { env: Arc::clone(&self.env), expr, bindings, text: source.text.clone() }))
    }

    fn parse(&self, source: &CelSource, map: &SpanMap<'_>) -> Result<(IdedExpr, SourceInfo, String), TemplateError> {
        let text = cel_lex::hide_var(&source.text).map_err(|range| {
            let message = format!("`{VAR_ALIAS}` is reserved in CEL expressions");
            syntax(Diagnostic::new(codes::TEMPLATE_SYNTAX, map.span(range), message).help("rename it"))
        })?;
        let (expr, info) =
            self.env.parser().parse_with_source_info(&text).map_err(|errors| parse_error(&errors, &text, map))?;
        Ok((expr, info, text))
    }
}

impl Default for CelCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CelCompiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CelCompiler").finish_non_exhaustive()
    }
}

fn reference(root: Root, map: &SpanMap<'_>, strings: &mut StringTable) -> Result<Reference, TemplateError> {
    let span = map.span(root.range);
    if TEMPLATE_ONLY.contains(&root.name.as_str()) {
        let message = format!("`{}` is not available in CEL", root.name);
        let help = "CEL reads response, request, var, env, item, iteration, example and step ids; \
                    set a var from a `${…}` template and read it as `var.<name>`";
        return Err(syntax(Diagnostic::new(codes::ROOT_UNAVAILABLE, span, message).help(help)));
    }
    let ns = parse::namespace(&root.name, strings)?;
    let root_span = map.span(root.ident);
    Ok(Reference { ns, path: Path { segments: root.path }, args: Vec::new(), pipes: Vec::new(), span, root_span })
}

fn reject(problem: Problem, map: &SpanMap<'_>) -> TemplateError {
    match problem {
        Problem::Unsupported(name, range) => {
            let message = format!("`{name}()` is not supported yet");
            let help = "CEL in vc has no timestamp or duration functions yet; compare ISO-8601 strings instead";
            syntax(Diagnostic::new(codes::TEMPLATE_SYNTAX, map.span(range), message).help(help))
        }
        Problem::BadRegex(reason, range) => {
            let message = format!("invalid regex in `matches()`: {reason}");
            let help = "fix the pattern; `matches()` uses the same regex syntax as the `regex` matcher";
            syntax(Diagnostic::new(codes::MATCHER_ARG, map.span(range), message).help(help))
        }
    }
}

fn parse_error(errors: &ParseErrors, text: &str, map: &SpanMap<'_>) -> TemplateError {
    let first =
        errors.errors.iter().find(|error| !error.msg.starts_with("Incomplete")).or_else(|| errors.errors.first());
    let (message, offset) = first.map_or_else(
        || (String::from("not an expression"), text.len()),
        |error| (describe(error), offset(text, error)),
    );
    let diagnostic = Diagnostic::new(codes::TEMPLATE_SYNTAX, map.char_span(offset), format!("invalid CEL: {message}"));
    syntax(diagnostic.help("for example `response.body.items.size() > 0`"))
}

fn describe(error: &ParseError) -> String {
    let message = error.msg.trim_start_matches("Syntax error: ");
    message
        .strip_prefix("reserved identifier: ")
        .map_or_else(|| message.to_owned(), |word| format!("`{word}` is a reserved word"))
}

/// The byte offset of a parse error's position: a 1-based line and a 1-based character column.
fn offset(text: &str, error: &ParseError) -> usize {
    let (line, column) = error.pos;
    let skip = usize::try_from(line.saturating_sub(1)).unwrap_or_default();
    let start: usize = text.split_inclusive('\n').take(skip).map(str::len).sum();
    let column = usize::try_from(column.saturating_sub(1)).unwrap_or_default();
    let rest = text.get(start..).unwrap_or_default();
    rest.char_indices().nth(column).map_or(text.len(), |(at, _)| start + at)
}

fn syntax(diagnostic: Diagnostic) -> TemplateError {
    TemplateError::Syntax(Box::new(diagnostic))
}
