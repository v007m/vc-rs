//! `$file` request bodies (D272). [`body_files`] finds the literal `$file` paths that the given
//! actions send, each resolved from the folder of the file that writes it, so the engine can
//! read them before a run. The bodies stay as written. A whole-body `$file` sits at the empty
//! pointer, and a multipart part at `/name` or `/name/index` (D274). Parts are read only when
//! the body's `content-type` is written as `multipart/form-data`. A templated path is not read,
//! and the executor fails it with V5111.

use indexmap::IndexMap;
use vc_model::{
    Action, ActionId, ActionKind, Api, BodySpec, FILE_KEY, Program, SourcePath, SourceSpan, Template, TemplateValue,
};

use crate::link::folder;

/// Where a `$file` sits: the action that sends the body, and the JSON pointer to the file.
pub type FileSlot = (ActionId, String);

/// A `$file`'s project path, or why the path was refused.
pub type FilePath = Result<SourcePath, String>;

/// What an action sends as its body: the body, where it is written, and the header levels
/// over it, the last one winning.
struct Sent<'p> {
    body: &'p BodySpec,
    span: SourceSpan,
    headers: Vec<&'p IndexMap<String, Template>>,
}

/// The literal `$file`s in the bodies that `actions` send, each with its project path or why
/// the path was refused (D272).
#[must_use]
pub fn body_files(program: &Program, actions: impl IntoIterator<Item = ActionId>) -> Vec<(FileSlot, FilePath)> {
    let mut found = Vec::new();
    for id in actions {
        let Some(sent) = sent(program, id) else { continue };
        let folder = program.files.get(sent.span.file).map(|file| folder(file.path.as_str()));
        let folder = folder.unwrap_or_default();
        found.extend(literals(&sent).into_iter().map(|(pointer, written)| ((id, pointer), resolved(&folder, written))));
    }
    found
}

/// The body action `id` sends, if it sends one: an inline request's, or a call's (the step's
/// own body, else its api's).
fn sent(program: &Program, id: ActionId) -> Option<Sent<'_>> {
    let action = usize::try_from(id.index()).ok().and_then(|at| program.actions.get(at))?;
    match &action.kind {
        ActionKind::Inline(inline) => {
            let headers = vec![&program.config.defaults.headers, &inline.headers];
            Some(Sent { body: &inline.body, span: action.span, headers })
        }
        ActionKind::Call(api) => Some(called(program, program.apis.get(api)?, action)),
        _ => None,
    }
}

fn called<'p>(program: &'p Program, api: &'p Api, action: &'p Action) -> Sent<'p> {
    let group = program.api_groups.get(&api.group).map(|group| &group.headers);
    let mut headers = vec![&program.config.defaults.headers];
    headers.extend(group);
    headers.extend([&api.headers, &action.headers]);
    let (body, span) = action.body.as_ref().map_or((&api.body, api.span), |body| (body, action.span));
    Sent { body, span, headers }
}

/// Each literal `$file` path in `sent`'s body, with its pointer.
fn literals<'p>(sent: &Sent<'p>) -> Vec<(String, &'p str)> {
    match sent.body {
        BodySpec::File(template) => template.as_literal().map(|path| (String::new(), path)).into_iter().collect(),
        BodySpec::Json(value) | BodySpec::Extend { changes: value, .. } if multipart(&sent.headers) => parts(value),
        _ => Vec::new(),
    }
}

/// The literal `$file` parts of a multipart map: a field's value, or an item of its list.
fn parts(value: &TemplateValue) -> Vec<(String, &str)> {
    let TemplateValue::Map(fields) = value else { return Vec::new() };
    let mut found = Vec::new();
    for (name, value) in fields {
        if let TemplateValue::List(items) = value {
            let items = items.iter().enumerate();
            found.extend(items.filter_map(|(at, item)| Some((vc_model::part_pointer(name, Some(at)), literal(item)?))));
        } else {
            found.extend(literal(value).map(|path| (vc_model::part_pointer(name, None), path)));
        }
    }
    found
}

/// The path of a single-key `$file` map written as plain text.
fn literal(value: &TemplateValue) -> Option<&str> {
    let TemplateValue::Map(map) = value else { return None };
    let Some(TemplateValue::String(template)) = map.get(FILE_KEY).filter(|_| map.len() == 1) else { return None };
    template.as_literal()
}

/// Whether the last `content-type` the header levels write is a literal multipart one.
fn multipart(levels: &[&IndexMap<String, Template>]) -> bool {
    let written = levels.iter().rev().find_map(|headers| content_type(headers));
    written.and_then(Template::as_literal).is_some_and(vc_model::is_multipart)
}

/// The last `content-type` one header level writes, its name in any case.
fn content_type(headers: &IndexMap<String, Template>) -> Option<&Template> {
    headers.iter().rev().find(|(name, _)| name.eq_ignore_ascii_case("content-type")).map(|(_, value)| value)
}

/// `written` resolved from `folder`, or why it was refused.
fn resolved(folder: &[&str], written: &str) -> FilePath {
    let path = vc_model::resolve_path(folder, written).ok_or_else(|| "it leads out of the project".to_owned())?;
    SourcePath::new(&path).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "body_files_tests.rs"]
mod tests;
