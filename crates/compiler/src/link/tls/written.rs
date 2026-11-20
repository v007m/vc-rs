//! The text fallback for a config that does not parse (D353): with no tree to find the clients'
//! keys and passphrases in, it masks every value written under a `key` or `passphrase` key, so
//! the syntax error's excerpt prints none of them.

use vc_model::{Pos, SourceFileId, SourceSpan};

/// The keys whose values the fallback masks.
const SECRET_KEYS: [&str; 4] = ["key", "passphrase", "secretAccessKey", "sessionToken"];

/// One line of the text, without its line break.
struct Line<'t> {
    number: u32,
    offset: usize,
    text: &'t str,
}

impl Line<'_> {
    /// The width of its leading spaces.
    fn indent(&self) -> usize {
        self.text.len() - self.text.trim_start_matches(' ').len()
    }

    /// The position `at` bytes into it.
    fn pos(&self, at: usize) -> Pos {
        let column = self.text.get(..at).map_or(0, |before| before.chars().count());
        Pos::new(self.number, u32::try_from(column + 1).unwrap_or(u32::MAX), self.offset + at)
    }

    /// The position after its last character.
    fn end(&self) -> Pos {
        self.pos(self.text.len())
    }
}

/// Where `text`, the config `file`, may write a client key or passphrase: on each line whose
/// key is `key` or `passphrase` (after any `- `), the value to the line's end, then each line
/// after it that is blank, indented deeper than that key, or no `key:` at all, as a block
/// scalar's lines and a PEM block that lost its indent are. A line with a `:` at the key's
/// indent or less ends it. References are masked too: without a tree they cannot be told apart.
pub fn spans(file: SourceFileId, text: &str) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    let mut open: Option<(usize, SourceSpan)> = None;
    for line in lines(text) {
        if let Some((indent, span)) = open.as_mut() {
            if continues(&line, *indent) {
                span.end = line.end();
                continue;
            }
            spans.extend(open.take().map(|(_, span)| span));
        }
        open = secret_value(&line).map(|(indent, start)| (indent, SourceSpan::new(file, start, line.end())));
    }
    spans.extend(open.map(|(_, span)| span));
    spans
}

/// Whether `line` still belongs to the value of a key indented `indent`.
fn continues(line: &Line<'_>, indent: usize) -> bool {
    line.text.trim().is_empty() || line.indent() > indent || !line.text.contains(':')
}

/// The indent of `line`'s key and where its value starts, when the key is a secret one.
fn secret_value(line: &Line<'_>) -> Option<(usize, Pos)> {
    let rest = &line.text[line.indent()..];
    let keyed = rest.strip_prefix("- ").map_or(rest, |item| item.trim_start_matches(' '));
    let (key, value) = keyed.split_once(':')?;
    let key = key.trim_end().trim_matches(['"', '\'']);
    if !SECRET_KEYS.contains(&key) {
        return None;
    }
    let key_at = line.text.len() - keyed.len();
    Some((key_at, line.pos(line.text.len() - value.trim_start().len())))
}

/// The lines of `text`, numbered from 1, each with the offset it starts at.
fn lines(text: &str) -> impl Iterator<Item = Line<'_>> {
    let raw = text.split_inclusive('\n').scan(0, |offset: &mut usize, raw: &str| {
        let start = *offset;
        *offset += raw.len();
        Some((start, raw.trim_end_matches(['\n', '\r'])))
    });
    (1..).zip(raw).map(|(number, (offset, text))| Line { number, offset, text })
}

#[cfg(test)]
mod tests {
    use vc_model::{SourceFileId, SourceSpan};

    use super::spans;

    /// The text each span covers, start to end.
    fn covered(text: &str) -> Vec<&str> {
        let found: Vec<SourceSpan> = spans(SourceFileId::new(0), text);
        found.iter().map(|span| &text[span.byte_range()]).collect()
    }

    #[test]
    fn a_one_line_value_is_masked_to_the_line_end() {
        let text = "clients:\n  - id: bundle\n    passphrase: hunter2: oops\n    pkcs12: b.p12\n";
        assert_eq!(covered(text), ["hunter2: oops"]);
        let at = spans(SourceFileId::new(0), text);
        assert_eq!((at[0].start.line, at[0].start.column, at[0].end.column), (3, 17, 30));
        assert_eq!(covered("  - key: k1\n  - 'key' : k2\n  - \"passphrase\":p3\n"), ["k1", "k2", "p3"]);
        assert!(covered("keyFile: a\napiKey: b\nid: key\n").is_empty(), "only the secret keys");
        assert_eq!(covered("    secretAccessKey: s1\n    sessionToken: t1\n    accessKeyId: a1\n"), ["s1", "t1"]);
    }

    #[test]
    fn a_block_runs_to_the_next_key_at_its_indent_or_less() {
        let text = "  - key: |\n      -----BEGIN PRIVATE KEY-----\n\n      MIIB\n    MIIC\n  AAAA\n    -----END\n    hosts: [a]\n";
        assert_eq!(
            covered(text),
            ["|\n      -----BEGIN PRIVATE KEY-----\n\n      MIIB\n    MIIC\n  AAAA\n    -----END"]
        );
        let deeper = "key: |\n  a: b\nnext: 1\n";
        assert_eq!(covered(deeper), ["|\n  a: b"], "a deeper line with a `:` is still the value");
        assert_eq!(covered("key: |\n  MIIB"), ["|\n  MIIB"], "a value at the end of the text");
    }

    #[test]
    fn positions_count_characters_and_bytes() {
        let text = "é: 1\r\npassphrase: ñ\r\n";
        let at = spans(SourceFileId::new(0), text);
        assert_eq!(covered(text), ["ñ"]);
        assert_eq!((at[0].start.line, at[0].start.column, at[0].start.offset), (2, 13, 19));
        assert_eq!((at[0].end.column, at[0].end.offset), (14, 21));
    }
}
