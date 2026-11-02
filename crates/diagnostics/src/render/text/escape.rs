use std::borrow::Cow;

/// Stands in for characters with no control picture of their own: the picture for SUB, the
/// control that marks a substituted character.
const SUBSTITUTE: char = '\u{241a}';

/// `text` with every character a terminal could act on replaced by one visible character.
///
/// Each becomes a Unicode control picture, which is one column wide in every terminal: C0
/// controls (except tab) their own (`ESC` → `␛`, `CR` → `␍`), `DEL` `␡`, and C1 controls and
/// the bidi controls (U+202A–U+202E, U+2066–U+2069, U+200E, U+200F, U+061C) the substitute
/// picture `␚`. One character in, one out, so columns still line up with the source.
pub(super) fn escape(text: &str) -> Cow<'_, str> {
    if text.chars().any(is_unsafe) { Cow::Owned(text.chars().map(visible).collect()) } else { Cow::Borrowed(text) }
}

const fn is_unsafe(c: char) -> bool {
    (c.is_control() && c != '\t')
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{61c}')
}

fn visible(c: char) -> char {
    if !is_unsafe(c) {
        return c;
    }
    match u32::from(c) {
        code @ 0..=0x1f => char::from_u32(0x2400 + code).unwrap_or(SUBSTITUTE),
        0x7f => '\u{2421}',
        _ => SUBSTITUTE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(escape("call: orders#place\tok"), Cow::Borrowed(_)));
    }

    #[test]
    fn controls_and_bidi_marks_become_control_pictures() {
        let text = "a\x1b[2Jb\rc\x07d\x7fe\u{9b}f\u{202e}g\u{2066}h\u{200e}i\u{200f}j\u{61c}k";
        assert_eq!(escape(text), "a␛[2Jb␍c␇d␡e␚f␚g␚h␚i␚j␚k");
    }
}
