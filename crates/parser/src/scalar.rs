use std::fmt::Write as _;

/// How a scalar was written in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarStyle {
    /// Unquoted: `value`.
    Plain,
    /// In single quotes: `'value'`.
    SingleQuoted,
    /// In double quotes, with escapes: `"value"`.
    DoubleQuoted,
    /// A literal block: `|`.
    Literal,
    /// A folded block: `>`.
    Folded,
}

/// What a scalar means under the YAML 1.2 core schema; only plain scalars can be anything but text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarKind {
    /// `null`, `~` or nothing.
    Null,
    /// `true` or `false`.
    Bool,
    /// `12`, `-3`, `0x1F`, `0o17`.
    Int,
    /// `1.5`, `1e3`, `.inf`, `.nan`.
    Float,
    /// Anything else, and every quoted or block scalar.
    String,
}

/// A scalar value: its text as written (quotes removed and escapes applied), style and kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scalar {
    /// The content: `0x1F` stays `0x1F`; the exact source bytes are the node's span.
    pub text: String,
    /// How it was written.
    pub style: ScalarStyle,
    /// What it resolves to.
    pub kind: ScalarKind,
}

impl Scalar {
    /// A scalar with its kind resolved from `text` and `style`.
    #[must_use]
    pub fn new(text: impl Into<String>, style: ScalarStyle) -> Self {
        let text = text.into();
        let kind = if style == ScalarStyle::Plain { resolve(&text) } else { ScalarKind::String };
        Self { text, style, kind }
    }
}

const NULLS: &[&str] = &["", "~", "null", "Null", "NULL"];
const BOOLS: &[&str] = &["true", "True", "TRUE", "false", "False", "FALSE"];
const INFINITIES: &[&str] = &[".inf", ".Inf", ".INF"];
const NANS: &[&str] = &[".nan", ".NaN", ".NAN"];

/// The core-schema kind of a plain scalar's text.
fn resolve(text: &str) -> ScalarKind {
    if NULLS.contains(&text) {
        ScalarKind::Null
    } else if BOOLS.contains(&text) {
        ScalarKind::Bool
    } else if is_int(text) {
        ScalarKind::Int
    } else if is_float(text) {
        ScalarKind::Float
    } else {
        ScalarKind::String
    }
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn unsigned(text: &str) -> &str {
    text.strip_prefix(['-', '+']).unwrap_or(text)
}

fn is_int(text: &str) -> bool {
    if let Some(hex) = text.strip_prefix("0x") {
        return !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    if let Some(octal) = text.strip_prefix("0o") {
        return !octal.is_empty() && octal.bytes().all(|b| (b'0'..=b'7').contains(&b));
    }
    digits(unsigned(text))
}

fn is_float(text: &str) -> bool {
    let body = unsigned(text);
    if INFINITIES.contains(&body) || NANS.contains(&text) {
        return true;
    }
    let (mantissa, exponent) = body.split_once(['e', 'E']).map_or((body, None), |(m, e)| (m, Some(e)));
    let exponent_ok = exponent.is_none_or(|e| digits(unsigned(e)));
    let mantissa_ok = match mantissa.split_once('.') {
        None => digits(mantissa),
        Some((whole, fraction)) => {
            (whole.is_empty() && digits(fraction)) || (digits(whole) && (fraction.is_empty() || digits(fraction)))
        }
    };
    exponent_ok && mantissa_ok
}

/// The text written in the house style: plain when YAML reads it back as the same text,
/// otherwise double-quoted with escapes.
pub fn render(text: &str) -> String {
    if needs_quotes(text) { double_quote(text) } else { text.to_owned() }
}

const INDICATORS: &[char] =
    &['-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`'];

fn needs_quotes(text: &str) -> bool {
    resolve(text) != ScalarKind::String
        || text.starts_with(INDICATORS)
        || text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.ends_with(':')
        || text.contains(": ")
        || text.contains(" #")
        || text.contains(char::is_control)
}

/// `text` as a double-quoted YAML scalar.
pub fn double_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::{ScalarKind as K, ScalarStyle, double_quote, render, resolve};

    #[test]
    fn plain_scalars_resolve_by_the_core_schema() {
        let cases = [
            ("~", K::Null),
            ("", K::Null),
            ("NULL", K::Null),
            ("True", K::Bool),
            ("false", K::Bool),
            ("12", K::Int),
            ("-3", K::Int),
            ("+0", K::Int),
            ("0x1F", K::Int),
            ("0o17", K::Int),
            ("1.5", K::Float),
            ("-.5", K::Float),
            ("1.", K::Float),
            ("1e3", K::Float),
            ("2.5E-3", K::Float),
            ("-.inf", K::Float),
            (".NaN", K::Float),
            ("yes", K::String),
            ("0x", K::String),
            ("0o8", K::String),
            ("1.2.3", K::String),
            (".", K::String),
            ("1e", K::String),
            ("-.nan", K::String),
            ("12ms", K::String),
        ];
        for (text, kind) in cases {
            assert_eq!(resolve(text), kind, "{text:?}");
        }
    }

    #[test]
    fn quoted_and_block_scalars_are_always_text() {
        assert_eq!(super::Scalar::new("12", ScalarStyle::DoubleQuoted).kind, K::String);
        assert_eq!(super::Scalar::new("12", ScalarStyle::Plain).kind, K::Int);
    }

    #[test]
    fn render_quotes_only_what_yaml_would_misread() {
        assert_eq!(render("orders#place"), "orders#place");
        assert_eq!(render("${var.userId}"), "${var.userId}");
        assert_eq!(render("12"), "\"12\"");
        assert_eq!(render("see #212"), "\"see #212\"");
        assert_eq!(render("a: b"), "\"a: b\"");
        assert_eq!(render(""), "\"\"");
        assert_eq!(double_quote("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
    }
}
