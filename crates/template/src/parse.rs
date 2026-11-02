use vc_diagnostics::Diagnostic;
use vc_model::{Namespace, Part, Path, PipeCall, Reference, SourceSpan, StringTable, Template};

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

/// The name a root is written with, or `None` for a step id.
#[must_use]
pub fn root_name(ns: Namespace) -> Option<&'static str> {
    ROOTS.iter().find(|(_, root)| *root == ns).map(|(name, _)| *name)
}

/// Parses the text of a YAML scalar into a [`Template`].
///
/// `scalar` is where the scalar is written; spans of references, pipes and errors are mapped
/// into it (exactly for one-line plain or quoted scalars without escapes, otherwise the whole
/// scalar). Roots that are not fixed names are action or step ids, interned into `strings`.
///
/// # Errors
///
/// The first syntax error, with the span of the character where parsing stopped.
pub fn parse(text: &str, scalar: SourceSpan, strings: &mut StringTable) -> Result<Template, Box<Diagnostic>> {
    let map = SpanMap::new(text, scalar);
    let raw = grammar::template(text).map_err(|error| {
        let detail = error.inner().to_string().replace('\n', "; ");
        Box::new(
            Diagnostic::new(
                codes::TEMPLATE_SYNTAX,
                map.char_span(error.offset()),
                format!("invalid template: {detail}"),
            )
            .help("write `${namespace.path | pipe}`; write `$${` for a literal `${`"),
        )
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

fn reference(raw: RawRef, map: &SpanMap<'_>, strings: &mut StringTable) -> Result<Reference, Box<Diagnostic>> {
    let span = map.span(raw.range);
    let ns = namespace(&raw.root, span, strings)?;
    if raw.args.is_some() && ns != Namespace::Fake {
        let message = format!("`{}` takes no arguments; only `fake` generators do", raw.root);
        return Err(Box::new(Diagnostic::new(codes::TEMPLATE_SYNTAX, span, message).help("remove the `(…)`")));
    }
    let pipes = raw.pipes.into_iter().map(|pipe| pipe_call(pipe, map)).collect();
    Ok(Reference { ns, path: Path { segments: raw.path }, args: raw.args.unwrap_or_default(), pipes, span })
}

fn namespace(root: &str, span: SourceSpan, strings: &mut StringTable) -> Result<Namespace, Box<Diagnostic>> {
    if let Some((_, ns)) = ROOTS.iter().find(|(name, _)| *name == root) {
        return Ok(*ns);
    }
    strings
        .intern(root)
        .map(Namespace::Step)
        .map_err(|error| Box::new(Diagnostic::new(codes::TEMPLATE_SYNTAX, span, error.to_string())))
}

fn pipe_call(raw: RawPipe, map: &SpanMap<'_>) -> PipeCall {
    PipeCall { name: raw.name, args: raw.args, name_span: map.span(raw.name_range), span: map.span(raw.range) }
}
