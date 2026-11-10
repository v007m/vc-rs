//! The `Content-Security-Policy` header. It is built here, never copied from the page: only the
//! `'sha256-…'` sources of the embedded `index.html`'s own CSP meta are taken from it, so a meta
//! that allowed more (`'unsafe-inline'`, another host) cannot widen the policy.

/// The policy when the page has no script hashes: its inline scripts do not run.
const FALLBACK: &str = "default-src 'self'; frame-ancestors 'none'";

/// The header for the web UI whose `index.html` is `index`, if there is one.
pub fn policy(index: Option<&str>) -> String {
    let hashes = index.map(script_hashes).unwrap_or_default();
    if hashes.is_empty() {
        return FALLBACK.to_owned();
    }
    format!(
        "default-src 'self'; script-src 'self' {}; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
        hashes.join(" ")
    )
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

/// The `content` of the first `<meta http-equiv="content-security-policy">`.
fn csp_meta(html: &str) -> Option<&str> {
    html.split("<meta").skip(1).filter_map(tag).find_map(|tag| {
        let equiv = attribute(tag, "http-equiv")?;
        equiv.eq_ignore_ascii_case("content-security-policy").then(|| attribute(tag, "content")).flatten()
    })
}

/// The attributes of a tag, up to the first `>` outside a double-quoted value.
fn tag(rest: &str) -> Option<&str> {
    let mut quoted = false;
    let end = rest.char_indices().find_map(|(at, c)| {
        quoted ^= c == '"';
        (c == '>' && !quoted).then_some(at)
    })?;
    rest.get(..end)
}

/// The double-quoted value of `name` in a tag's attributes.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let after = &rest[at + name.len()..];
        let starts_word = rest[..at].ends_with(|c: char| c.is_ascii_whitespace());
        if let Some(value) = after.strip_prefix("=\"").filter(|_| starts_word) {
            return value.split_once('"').map(|(value, _)| value);
        }
        rest = after;
    }
    None
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
