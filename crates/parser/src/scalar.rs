use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::{Syntax, json};

/// How a scalar was written in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
pub fn resolve(text: &str) -> ScalarKind {
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

impl ScalarKind {
    /// The kind in words, for messages: "an integer".
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool => "a boolean",
            Self::Int => "an integer",
            Self::Float => "a float",
            Self::String => "text",
        }
    }
}

/// The kind `text` reads back as when written plain in `syntax`. JSON has no plain text, so
/// there anything but a JSON number, `true`, `false` or `null` is text.
pub fn resolve_in(syntax: Syntax, text: &str) -> ScalarKind {
    if syntax == Syntax::Json && !json::is_literal(text) { ScalarKind::String } else { resolve(text) }
}

/// How a new value is written in place of an old one: the file's syntax, the old value's quote
/// style, the kind the new one must read back as, and whether it sits inside a flow collection
/// (`[..]` or `{..}`).
#[derive(Clone, Copy, Debug)]
pub struct Spelling {
    /// The file's syntax.
    pub syntax: Syntax,
    /// The quote style written before; kept for text.
    pub style: ScalarStyle,
    /// The kind the written value reads back as.
    pub kind: ScalarKind,
    /// Inside `[..]` or `{..}`, where `,[]{}` end a plain value.
    pub flow: bool,
}

impl Spelling {
    /// `text` written to read back as `kind`, or `None` when it cannot. Text keeps a quoted
    /// style (single quotes fall back to double for control characters) and stays plain when
    /// YAML reads it back as the same text; other kinds are written plain and must resolve to
    /// that kind, so an empty value is refused (write `null`). In JSON, text is always
    /// double-quoted and other kinds must be JSON numbers, `true`, `false` or `null`.
    #[must_use]
    pub fn write(self, text: &str) -> Option<String> {
        if self.syntax == Syntax::Json {
            return write_json(self.kind, text);
        }
        if self.kind != ScalarKind::String {
            return (!text.is_empty() && resolve(text) == self.kind).then(|| text.to_owned());
        }
        Some(match self.style {
            ScalarStyle::DoubleQuoted => double_quote(text),
            ScalarStyle::SingleQuoted if !text.contains(char::is_control) => single_quote(text),
            _ => write_text(text, self.flow),
        })
    }
}

fn write_json(kind: ScalarKind, text: &str) -> Option<String> {
    if kind == ScalarKind::String {
        return Some(double_quote(text));
    }
    (json::is_literal(text) && resolve(text) == kind).then(|| text.to_owned())
}

/// Text in the house style: plain when YAML reads it back as the same text, otherwise
/// double-quoted with escapes. `flow` is true inside `[..]` or `{..}`.
pub fn write_text(text: &str, flow: bool) -> String {
    if needs_quotes(text, flow) { double_quote(text) } else { text.to_owned() }
}

const INDICATORS: &[char] =
    &['-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`'];
const FLOW_INDICATORS: &[char] = &[',', '[', ']', '{', '}'];

fn needs_quotes(text: &str, flow: bool) -> bool {
    resolve(text) != ScalarKind::String
        || (flow && text.contains(FLOW_INDICATORS))
        || text.starts_with(INDICATORS)
        || text.starts_with(char::is_whitespace)
        || text.ends_with(char::is_whitespace)
        || text.ends_with(':')
        || text.contains(": ")
        || text.contains(" #")
        || text.contains(char::is_control)
}

/// `text` as a single-quoted YAML scalar; `text` must hold no control characters.
fn single_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// `text` as a double-quoted YAML scalar, which is also a JSON string.
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
    use super::{ScalarKind as K, ScalarStyle, Spelling, double_quote, resolve, resolve_in, write_text};
    use crate::Syntax;

    fn render(text: &str) -> String {
        write_text(text, false)
    }

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

    #[test]
    fn flow_context_quotes_flow_indicators_anywhere() {
        for text in ["x, y", "a]b", "a[b", "a{b", "a}b"] {
            assert_eq!(write_text(text, true), format!("\"{text}\""));
            assert_eq!(write_text(text, false), text);
        }
        assert_eq!(write_text("plain", true), "plain");
    }

    #[test]
    fn spelling_keeps_the_quote_style_and_the_kind() {
        let spell = |style, kind, text| Spelling { syntax: Syntax::Yaml, style, kind, flow: false }.write(text);
        assert_eq!(spell(ScalarStyle::SingleQuoted, K::String, "it's").as_deref(), Some("'it''s'"));
        assert_eq!(spell(ScalarStyle::SingleQuoted, K::String, "a\nb").as_deref(), Some("\"a\\nb\""));
        assert_eq!(spell(ScalarStyle::DoubleQuoted, K::String, "x").as_deref(), Some("\"x\""));
        assert_eq!(spell(ScalarStyle::Plain, K::String, "x").as_deref(), Some("x"));
        assert_eq!(spell(ScalarStyle::Plain, K::Int, "-3").as_deref(), Some("-3"));
        assert_eq!(spell(ScalarStyle::DoubleQuoted, K::Bool, "true").as_deref(), Some("true"));
        assert_eq!(spell(ScalarStyle::Plain, K::Int, "abc"), None);
        assert_eq!(spell(ScalarStyle::Plain, K::Null, ""), None);
        assert_eq!(spell(ScalarStyle::Plain, K::Null, "~").as_deref(), Some("~"));
    }

    #[test]
    fn json_spelling_quotes_text_and_takes_only_json_literals() {
        let spell = |style, kind, text| Spelling { syntax: Syntax::Json, style, kind, flow: true }.write(text);
        assert_eq!(spell(ScalarStyle::Plain, K::String, "x, y").as_deref(), Some("\"x, y\""));
        assert_eq!(spell(ScalarStyle::DoubleQuoted, K::String, "a\"b").as_deref(), Some("\"a\\\"b\""));
        assert_eq!(spell(ScalarStyle::Plain, K::Int, "-3").as_deref(), Some("-3"));
        assert_eq!(spell(ScalarStyle::Plain, K::Float, "1e3").as_deref(), Some("1e3"));
        assert_eq!(spell(ScalarStyle::Plain, K::Null, "null").as_deref(), Some("null"));
        assert_eq!(spell(ScalarStyle::Plain, K::Bool, "false").as_deref(), Some("false"));
        for (kind, text) in [(K::Int, "0x1F"), (K::Null, "~"), (K::Bool, "True"), (K::Float, "1."), (K::Int, "1.5")] {
            assert_eq!(spell(ScalarStyle::Plain, kind, text), None, "{text:?}");
        }
    }

    #[test]
    fn json_reads_anything_but_a_json_literal_as_text() {
        assert_eq!(resolve_in(Syntax::Json, "~"), K::String);
        assert_eq!(resolve_in(Syntax::Json, "0x1F"), K::String);
        assert_eq!(resolve_in(Syntax::Json, "12"), K::Int);
        assert_eq!(resolve_in(Syntax::Json, "null"), K::Null);
        assert_eq!(resolve_in(Syntax::Yaml, "~"), K::Null);
    }

    #[test]
    fn kinds_have_names_for_messages() {
        let names = [K::Null, K::Bool, K::Int, K::Float, K::String].map(K::name);
        assert_eq!(names, ["null", "a boolean", "an integer", "a float", "text"]);
    }
}
