//! A path into a response body as written in a script (`.items[0]["id"]`), as template text
//! (`.items[0].id`): what a saved value reads.

/// `text`, a run of accesses, as template text; none when one of them is not a plain key or
/// index.
pub fn path(text: &str) -> Option<String> {
    let mut rest = text;
    let mut path = String::new();
    while !rest.is_empty() {
        let (step, after) = access(rest)?;
        path.push_str(&step);
        rest = after;
    }
    Some(path)
}

/// The first access of `text` as template text (`.id`, `[0]`, `["id"]` gives `.id`) and the
/// rest.
fn access(text: &str) -> Option<(String, &str)> {
    if let Some(rest) = text.strip_prefix('.') {
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
        let (key, rest) = rest.split_at_checked(end)?;
        return plain(key).then(|| (format!(".{key}"), rest));
    }
    let (inside, rest) = text.strip_prefix('[')?.split_once(']')?;
    let inside = inside.trim();
    if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
        return Some((format!("[{inside}]"), rest));
    }
    let key = inside.strip_prefix(['"', '\''])?.strip_suffix(['"', '\''])?;
    plain(key).then(|| (format!(".{key}"), rest))
}

/// Whether `key` can follow a `.` in a template.
pub fn plain(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
