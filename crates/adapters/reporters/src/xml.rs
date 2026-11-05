//! Just enough XML for `JUnit` reports: escaped text and elements.

use std::fmt::Write as _;

/// `text` safe inside an element or a quoted attribute: markup characters escaped, and
/// characters XML 1.0 cannot hold (most control characters) replaced with U+FFFD.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if allowed(c) => out.push(c),
            _ => out.push(char::REPLACEMENT_CHARACTER),
        }
    }
    out
}

/// Whether XML 1.0 allows `c` in a document.
const fn allowed(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// `<tag a="1" b="2">body</tag>`, or `<tag a="1"/>` without a body; attribute values and the
/// body are escaped here.
pub fn element(tag: &str, attrs: &[(&str, String)], body: Option<&str>) -> String {
    let head = start(tag, attrs);
    body.map_or_else(|| format!("{head}/>"), |body| format!("{head}>{}</{tag}>", escape(body)))
}

/// `<tag a="1" b="2">`, for an element whose children follow.
pub fn open(tag: &str, attrs: &[(&str, String)]) -> String {
    format!("{}>", start(tag, attrs))
}

/// `<tag a="1" b="2"`, unclosed.
fn start(tag: &str, attrs: &[(&str, String)]) -> String {
    let mut out = format!("<{tag}");
    for (name, value) in attrs {
        let _ = write!(out, " {name}=\"{}\"", escape(value));
    }
    out
}

/// Milliseconds as seconds, the way `JUnit` writes times: `0.150`.
pub fn seconds(millis: u64) -> String {
    format!("{}.{:03}", millis / 1_000, millis % 1_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_and_replaces_characters_xml_cannot_hold() {
        assert_eq!(escape(r#"a & b < c > d "e" 'f'"#), "a &amp; b &lt; c &gt; d &quot;e&quot; &apos;f&apos;");
        assert_eq!(escape("tab\tline\ncr\r"), "tab\tline\ncr\r");
        assert_eq!(escape("bell\u{7} esc\u{1b} nul\u{0} \u{FFFE}"), "bell\u{FFFD} esc\u{FFFD} nul\u{FFFD} \u{FFFD}");
        assert_eq!(
            escape("é \u{D7FF} \u{E000} \u{FFFD} \u{10000} \u{1F600}"),
            "é \u{D7FF} \u{E000} \u{FFFD} \u{10000} \u{1F600}"
        );
        assert_eq!(escape(" \u{1f}"), " \u{FFFD}");
    }

    #[test]
    fn writes_elements_with_escaped_attributes_and_bodies() {
        let attrs = [("name", "a<b".to_owned()), ("type", "V5101".to_owned())];
        assert_eq!(
            element("failure", &attrs, Some("x & y")),
            r#"<failure name="a&lt;b" type="V5101">x &amp; y</failure>"#
        );
        assert_eq!(
            element("skipped", &[("message", "\"later\"".to_owned())], None),
            r#"<skipped message="&quot;later&quot;"/>"#
        );
        assert_eq!(element("br", &[], None), "<br/>");
        assert_eq!(element("p", &[], Some("")), "<p></p>");
        assert_eq!(open("testsuite", &[("name", "a&b".to_owned())]), r#"<testsuite name="a&amp;b">"#);
    }

    #[test]
    fn writes_seconds_to_the_millisecond() {
        assert_eq!(seconds(150), "0.150");
        assert_eq!(seconds(2_500), "2.500");
        assert_eq!(seconds(0), "0.000");
        assert_eq!(seconds(61_007), "61.007");
    }
}
