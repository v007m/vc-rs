//! Walks a JSON value along a template path: `.key`, `[n]` (negative from the end), `[*]` and
//! quoted keys (LLD "Rendering").

use std::fmt::Write as _;

use serde_json::Value;
use vc_model::Segment;

use crate::RenderError;

/// The longest value shown in a message, in characters.
const PREVIEW: usize = 120;

/// The value at `path` inside `value`; `[*]` gives the list of the rest of the path applied to
/// each element. `root` names the value in messages: `response.body`.
///
/// # Errors
///
/// A missing [`RenderError`] naming the path and the value found where the path stops.
pub fn select(root: &str, value: &Value, path: &[Segment]) -> Result<Value, RenderError> {
    walk(value, path, 0).map_err(|stop| {
        let wanted = format!("{root}{}", path_text(&path[..=stop.depth]));
        let found = format!("{root}{}", path_text(&path[..stop.depth]));
        RenderError::missing(format!("`{wanted}` is missing: `{found}` is {}", stop.actual))
    })
}

/// Where a walk stopped: the segment it could not take and the value it was at.
struct Stop {
    depth: usize,
    actual: String,
}

fn walk(value: &Value, path: &[Segment], depth: usize) -> Result<Value, Stop> {
    let Some((first, rest)) = path.split_first() else { return Ok(value.clone()) };
    if let (Segment::All, Value::Array(items)) = (first, value) {
        return items.iter().map(|item| walk(item, rest, depth + 1)).collect::<Result<_, _>>().map(Value::Array);
    }
    child(value, first).map_or_else(|| Err(Stop { depth, actual: preview(value) }), |next| walk(next, rest, depth + 1))
}

fn child<'v>(value: &'v Value, segment: &Segment) -> Option<&'v Value> {
    match (segment, value) {
        (Segment::Key(key), Value::Object(map)) => map.get(key),
        (Segment::Index(index), Value::Array(items)) => position(*index, items.len()).and_then(|at| items.get(at)),
        _ => None,
    }
}

/// The position of `index` in a list of `len`; a negative index counts from the end.
#[must_use]
pub fn position(index: i64, len: usize) -> Option<usize> {
    let at = usize::try_from(index.unsigned_abs()).ok()?;
    if index < 0 { len.checked_sub(at) } else { Some(at) }
}

/// A path as written after its root: `.items[0]["first name"][*]`.
///
/// A key that is not a name is quoted, in single quotes when it holds `"`; quoted keys have no
/// escapes, so a key holding both quotes is written in double quotes and does not read back.
#[must_use]
pub fn path_text(path: &[Segment]) -> String {
    path.iter().fold(String::new(), |mut text, segment| {
        let _ = match segment {
            Segment::Key(key) if is_ident(key) => write!(text, ".{key}"),
            Segment::Key(key) if key.contains('"') && !key.contains('\'') => write!(text, "['{key}']"),
            Segment::Key(key) => write!(text, "[\"{key}\"]"),
            Segment::Index(index) => write!(text, "[{index}]"),
            Segment::All => write!(text, "[*]"),
        };
        text
    })
}

/// A path as an `expect.errors` entry writes it, without a root: `users[2].email`; the same text
/// as [`path_text`] without its leading `.`.
#[must_use]
pub fn dotted(path: &[Segment]) -> String {
    let mut text = path_text(path);
    if text.starts_with('.') {
        text.remove(0);
    }
    text
}

fn is_ident(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A value as compact JSON for a message, cut to [`PREVIEW`] characters.
#[must_use]
pub fn preview(value: &Value) -> String {
    let text = value.to_string();
    match text.char_indices().nth(PREVIEW) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use vc_model::Segment;

    use super::{dotted, path_text};

    fn key(name: &str) -> Segment {
        Segment::Key(name.to_owned())
    }

    #[test]
    fn dots_names_and_quotes_any_other_key_in_single_quotes_when_it_holds_a_double_one() {
        assert_eq!(dotted(&[key("users"), Segment::Index(2), key("email")]), "users[2].email");
        assert_eq!(dotted(&[Segment::Index(0), key("a"), Segment::All]), "[0].a[*]");
        assert_eq!(dotted(&[key("a.b"), key("c[1]"), key("d]")]), "[\"a.b\"][\"c[1]\"][\"d]\"]");
        assert_eq!(dotted(&[key("x"), key("say \"hi\"")]), "x['say \"hi\"']");
        assert_eq!(dotted(&[key("first name"), key("*"), key(""), key("1")]), "[\"first name\"][\"*\"][\"\"][\"1\"]");
        assert_eq!(dotted(&[key("it's \"x\"")]), "[\"it's \"x\"\"]", "no escapes: both quotes do not read back");
        assert_eq!(dotted(&[]), "");
        assert_eq!(path_text(&[key("say \"hi\""), key("ok")]), "['say \"hi\"'].ok");
    }
}
