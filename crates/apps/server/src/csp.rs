//! The `Content-Security-Policy` header. It is built here, never copied from the page: only the
//! `'sha256-…'` sources of the embedded `index.html`'s own CSP meta are taken from it, so a meta
//! that allowed more (`'unsafe-inline'`, another host) cannot widen the policy.

/// The policy when there is no page, or its script hashes cannot be read: no inline script runs.
pub const FALLBACK: &str = "default-src 'self'; frame-ancestors 'none'";

/// Why a page gave no script hashes: it is not UTF-8.
const NOT_UTF8: &str = "the web UI's index.html is not UTF-8, so its CSP script hashes cannot be read";

/// Why a page gave no script hashes: no CSP meta lists one.
const NO_HASHES: &str =
    "the web UI's index.html has no content-security-policy meta whose script-src lists a 'sha256-…' hash";

/// The header for the web UI whose `index.html` is `index`; [`FALLBACK`] when there is none.
///
/// # Errors
///
/// Why, when there is a page but its script hashes cannot be read; its inline scripts would
/// not run under [`FALLBACK`], so the caller says so rather than serve a page that never loads.
pub fn policy(index: Option<&[u8]>) -> Result<String, &'static str> {
    let Some(bytes) = index else { return Ok(FALLBACK.to_owned()) };
    let html = std::str::from_utf8(bytes).map_err(|_| NOT_UTF8)?;
    let hashes = script_hashes(html);
    if hashes.is_empty() {
        return Err(NO_HASHES);
    }
    Ok(format!(
        "default-src 'self'; script-src 'self' {}; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
        hashes.join(" ")
    ))
}

/// The `'sha256-…'` sources of the `script-src` directive in the page's first CSP meta.
fn script_hashes(html: &str) -> Vec<&str> {
    let Some(content) = csp_meta(html) else { return Vec::new() };
    let script_src = content.split(';').find_map(|directive| {
        let mut words = directive.split_ascii_whitespace();
        words.next().is_some_and(|name| name.eq_ignore_ascii_case("script-src")).then_some(words)
    });
    script_src.map(|sources| sources.filter(|source| is_hash(source)).collect()).unwrap_or_default()
}

/// The `content` of the first `<meta http-equiv="content-security-policy">`, in any case.
fn csp_meta(html: &str) -> Option<&str> {
    let lower = html.to_ascii_lowercase();
    let starts = lower.match_indices("<meta").map(|(at, found)| at + found.len());
    starts.filter_map(|at| html.get(at..)).filter_map(tag).find_map(|tag| {
        let equiv = attribute(tag, "http-equiv")?;
        equiv.eq_ignore_ascii_case("content-security-policy").then(|| attribute(tag, "content")).flatten()
    })
}

/// The attributes of a tag, up to the first `>` outside a quoted value.
fn tag(rest: &str) -> Option<&str> {
    let mut quote = None;
    let end = rest.char_indices().find_map(|(at, c)| {
        match quote {
            Some(open) if c == open => quote = None,
            None if matches!(c, '"' | '\'') => quote = Some(c),
            None if c == '>' => return Some(at),
            _ => {}
        }
        None
    })?;
    rest.get(..end)
}

/// The value of the attribute `name`, in any case, in a tag's attributes: double-quoted,
/// single-quoted or unquoted. `None` when it is missing or has no value.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if rest.is_empty() {
            return None;
        }
        let end = rest.find(|c: char| c.is_ascii_whitespace() || matches!(c, '=' | '/')).unwrap_or(rest.len());
        let (found, after) = rest.split_at(end);
        let (value, next) = value(after);
        if found.eq_ignore_ascii_case(name) {
            return value;
        }
        rest = next;
    }
}

/// The value after an attribute's name, if it has one, and the attributes after it.
fn value(after: &str) -> (Option<&str>, &str) {
    let Some(raw) = after.trim_start().strip_prefix('=') else { return (None, after) };
    let raw = raw.trim_start();
    if let Some(quote @ ('"' | '\'')) = raw.chars().next() {
        let inner = raw.get(1..).unwrap_or_default();
        return inner.split_once(quote).map_or((Some(inner), ""), |(value, next)| (Some(value), next));
    }
    let end = raw.find(|c: char| c.is_ascii_whitespace()).unwrap_or(raw.len());
    let (value, next) = raw.split_at(end);
    (Some(value), next)
}

/// Whether `source` is `'sha256-<base64>'` and nothing more.
fn is_hash(source: &str) -> bool {
    let base64 = source.strip_prefix("'sha256-").and_then(|rest| rest.strip_suffix('\''));
    base64.is_some_and(|text| {
        !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    })
}

#[cfg(test)]
#[path = "csp_tests.rs"]
mod tests;
