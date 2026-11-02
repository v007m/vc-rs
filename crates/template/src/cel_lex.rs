//! CEL reserves the word `var`, which is one of our roots (D125). Before parsing, every `var`
//! that names the root is renamed in place to [`VAR_ALIAS`], a word of the same length, so every
//! offset into the text stays the same.

use std::ops::Range;

/// What the root `var` is called inside the CEL program.
pub const VAR_ALIAS: &str = "_v_";

/// The root as the user writes it.
const VAR: &str = "var";

/// `text` with every word `var` outside strings and comments, and not after a `.`, renamed to
/// [`VAR_ALIAS`].
///
/// # Errors
///
/// The range of a [`VAR_ALIAS`] written in the text, which the rename would make ambiguous.
pub fn hide_var(text: &str) -> Result<String, Range<usize>> {
    let mut hidden = text.to_owned();
    for range in words(text) {
        let word = text.get(range.clone()).unwrap_or_default();
        if word == VAR_ALIAS {
            return Err(range);
        }
        let selected = text.get(..range.start).unwrap_or_default().trim_end().ends_with('.');
        if word == VAR && !selected {
            hidden.replace_range(range, VAR_ALIAS);
        }
    }
    Ok(hidden)
}

/// The identifiers and keywords of `text`: words outside string literals and `//` comments.
fn words(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at = match byte {
            b'/' if bytes.get(at + 1) == Some(&b'/') => line_end(bytes, at),
            b'\'' | b'"' => string_end(bytes, at, false),
            b'0'..=b'9' => word_end(bytes, at),
            _ if is_word_start(byte) => {
                let end = word_end(bytes, at);
                let word = || {
                    found.push(at..end);
                    end
                };
                string_prefix(bytes, at..end).map_or_else(word, |raw| string_end(bytes, end, raw))
            }
            _ => at + 1,
        };
    }
    found
}

const fn is_word_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn word_end(bytes: &[u8], start: usize) -> usize {
    let len =
        bytes.get(start..).unwrap_or_default().iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'_').count();
    start + len
}

fn line_end(bytes: &[u8], start: usize) -> usize {
    bytes.get(start..).unwrap_or_default().iter().position(|b| *b == b'\n').map_or(bytes.len(), |len| start + len)
}

/// Whether the word is a string prefix (`r`, `b`, `rb`, `br` in any case) followed by a quote,
/// and if so whether the string is raw.
fn string_prefix(bytes: &[u8], word: Range<usize>) -> Option<bool> {
    let quoted = matches!(bytes.get(word.end), Some(b'\'' | b'"'));
    let prefix = bytes.get(word)?.to_ascii_lowercase();
    let known = matches!(prefix.as_slice(), b"r" | b"b" | b"rb" | b"br");
    (quoted && known).then(|| prefix.contains(&b'r'))
}

/// The offset after the string literal whose opening quote is at `open`; a backslash escapes
/// the next character unless the string is raw.
fn string_end(bytes: &[u8], open: usize, raw: bool) -> usize {
    let quote = bytes.get(open).copied().unwrap_or(b'"');
    let triple = [quote; 3];
    let width = if bytes.get(open..open + 3) == Some(&triple[..]) { 3 } else { 1 };
    let mut at = open + width;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'\\' && !raw {
            at += 2;
        } else if bytes.get(at..at + width) == Some(&triple[..width]) {
            return at + width;
        } else {
            at += 1;
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::{VAR_ALIAS, hide_var};

    #[test]
    fn renames_the_root_var_only() {
        let cases = [
            ("var.a + var.b", "_v_.a + _v_.b"),
            ("has(var.a) && x.var == variable", "has(_v_.a) && x.var == variable"),
            ("x . var", "x . var"),
            ("[var]", "[_v_]"),
            ("'var' + \"var\" + var", "'var' + \"var\" + _v_"),
            ("'''a ' var''' + var", "'''a ' var''' + _v_"),
            ("\"\"\"var\"\"\" + var", "\"\"\"var\"\"\" + _v_"),
            ("'it\\'s var' + var", "'it\\'s var' + _v_"),
            ("r'\\' + var", "r'\\' + _v_"),
            ("R'\\' + var", "R'\\' + _v_"),
            ("b'var' + rb\"var\" + Br'var' + var", "b'var' + rb\"var\" + Br'var' + _v_"),
            ("x'var' + var", "x'var' + _v_"),
            ("1var + 0x1F + var", "1var + 0x1F + _v_"),
            ("// var\nvar", "// var\n_v_"),
            ("1 / var // var", "1 / _v_ // var"),
            ("é + var", "é + _v_"),
            ("'unterminated var", "'unterminated var"),
        ];
        for (text, hidden) in cases {
            assert_eq!(hide_var(text).as_deref(), Ok(hidden), "{text}");
        }
    }

    #[test]
    fn rejects_the_alias_when_written() {
        assert_eq!(hide_var("var.a + _v_"), Err(8..11));
        assert_eq!(hide_var(&format!("'{VAR_ALIAS}'")).as_deref(), Ok("'_v_'"));
    }
}
