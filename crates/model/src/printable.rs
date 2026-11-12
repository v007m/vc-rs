//! Text fit for a terminal. A response, a message or an example row's name may hold characters
//! that move the cursor, retitle the window, reorder a line or break it; these give them back
//! replaced.

use std::borrow::Cow;

/// Shown in place of a character that is unsafe to print.
const SUBSTITUTE: char = '\u{fffd}';

/// `text` with control characters (tabs and newlines aside) and bidi marks replaced.
#[must_use]
pub fn printable(text: &str) -> Cow<'_, str> {
    replaced(text, |c| is_unsafe(c) && !matches!(c, '\t' | '\n'))
}

/// `text` for one line, such as a label: every control character, tabs and newlines too, and
/// every bidi mark replaced.
#[must_use]
pub fn one_line(text: &str) -> Cow<'_, str> {
    replaced(text, is_unsafe)
}

fn replaced(text: &str, unsafe_char: impl Fn(char) -> bool) -> Cow<'_, str> {
    if text.chars().any(&unsafe_char) {
        Cow::Owned(text.chars().map(|c| if unsafe_char(c) { SUBSTITUTE } else { c }).collect())
    } else {
        Cow::Borrowed(text)
    }
}

const fn is_unsafe(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{61c}' | '\u{2028}' | '\u{2029}')
}

#[cfg(test)]
mod tests {
    use super::{one_line, printable};

    #[test]
    fn printable_keeps_tabs_and_newlines_and_one_line_does_not() {
        assert_eq!(printable("a\tb\nc\x1b]0;x\x07"), "a\tb\nc\u{fffd}]0;x\u{fffd}");
        assert_eq!(one_line("a\tb\nc\u{202e}d"), "a\u{fffd}b\u{fffd}c\u{fffd}d");
        assert_eq!(one_line("a\u{2028}b\u{2029}c"), "a\u{fffd}b\u{fffd}c", "some terminals break lines at these");
        assert!(matches!(one_line("Oslo › 2"), std::borrow::Cow::Borrowed(_)));
    }
}
