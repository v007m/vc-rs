use vc_diagnostics::Diagnostic;
use vc_model::{Namespace, Part, Path, PipeCall, Reference, ScalarSource, Segment, StringTable, Template};

use crate::TemplateError;
use crate::codes;
use crate::grammar::{self, RawPart, RawPipe, RawRef};
use crate::span_map::SpanMap;

/// The fixed roots by name (D125); any other root is an action or step id.
const ROOTS: &[(&str, Namespace)] = &[
    ("var", Namespace::Var),
    ("env", Namespace::Env),
    ("secret", Namespace::Secret),
    ("response", Namespace::Response),
    ("request", Namespace::Request),
    ("example", Namespace::Example),
    ("item", Namespace::Item),
    ("iteration", Namespace::Iteration),
    ("fake", Namespace::Fake),
    ("dataset", Namespace::Dataset),
    ("run", Namespace::Run),
];

/// Bare names short for `response.<name>` inside `expect` (D125).
const RESPONSE_ALIASES: &[&str] = &["status", "headers", "body"];

/// The name a root is written with, or `None` for a step id.
#[must_use]
pub fn root_name(ns: Namespace) -> Option<&'static str> {
    ROOTS.iter().find(|(_, root)| *root == ns).map(|(name, _)| *name)
}

/// Parses the text of a YAML scalar into a [`Template`].
///
/// `source` is where and how the scalar is written; spans of references, pipes and errors are
/// mapped into it (exactly for one-line plain scalars and unescaped quoted ones, otherwise the
/// whole scalar). A bare `status`, `headers` or `body` root is `response.…` with
/// [`Reference::response_alias`] set. Other roots that are not fixed names are action or step
/// ids, interned into `strings`.
///
/// # Errors
///
/// [`TemplateError::Syntax`] (V2305) with the span of the character where parsing stopped;
/// [`TemplateError::Internal`] when `strings` is full.
pub fn parse(text: &str, source: ScalarSource, strings: &mut StringTable) -> Result<Template, TemplateError> {
    let map = SpanMap::new(text, source);
    let raw = grammar::template(text).map_err(|error| {
        let message = format!("invalid template: {}", grammar::describe(error.inner()));
        let diagnostic = Diagnostic::new(codes::TEMPLATE_SYNTAX, map.char_span(error.offset()), message);
        TemplateError::Syntax(Box::new(
            diagnostic.help("write `${namespace.path | pipe}`; write `$${` for a literal `${`"),
        ))
    })?;
    let mut parts: Vec<Part> = Vec::with_capacity(raw.len());
    for part in raw {
        match (part, parts.last_mut()) {
            (RawPart::Text(text), Some(Part::Literal(last))) => last.push_str(&text),
            (RawPart::Text(text), _) => parts.push(Part::Literal(text)),
            (RawPart::Ref(raw), _) => parts.push(Part::Ref(reference(raw, &map, strings)?)),
        }
    }
    Ok(Template { parts })
}

fn reference(raw: RawRef, map: &SpanMap<'_>, strings: &mut StringTable) -> Result<Reference, TemplateError> {
    let span = map.span(raw.range);
    if raw.args.is_some() && raw.root != "fake" {
        let message = format!("`{}` takes no arguments; only `fake` generators do", raw.root);
        let diagnostic = Diagnostic::new(codes::TEMPLATE_SYNTAX, span, message).help("remove the `(…)`");
        return Err(TemplateError::Syntax(Box::new(diagnostic)));
    }
    let response_alias = RESPONSE_ALIASES.contains(&raw.root.as_str());
    let (ns, mut segments) = if response_alias {
        (Namespace::Response, vec![Segment::Key(raw.root)])
    } else {
        (namespace(&raw.root, strings)?, Vec::new())
    };
    segments.extend(raw.path);
    let pipes = raw.pipes.into_iter().map(|pipe| pipe_call(pipe, map)).collect();
    let args = raw.args.unwrap_or_default();
    Ok(Reference { ns, path: Path { segments }, args, pipes, span, response_alias })
}

fn namespace(root: &str, strings: &mut StringTable) -> Result<Namespace, TemplateError> {
    if let Some((_, ns)) = ROOTS.iter().find(|(name, _)| *name == root) {
        return Ok(*ns);
    }
    Ok(Namespace::Step(strings.intern(root)?))
}

fn pipe_call(raw: RawPipe, map: &SpanMap<'_>) -> PipeCall {
    PipeCall { name: raw.name, args: raw.args, name_span: map.span(raw.name_range), span: map.span(raw.range) }
}
