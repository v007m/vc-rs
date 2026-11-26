//! Where the YAML editor's cursor is (D681): the key path to the map it is in, what is being
//! written there, and the keys the map already has. It reads the text as it is being typed,
//! which need not parse, so it goes by indentation: block style only, flow maps and lists
//! (`{a: 1}`, `[a]`) are left out.

/// Where the cursor is in a YAML file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Cursor {
    /// The keys from the root to the map the cursor is in; `-` for a list item.
    pub path: Vec<String>,
    /// What is being written there.
    pub place: Place,
    /// The text from the start of what is being written to the cursor.
    pub word: String,
    /// The key written on the cursor's line, whole, when there is one.
    pub key: Option<String>,
    /// The value written after that key, whole and trimmed, quotes dropped.
    pub value: String,
    /// The other keys written in the same map.
    pub siblings: Vec<String>,
}

/// What the cursor is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Place {
    /// A key, or the start of a line.
    #[default]
    Key,
    /// A key's value, after its `:`.
    Value,
    /// A template's reference, after an unclosed `${`.
    Template,
}

/// One line read by indentation: the columns of its list dashes and its key, if any.
struct Line<'a> {
    /// The column of its first character.
    start: usize,
    /// The columns of its `- ` dashes, left to right.
    dashes: Vec<usize>,
    /// The column after the dashes.
    at: usize,
    /// The key written there.
    key: Option<&'a str>,
}

/// The cursor at byte `offset` of `text`; an offset inside a character counts as its start.
#[must_use]
pub fn cursor(text: &str, offset: usize) -> Cursor {
    let offset = (0..=offset.min(text.len())).rev().find(|at| text.is_char_boundary(*at)).unwrap_or(0);
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |at| offset + at);
    let line = read(&text[start..end]).unwrap_or(Line {
        start: offset - start,
        dashes: Vec::new(),
        at: offset - start,
        key: None,
    });
    let mut cursor = on_line(&text[start..end], offset - start, &line);
    let mut walk =
        Walk { col: line.at, own: line.dashes.is_empty(), list: false, path: Vec::new(), siblings: Vec::new() };
    walk.dashes(&line.dashes);
    text[..start].lines().rev().filter_map(read).for_each(|above| walk.up(&above));
    walk.siblings.extend(below(&text[end..], line.at));
    walk.path.reverse();
    (cursor.path, cursor.siblings) = (walk.path, walk.siblings);
    cursor
}

/// The place, word, key and value of the cursor at `column` of `text`, read as `line`.
fn on_line(text: &str, column: usize, line: &Line<'_>) -> Cursor {
    let before = &text[..column];
    let key = line.key.map(str::to_owned);
    let after_key = line.key.map(|key| line.at + key.len() + 1);
    let value = after_key.and_then(|at| text.get(at..)).map_or("", |value| value.trim().trim_matches(['"', '\'']));
    let template = before.rfind("${").filter(|at| !before[*at..].contains('}'));
    let (place, word) = template.map_or_else(
        || {
            after_key.filter(|at| column >= *at).map_or_else(
                || (Place::Key, before.get(line.at..).unwrap_or_default()),
                |at| (Place::Value, unquoted(&before[at..])),
            )
        },
        |at| (Place::Template, &before[at + 2..]),
    );
    Cursor { place, word: word.to_owned(), key, value: value.to_owned(), ..Cursor::default() }
}

/// A value as typed so far, its opening quote dropped.
fn unquoted(value: &str) -> &str {
    value.trim_start().trim_start_matches(['"', '\''])
}

/// The keys at `col` on the lines after the cursor's, up to the first line that starts left of it.
fn below(text: &str, col: usize) -> Vec<String> {
    let lines = text.lines().filter_map(read).take_while(|line| line.start >= col);
    lines
        .filter(|line| line.at == col && line.dashes.is_empty())
        .filter_map(|line| line.key.map(str::to_owned))
        .collect()
}

/// `text` read by indentation; `None` for a blank line or a comment.
fn read(text: &str) -> Option<Line<'_>> {
    let start = text.len() - text.trim_start_matches(' ').len();
    let (mut at, mut dashes) = (start, Vec::new());
    while let Some(rest) = text.get(at..).filter(|rest| rest.trim_end() == "-" || rest.starts_with("- ")) {
        dashes.push(at);
        at += 1 + rest[1..].len() - rest[1..].trim_start_matches(' ').len();
    }
    let rest = text[at..].trim_end();
    if (rest.is_empty() && dashes.is_empty()) || rest.starts_with('#') {
        return None;
    }
    Some(Line { start, dashes, at, key: key_of(rest) })
}

/// The key `rest` starts with: the text before its first `: `, or before a final `:`.
fn key_of(rest: &str) -> Option<&str> {
    let end = rest.find(": ").or_else(|| rest.strip_suffix(':').map(str::len))?;
    Some(&rest[..end]).filter(|key| !key.is_empty() && !key.contains([' ', '{', '[', '"', '\'', '#']))
}

/// The walk up from the cursor's line to the root.
struct Walk {
    /// The column of the map or list the walk is in.
    col: usize,
    /// Whether the walk is still in the cursor's own map, whose keys are its siblings.
    own: bool,
    /// Whether the walk came to `col` by a list dash, so a key at `col` holds that list.
    list: bool,
    /// The path so far, innermost first.
    path: Vec<String>,
    /// The cursor's map's keys found so far.
    siblings: Vec<String>,
}

impl Walk {
    /// Takes in the line `line` above the walk's place.
    fn up(&mut self, line: &Line<'_>) {
        match line.key {
            Some(_) if line.at > self.col => return,
            Some(key) if line.at < self.col || (self.list && line.dashes.is_empty()) => self.enter(key, line.at),
            Some(key) if self.own => self.siblings.push(key.to_owned()),
            None if line.start >= self.col => return,
            _ => {}
        }
        self.dashes(&line.dashes);
    }

    /// Steps out to the key `key` at `col`.
    fn enter(&mut self, key: &str, col: usize) {
        self.path.push(key.to_owned());
        (self.col, self.own, self.list) = (col, false, false);
    }

    /// Steps out through each dash left of the walk's column, innermost first.
    fn dashes(&mut self, dashes: &[usize]) {
        for &dash in dashes.iter().rev() {
            if dash < self.col {
                self.path.push("-".to_owned());
                (self.col, self.own, self.list) = (dash, false, true);
            }
        }
    }
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod tests;
