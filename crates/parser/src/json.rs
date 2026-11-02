//! The JSON reader (D148): strict RFC 8259, by hand, into the same tree as YAML. Strings are
//! [`ScalarStyle::DoubleQuoted`] text; numbers, `true`, `false` and `null` are plain scalars,
//! which resolve to the same kinds as in YAML.

use vc_diagnostics::Diagnostics;
use vc_model::SourceFileId;

use crate::build::{Builder, Flow, Shape};
use crate::scan::{Cursor, Fault, shown};
use crate::{Node, Scalar, ScalarStyle, json_string, problems};

/// The JSON words.
const LITERALS: [&str; 3] = ["true", "false", "null"];

/// What the reader wanted to find next, and how to put it right.
struct Expect {
    what: &'static str,
    help: &'static str,
}

const VALUE: Expect = Expect {
    what: "a value",
    help: "a value is an object `{…}`, a list `[…]`, text in double quotes, a number, `true`, `false` or `null`",
};
const KEY: Expect =
    Expect { what: "a key in double quotes", help: r#"write keys as text in double quotes: `"name": 1`"# };
const COLON: Expect = Expect { what: "`:`", help: "separate a key from its value with `:`" };
const END: Expect =
    Expect { what: "the end of the file", help: "a JSON file holds one value; put several values in a list `[…]`" };

/// An object or a list: how it closes and what may follow each of its items.
struct Brackets {
    shape: Shape,
    close: char,
    next: Expect,
}

const OBJECT: Brackets = Brackets {
    shape: Shape::Map,
    close: '}',
    next: Expect { what: "`,` or `}`", help: "separate entries with `,` and close the object with `}`" },
};
const LIST: Brackets = Brackets {
    shape: Shape::Seq,
    close: ']',
    next: Expect { what: "`,` or `]`", help: "separate items with `,` and close the list with `]`" },
};

/// The reader stopped; the diagnostic is already recorded.
struct Halt;

type Step = Result<(), Halt>;

/// Reads `text` as one JSON value. Blank text is a null root, as an empty YAML file is.
pub fn read(file: SourceFileId, text: &str) -> (Option<Node>, Diagnostics) {
    let mut reader = Reader { cursor: Cursor::new(file, text), builder: Builder::new(file) };
    let _ = reader.document();
    reader.builder.finish()
}

/// Whether `text` is exactly one JSON number, `true`, `false` or `null`.
pub fn is_literal(text: &str) -> bool {
    LITERALS.contains(&text) || number_len(text.as_bytes()) == Ok(text.len())
}

/// The length of the JSON number at the start of `bytes`, or the index of the first byte that
/// breaks it.
fn number_len(bytes: &[u8]) -> Result<usize, usize> {
    let mut at = usize::from(bytes.first() == Some(&b'-'));
    at = if bytes.get(at) == Some(&b'0') { at + 1 } else { digits(bytes, at)? };
    if bytes.get(at) == Some(&b'.') {
        at = digits(bytes, at + 1)?;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(at + 1), Some(b'+' | b'-')));
        at = digits(bytes, at + 1 + sign)?;
    }
    if bytes.get(at).is_some_and(u8::is_ascii_digit) { Err(at) } else { Ok(at) }
}

/// The index after the digits starting at `from`, or `from` when there are none.
fn digits(bytes: &[u8], from: usize) -> Result<usize, usize> {
    let count = bytes.iter().skip(from).take_while(|b| b.is_ascii_digit()).count();
    if count == 0 { Err(from) } else { Ok(from + count) }
}

struct Reader<'t> {
    cursor: Cursor<'t>,
    builder: Builder,
}

impl Reader<'_> {
    fn document(&mut self) -> Step {
        self.cursor.skip_blanks();
        if self.cursor.peek().is_none() {
            return Ok(());
        }
        self.value()?;
        self.cursor.skip_blanks();
        if self.cursor.peek().is_some() {
            return Err(self.unexpected(&END));
        }
        Ok(())
    }

    fn value(&mut self) -> Step {
        self.cursor.skip_blanks();
        match self.cursor.peek() {
            Some('{') => self.collection(&OBJECT),
            Some('[') => self.collection(&LIST),
            Some('"') => self.string(),
            Some('-' | '0'..='9') => self.number(),
            Some(c) if c.is_ascii_alphabetic() => self.word(),
            _ => Err(self.unexpected(&VALUE)),
        }
    }

    fn collection(&mut self, brackets: &Brackets) -> Step {
        let open = self.cursor.here();
        self.cursor.bump();
        if self.builder.open(brackets.shape, open) == Flow::Stop {
            return Err(Halt);
        }
        self.items(brackets)?;
        let close = self.cursor.here();
        self.cursor.bump();
        self.builder.close(close);
        Ok(())
    }

    /// Reads the items up to the closing bracket, and leaves the cursor on it.
    fn items(&mut self, brackets: &Brackets) -> Step {
        self.cursor.skip_blanks();
        if self.cursor.peek() == Some(brackets.close) {
            return Ok(());
        }
        loop {
            self.item(brackets.shape)?;
            self.cursor.skip_blanks();
            let comma = self.cursor.here();
            if !self.cursor.eat(',') {
                break;
            }
            self.cursor.skip_blanks();
            if self.cursor.peek() == Some(brackets.close) {
                let message = format!("a comma before the closing `{}`", brackets.close);
                return Err(self.halt(&Fault { at: comma, message, help: "remove the comma after the last item" }));
            }
        }
        if self.cursor.peek() == Some(brackets.close) { Ok(()) } else { Err(self.unexpected(&brackets.next)) }
    }

    fn item(&mut self, shape: Shape) -> Step {
        match shape {
            Shape::Seq => self.value(),
            Shape::Map => self.entry(),
        }
    }

    fn entry(&mut self) -> Step {
        if self.cursor.peek() != Some('"') {
            return Err(self.unexpected(&KEY));
        }
        self.string()?;
        self.cursor.skip_blanks();
        if !self.cursor.eat(':') {
            return Err(self.unexpected(&COLON));
        }
        self.value()
    }

    fn string(&mut self) -> Step {
        let (text, at) = json_string::read(&mut self.cursor).map_err(|fault| self.halt(&fault))?;
        self.builder.scalar(Scalar::new(text, ScalarStyle::DoubleQuoted), at);
        Ok(())
    }

    fn number(&mut self) -> Step {
        let start = self.cursor.pos();
        let rest = self.cursor.rest();
        match number_len(rest.as_bytes()) {
            Ok(len) => {
                self.cursor.skip_ascii(len);
                let text = rest.get(..len).unwrap_or_default();
                self.builder.scalar(Scalar::new(text, ScalarStyle::Plain), self.cursor.span_from(start));
                Ok(())
            }
            Err(bad) => {
                self.cursor.skip_ascii(bad);
                let at = self.cursor.span(start, self.cursor.here().end);
                let help = "write numbers as JSON does: `12`, `-0.5`, `1e3`, with no leading zeros or `+`";
                Err(self.halt(&Fault { at, message: "not a valid number".to_owned(), help }))
            }
        }
    }

    /// A word: `true`, `false` or `null`, or a mistake such as `True` or unquoted text.
    fn word(&mut self) -> Step {
        let start = self.cursor.pos();
        let rest = self.cursor.rest();
        let len = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
        let word = rest.get(..len).unwrap_or_default();
        self.cursor.skip_ascii(len);
        let at = self.cursor.span_from(start);
        if LITERALS.contains(&word) {
            self.builder.scalar(Scalar::new(word, ScalarStyle::Plain), at);
            return Ok(());
        }
        let help = "put text in double quotes, and write `true`, `false` and `null` in lowercase";
        Err(self.halt(&Fault { at, message: format!("`{word}` is not a JSON value"), help }))
    }

    /// Stops on the character at the cursor, which is not what `expect` wanted.
    fn unexpected(&mut self, expect: &Expect) -> Halt {
        let message = self.cursor.peek().map_or_else(
            || format!("the file ends before {}", expect.what),
            |c| format!("expected {}, found `{}`", expect.what, shown(&c.to_string())),
        );
        self.halt(&Fault { at: self.cursor.here(), message, help: expect.help })
    }

    fn halt(&mut self, fault: &Fault) -> Halt {
        self.builder.fail(problems::json_syntax(fault.at, &fault.message, fault.help));
        Halt
    }
}

#[cfg(test)]
mod tests {
    use super::{is_literal, number_len};

    #[test]
    fn numbers_follow_the_json_grammar() {
        type Case = (&'static str, Result<usize, usize>);
        let cases: [Case; 14] = [
            ("0", Ok(1)),
            ("-0", Ok(2)),
            ("12,", Ok(2)),
            ("1.5]", Ok(3)),
            ("-2.5e-3", Ok(7)),
            ("1E+9", Ok(4)),
            ("01", Err(1)),
            ("-01", Err(2)),
            ("-", Err(1)),
            ("1.", Err(2)),
            ("1.e3", Err(2)),
            ("1e", Err(2)),
            ("1e+", Err(3)),
            ("-x", Err(1)),
        ];
        for (text, expected) in cases {
            assert_eq!(number_len(text.as_bytes()), expected, "{text:?}");
        }
    }

    #[test]
    fn literals_are_json_numbers_and_words_only() {
        for text in ["null", "true", "false", "0", "-1.5e3"] {
            assert!(is_literal(text), "{text:?}");
        }
        for text in ["~", "True", "0x1F", "+1", ".5", "1.", "1 ", "", "nul"] {
            assert!(!is_literal(text), "{text:?}");
        }
    }
}
