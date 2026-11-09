//! Curl command lines as bash reads them: words split at blanks, with `'…'`, `"…"` and `$'…'`
//! quotes, backslash escapes, line continuations and `#` comments.

use std::iter::Peekable;
use std::str::Chars;

/// The characters that start a word ending the command (`| jq .`, `; echo`, `> out.json`, a
/// line break).
const OPERATORS: [char; 6] = ['|', ';', '&', '<', '>', '\n'];

/// One word of a command line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Word {
    /// The word with its quotes and escapes removed.
    pub text: String,
    /// Whether any of it was quoted or escaped, so a quoted `curl` never starts a command.
    pub quoted: bool,
}

impl Word {
    /// Whether the word ends a command: what follows it is another program's.
    fn operator(&self) -> bool {
        !self.quoted && self.text.starts_with(OPERATORS)
    }
}

/// The words of `text`.
///
/// # Errors
///
/// Why, when a quote is not closed.
pub fn words(text: &str) -> Result<Vec<Word>, String> {
    let mut lexer = Lexer { chars: text.chars().peekable(), words: Vec::new(), word: None };
    while let Some(c) = lexer.chars.next() {
        lexer.read(c)?;
    }
    lexer.end();
    Ok(lexer.words)
}

/// The arguments of each `curl` command in `words`; words before the first `curl` (a `$`
/// prompt) and after an operator are not curl's.
pub fn commands(words: &[Word]) -> Vec<&[Word]> {
    let starts = words.iter().enumerate().filter(|(_, word)| !word.quoted && word.text == "curl");
    let starts: Vec<usize> = starts.map(|(at, _)| at).collect();
    let ends = starts.iter().skip(1).copied().chain([words.len()]);
    let spans = starts.iter().zip(ends).filter_map(|(&start, end)| words.get(start + 1..end));
    spans.filter_map(|args| args.split(Word::operator).next()).collect()
}

/// The quotes a word may hold.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Quote {
    /// `'…'`: every character as it is.
    Single,
    /// `"…"`: `\` escapes only `$`, `` ` ``, `"`, `\` and a line break.
    Double,
    /// `$'…'`: `\` escapes as in C (`\n`, `\t`, `\xHH`, `\uHHHH`).
    Ansi,
}

struct Lexer<'t> {
    chars: Peekable<Chars<'t>>,
    words: Vec<Word>,
    word: Option<Word>,
}

impl Lexer<'_> {
    fn read(&mut self, c: char) -> Result<(), String> {
        match c {
            '\\' => self.escaped(),
            '\'' => self.quoted(Quote::Single)?,
            '"' => self.quoted(Quote::Double)?,
            '$' if self.chars.next_if_eq(&'\'').is_some() => self.quoted(Quote::Ansi)?,
            '#' if self.word.is_none() => while self.chars.next_if(|&c| c != '\n').is_some() {},
            '|' | ';' | '\n' => self.operator(c),
            '&' | '<' | '>' if self.word.is_none() => self.operator(c),
            c if c.is_whitespace() => self.end(),
            c => self.push(c),
        }
        Ok(())
    }

    fn push(&mut self, c: char) {
        self.word.get_or_insert_with(Word::default).text.push(c);
    }

    /// Marks the word being read as quoted, so an empty `''` is still a word.
    fn mark(&mut self) {
        self.word.get_or_insert_with(Word::default).quoted = true;
    }

    fn end(&mut self) {
        self.words.extend(self.word.take());
    }

    /// An operator, a word of its own; an `&` inside a word (an unquoted query) is not one.
    fn operator(&mut self, c: char) {
        self.end();
        self.push(c);
        while let Some(next) = self.chars.next_if(|next| OPERATORS.contains(next)) {
            self.push(next);
        }
        self.end();
    }

    /// `\` outside quotes: the next character as it is, or nothing before a line break.
    fn escaped(&mut self) {
        match self.chars.next() {
            Some('\n') | None => {}
            Some('\r') => _ = self.chars.next_if_eq(&'\n'),
            Some(c) => {
                self.push(c);
                self.mark();
            }
        }
    }

    /// The rest of a quoted part of a word, up to its closing quote.
    fn quoted(&mut self, quote: Quote) -> Result<(), String> {
        self.mark();
        let close = if quote == Quote::Double { '"' } else { '\'' };
        loop {
            match (self.chars.next(), quote) {
                (Some(c), _) if c == close => return Ok(()),
                (Some('\\'), Quote::Double) => self.in_double(),
                (Some('\\'), Quote::Ansi) => self.in_ansi(),
                (Some(c), _) => self.push(c),
                (None, _) => return Err(format!("a {close} quote is not closed")),
            }
        }
    }

    fn in_double(&mut self) {
        match self.chars.next_if(|c| matches!(c, '$' | '`' | '"' | '\\' | '\n')) {
            Some('\n') => {}
            Some(c) => self.push(c),
            None => self.push('\\'),
        }
    }

    fn in_ansi(&mut self) {
        let Some(c) = self.chars.next() else { return };
        let read = match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            'x' => self.hex(2),
            'u' => self.hex(4),
            'U' => self.hex(8),
            '\\' | '\'' | '"' | '?' => Some(c),
            _ => None,
        };
        if read.is_none() {
            self.push('\\');
        }
        self.push(read.unwrap_or(c));
    }

    /// The character up to `most` hex digits name; none when no digit follows.
    fn hex(&mut self, most: usize) -> Option<char> {
        let mut digits = String::new();
        while digits.len() < most
            && let Some(digit) = self.chars.next_if(char::is_ascii_hexdigit)
        {
            digits.push(digit);
        }
        u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32)
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
