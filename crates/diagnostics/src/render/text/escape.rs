use std::borrow::Cow;

/// `text` with every character a terminal could act on replaced by one visible character.
///
/// C0 controls (except tab) become their Unicode control picture (`ESC` → `␛`, `CR` → `␍`),
/// `DEL` becomes `␡`, and C1 controls and the bidi embedding, override and isolate characters
/// (U+202A–U+202E, U+2066–U+2069) become `�`. One character in, one out, so columns still line
/// up with the source.
pub(super) fn escape(text: &str) -> Cow<'_, str> {
    if text.chars().any(is_unsafe) { Cow::Owned(text.chars().map(visible).collect()) } else { Cow::Borrowed(text) }
}

const fn is_unsafe(c: char) -> bool {
    (c.is_control() && c != '\t') || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn visible(c: char) -> char {
    if !is_unsafe(c) {
        return c;
    }
    match u32::from(c) {
        code @ 0..=0x1f => char::from_u32(0x2400 + code).unwrap_or(char::REPLACEMENT_CHARACTER),
        0x7f => '\u{2421}',
        _ => char::REPLACEMENT_CHARACTER,
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
    fn controls_and_bidi_overrides_become_visible() {
        let text = "a\x1b[2Jb\rc\x07d\x7fe\u{9b}f\u{202e}g\u{2066}h";
        assert_eq!(escape(text), "a␛[2Jb␍c␇d␡e�f�g�h");
    }
}
