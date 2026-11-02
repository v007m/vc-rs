//! JSON strings: the quotes, the escapes, and `\u` escapes with surrogate pairs (RFC 8259 §7).

use vc_model::{Pos, SourceSpan};

use crate::scan::{Cursor, Fault, shown};

/// The one-letter escapes and what they stand for.
const SIMPLE: [(char, char); 8] =
    [('"', '"'), ('\\', '\\'), ('/', '/'), ('b', '\u{8}'), ('f', '\u{c}'), ('n', '\n'), ('r', '\r'), ('t', '\t')];

const ESCAPE_HELP: &str =
    r#"JSON escapes are `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and `\u` with four hex digits"#;
const SURROGATE_HELP: &str =
    r"write the character itself, or both halves of its surrogate pair, such as `\ud83d\ude00` for 😀";

/// Reads the string at the cursor, which is on its opening `"`: the text with escapes applied,
/// and the span from quote to quote.
pub fn read(cursor: &mut Cursor<'_>) -> Result<(String, SourceSpan), Fault> {
    let start = cursor.pos();
    let quote = cursor.here();
    cursor.bump();
    let mut text = String::new();
    loop {
        match cursor.peek() {
            Some('"') => {
                cursor.bump();
                return Ok((text, cursor.span_from(start)));
            }
            Some('\\') => text.push(escape(cursor)?),
            Some(c) if c < ' ' => return Err(control(cursor.here(), c)),
            Some(c) => {
                text.push(c);
                cursor.bump();
            }
            None => return Err(unclosed(quote)),
        }
    }
}

/// Reads the escape at the cursor, which is on its `\`.
fn escape(cursor: &mut Cursor<'_>) -> Result<char, Fault> {
    let start = cursor.pos();
    cursor.bump();
    let letter = cursor.peek();
    cursor.bump();
    if letter == Some('u') {
        return unicode(cursor, start);
    }
    let simple = letter.and_then(|letter| SIMPLE.iter().find(|&&(l, _)| l == letter).map(|&(_, c)| c));
    simple.ok_or_else(|| {
        let at = cursor.span_from(start);
        let message = format!("`{}` is not a JSON escape", shown(cursor.slice(start, cursor.pos())));
        Fault { at, message, help: ESCAPE_HELP }
    })
}

/// Reads the four hex digits after `\u` and, for the first half of a surrogate pair, the
/// `\u` escape of the second half. `start` is the `\`.
fn unicode(cursor: &mut Cursor<'_>, start: Pos) -> Result<char, Fault> {
    let unit = hex4(cursor, start)?;
    let first_end = cursor.pos();
    let lone = |cursor: &Cursor<'_>| lone_surrogate(cursor, start, first_end);
    if (0xDC00..0xE000).contains(&unit) {
        return Err(lone(cursor));
    }
    if !(0xD800..0xDC00).contains(&unit) {
        return char::from_u32(unit).ok_or_else(|| lone(cursor));
    }
    if !cursor.rest().starts_with("\\u") {
        return Err(lone(cursor));
    }
    cursor.skip_ascii(2);
    let low = hex4(cursor, first_end)?;
    if !(0xDC00..0xE000).contains(&low) {
        return Err(lone(cursor));
    }
    char::from_u32(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)).ok_or_else(|| lone(cursor))
}

/// Reads four hex digits; `start` is the `\` of their escape.
fn hex4(cursor: &mut Cursor<'_>, start: Pos) -> Result<u32, Fault> {
    let rest = cursor.rest();
    let found = rest.bytes().take(4).take_while(u8::is_ascii_hexdigit).count();
    cursor.skip_ascii(found);
    let digits = rest.get(..found).filter(|_| found == 4);
    digits.and_then(|digits| u32::from_str_radix(digits, 16).ok()).ok_or_else(|| {
        let at = cursor.span(start, cursor.here().end);
        Fault { at, message: r"`\u` needs four hex digits".to_owned(), help: ESCAPE_HELP }
    })
}

fn lone_surrogate(cursor: &Cursor<'_>, start: Pos, end: Pos) -> Fault {
    let message = format!("`{}` is half of a surrogate pair", cursor.slice(start, end));
    Fault { at: cursor.span(start, end), message, help: SURROGATE_HELP }
}

fn control(at: SourceSpan, c: char) -> Fault {
    if c == '\n' {
        let help = r#"close the string with `"` before the line ends, or write the line break as `\n`"#;
        return Fault { at, message: "a line break inside a string".to_owned(), help };
    }
    let message = format!("the control character `{}` inside a string", c.escape_debug());
    Fault { at, message, help: r"write control characters as escapes, such as `\t` or `\u0001`" }
}

fn unclosed(quote: SourceSpan) -> Fault {
    let help = r#"add the closing `"`"#;
    Fault { at: quote, message: "the string is never closed".to_owned(), help }
}
