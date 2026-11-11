//! The compile-parts stage: compiles each leaf the shape-check found into a [`Piece`].
//!
//! Templates are parsed, their roots checked for the spot (D125) and their pipe chains checked
//! (D145); durations, sizes and statuses are parsed; matchers are validated (V2306); CEL is
//! checked with [`CelCompiler::check`]. Which step ids a template may read is the linker's work,
//! so a step root is left alone here unless it is a bare `status`, `headers` or `body` (D150).
//! CEL is checked whole, so it may read the `id` of any action in the same file.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{
    CelProgram, CelSource, Duration, Matcher, Namespace, Part, Reference, ScalarSource, ScalarStyle, Size, SourceSpan,
    StringTable, Sym, Template, TemplateValue,
};
use vc_parser::{Node, Value};
use vc_template::{CelCompiler, PipeRegistry, Roots, TemplateError, ValueType};

use crate::shape_check;
use crate::shape_check::{Leaf, LeafTy, Walk};
use crate::shapes::{Scope, Unit};
use crate::{matchers, value};

/// One compiled value, keyed by its node's span in a [`FileUnit`](crate::stage::FileUnit).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Piece {
    /// Text with templates.
    Template(Template),
    /// A duration.
    Duration(Duration),
    /// A size.
    Size(Size),
    /// A status check or another matcher.
    Matcher(Matcher),
    /// A checked CEL expression.
    Cel(CelProgram),
    /// Any value.
    Value(TemplateValue),
}

/// What compiling parts needs besides the file: built once per compile.
pub struct Tools {
    cel: CelCompiler,
    pipes: PipeRegistry,
}

impl Tools {
    /// The CEL compiler and the built-in pipes.
    pub fn new() -> Self {
        Self { cel: CelCompiler::new(), pipes: PipeRegistry::builtin() }
    }
}

/// The state of compiling one file's parts.
pub struct Cx<'a> {
    /// The file's text, to tell how each scalar is quoted.
    pub text: &'a str,
    /// Interned names.
    pub strings: &'a mut StringTable,
    /// The CEL compiler and pipes.
    pub tools: &'a Tools,
    /// The `id` of every action in the file.
    pub steps: Vec<Sym>,
    /// Every problem found.
    pub diagnostics: Diagnostics,
}

/// A node and the roots available at it.
#[derive(Clone, Copy)]
pub struct At<'n> {
    /// The node.
    pub node: &'n Node,
    /// The roots its templates may read.
    pub scope: Scope,
}

impl At<'_> {
    /// The node itself with another scope.
    pub const fn child<'c>(&self, node: &'c Node) -> At<'c> {
        At { node, scope: self.scope }
    }
}

/// Compiles every leaf of `walk` under `root`.
pub fn compile(root: &Node, walk: &Walk, cx: &mut Cx<'_>) -> IndexMap<SourceSpan, Piece> {
    let mut pieces = IndexMap::new();
    for leaf in &walk.leaves {
        let Some(node) = shape_check::at(root, &leaf.path) else { continue };
        if let Some(piece) = piece(cx, At { node, scope: leaf.scope }, leaf) {
            if let (Piece::Matcher(matcher), Some(key)) = (&piece, list_key(root, leaf)) {
                matchers::on_list(cx, matcher, key);
            }
            pieces.insert(node.span, piece);
        }
    }
    pieces
}

/// The key a matcher leaf sits under when it holds `[*]`, so the matcher sees a list.
fn list_key<'n>(root: &'n Node, leaf: &Leaf) -> Option<&'n str> {
    let LeafTy::Matcher(_) = leaf.ty else { return None };
    let (last, parent) = leaf.path.split_last()?;
    let Value::Map(entries) = &shape_check::at(root, parent)?.value else { return None };
    entries.get(*last).map(|entry| entry.key.as_str()).filter(|key| key.contains("[*]"))
}

fn piece(cx: &mut Cx<'_>, at: At<'_>, leaf: &Leaf) -> Option<Piece> {
    match leaf.ty {
        LeafTy::Template => template(cx, at).map(Piece::Template),
        LeafTy::Duration => unit(cx, at, |text, span| vc_template::parse_duration(text, span).map(Piece::Duration)),
        LeafTy::Size => unit(cx, at, |text, span| vc_template::parse_size(text, span).map(Piece::Size)),
        LeafTy::Status => status(cx, at).map(Piece::Matcher),
        LeafTy::Matcher(unit) => matchers::matcher(cx, at, unit).map(Piece::Matcher),
        LeafTy::Cel => cel(cx, at).map(Piece::Cel),
        LeafTy::Value => value::lower(cx, at).map(Piece::Value),
    }
}

type UnitParser = fn(&str, SourceSpan) -> Result<Piece, Box<Diagnostic>>;

/// A duration or size; text holding `${` is a template that gives one at run time.
fn unit(cx: &mut Cx<'_>, at: At<'_>, parse: UnitParser) -> Option<Piece> {
    let text = at.node.as_scalar()?.text.as_str();
    if text.contains("${") {
        return template(cx, at).map(Piece::Template);
    }
    cx.check(parse(text, at.node.span))
}

/// A code, a class, a list of them, or a matcher; a template gives a code at run time.
fn status(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let items: Vec<&Node> = match &at.node.value {
        Value::Map(_) => return matchers::status(cx, at),
        Value::Seq(items) => items.iter().collect(),
        Value::Scalar(scalar) if scalar.text.contains("${") => return value::lower(cx, at).map(Matcher::Equals),
        Value::Scalar(_) => vec![at.node],
    };
    let patterns: Vec<_> = items.into_iter().map(|item| status_pattern(cx, item)).collect();
    patterns.into_iter().collect::<Option<Vec<_>>>().map(Matcher::Status)
}

fn status_pattern(cx: &mut Cx<'_>, node: &Node) -> Option<vc_model::StatusPattern> {
    let Some(scalar) = node.as_scalar() else {
        let message = format!("expected a status code or class, found {}", node.kind_name());
        cx.push(Diagnostic::new(Code::V2302, node.span, message));
        return None;
    };
    cx.check(vc_template::parse_status(&scalar.text, node.span))
}

/// A template, with its roots and pipe chains checked.
pub fn template(cx: &mut Cx<'_>, at: At<'_>) -> Option<Template> {
    let source = cx.source(at.node)?;
    let text = at.node.as_scalar()?.text.as_str();
    let parsed = vc_template::parse(text, source, cx.strings);
    let template = cx.check_template(parsed, at.node.span)?;
    let roots = roots(at.scope);
    for reference in template.parts.iter().filter_map(reference) {
        cx.check_reference(reference, &roots);
    }
    Some(template)
}

const fn reference(part: &Part) -> Option<&Reference> {
    match part {
        Part::Ref(reference) => Some(reference),
        Part::Literal(_) => None,
    }
}

/// A CEL expression, checked against the roots of the spot and the file's action ids.
pub fn cel(cx: &mut Cx<'_>, at: At<'_>) -> Option<CelProgram> {
    let style = cx.source(at.node)?.style;
    let text = at.node.as_scalar()?.text.clone();
    let source = CelSource { text, span: at.node.span, style };
    let roots = cx.steps.iter().fold(roots(at.scope), |roots, step| roots.with_step(*step));
    let checked = cx.tools.cel.check(&source, &roots, cx.strings);
    cx.check_template(checked, at.node.span)
}

/// Adds a scope's roots.
type AddRoots = fn(Roots) -> Roots;

/// The roots a scope adds (D125).
const SCOPE_ROOTS: &[(Scope, AddRoots)] = &[
    (Scope::CALL, Roots::with_call),
    (Scope::EXAMPLES, Roots::with_examples),
    (Scope::LOOP, Roots::in_loop),
    (Scope::FOR_EACH, Roots::in_for_each),
];

fn roots(scope: Scope) -> Roots {
    SCOPE_ROOTS.iter().filter(|(added, _)| scope.has(*added)).fold(Roots::new(), |roots, (_, add)| add(roots))
}

/// Bare names that mean `response.<name>` unless a step has that id (D150).
const BARE_FIELDS: &[&str] = &["status", "headers", "body"];

impl Cx<'_> {
    /// Records a problem.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// The value, or `None` after recording why there is none.
    pub fn check<T>(&mut self, result: Result<T, Box<Diagnostic>>) -> Option<T> {
        result.map_err(|diagnostic| self.push(*diagnostic)).ok()
    }

    fn check_template<T>(&mut self, result: Result<T, TemplateError>, span: SourceSpan) -> Option<T> {
        result
            .map_err(|error| match error {
                TemplateError::Syntax(diagnostic) => self.push(*diagnostic),
                other => self.push(Diagnostic::new(Code::V2305, span, other.to_string())),
            })
            .ok()
    }

    fn check_reference(&mut self, reference: &Reference, roots: &Roots) {
        if self.is_step(reference.ns) {
            return;
        }
        if let Err(diagnostic) = roots.check(reference, self.strings) {
            self.push(*diagnostic);
        }
        let chain = self.tools.pipes.check_chain(&reference.pipes, ValueType::Any);
        self.check(chain);
    }

    /// Whether `ns` is a step id the linker checks: any step but a bare field no action is named.
    fn is_step(&self, ns: Namespace) -> bool {
        let Namespace::Step(sym) = ns else { return false };
        let bare = self.strings.resolve(sym).is_some_and(|name| BARE_FIELDS.contains(&name));
        !bare || self.steps.contains(&sym)
    }

    /// How the scalar at `node` is written, for exact spans.
    pub fn source(&self, node: &Node) -> Option<ScalarSource> {
        let scalar = node.as_scalar()?;
        let written = self.text.get(node.span.byte_range()).unwrap_or_default();
        Some(ScalarSource { span: node.span, style: style(scalar.style, written) })
    }
}

/// The model's style for a parser style; `written` is the scalar's source, quotes included.
fn style(style: vc_parser::ScalarStyle, written: &str) -> ScalarStyle {
    match style {
        vc_parser::ScalarStyle::Plain => ScalarStyle::Plain,
        vc_parser::ScalarStyle::SingleQuoted => ScalarStyle::SingleQuoted { escaped: inner(written).contains("''") },
        vc_parser::ScalarStyle::DoubleQuoted => ScalarStyle::DoubleQuoted { escaped: written.contains('\\') },
        vc_parser::ScalarStyle::Literal | vc_parser::ScalarStyle::Folded => ScalarStyle::Block,
    }
}

/// The text between a quoted scalar's quotes.
fn inner(written: &str) -> &str {
    written.get(1..written.len().saturating_sub(1)).unwrap_or_default()
}

#[cfg(test)]
mod tests;
