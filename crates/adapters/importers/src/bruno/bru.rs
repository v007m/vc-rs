//! A `.bru` file of a Bruno collection folder, read as Bruno writes it: named blocks, each
//! opened by `name {` (or `name [` for a list) at the start of a line and closed by `}` (or `]`)
//! alone at the start of a line. A dictionary block holds `key: value` lines, `~key` when the
//! entry is off; a text block (a body, a script, tests) holds text indented by two spaces; a list
//! block holds comma-separated names. No type here derives `Debug`, so a value the file holds
//! cannot reach a printout through one.

use super::collection::Pair;

/// A `.bru` file's blocks, in order, each with its lines outdented by the two spaces Bruno
/// indents them by.
pub struct Bru {
    blocks: Vec<(String, Vec<String>)>,
}

/// What a line outside any block is.
enum Opener {
    /// A block opens; its lines run to the line that is the closer alone.
    Block(String, &'static str),
    /// A block opens and closes on the line (`vars {}`).
    Empty(String),
}

impl Bru {
    /// The blocks of `text`. Lines outside any block are passed by.
    ///
    /// # Errors
    ///
    /// The block that is not closed; the reason never quotes the file's values.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut blocks = Vec::new();
        let mut open: Option<(String, &str, Vec<String>)> = None;
        for line in text.lines() {
            match open.take() {
                Some((name, closer, lines)) if line.trim_end() == closer => blocks.push((name, lines)),
                Some((name, closer, mut lines)) => {
                    lines.push(line.strip_prefix("  ").unwrap_or(line).to_owned());
                    open = Some((name, closer, lines));
                }
                None => match opener(line) {
                    Some(Opener::Block(name, closer)) => open = Some((name, closer, Vec::new())),
                    Some(Opener::Empty(name)) => blocks.push((name, Vec::new())),
                    None => {}
                },
            }
        }
        match open {
            Some((name, closer, _)) => Err(format!("its {name} block has no closing {closer}")),
            None => Ok(Self { blocks }),
        }
    }

    /// Whether the file has a block named `name`.
    pub fn has(&self, name: &str) -> bool {
        self.lines(name).is_some()
    }

    /// The text of the block `name`, its lines joined by newlines.
    pub fn text(&self, name: &str) -> Option<String> {
        self.lines(name).map(|lines| lines.join("\n"))
    }

    /// The entries of the dictionary block `name`, on or off; none when there is no such block.
    pub fn pairs(&self, name: &str) -> Vec<Pair> {
        self.lines(name).unwrap_or_default().iter().filter_map(|line| pair(line)).collect()
    }

    /// The value of the entry `key` of the dictionary block `name`, when it is on.
    pub fn value(&self, name: &str, key: &str) -> Option<String> {
        self.pairs(name).into_iter().find(|pair| pair.enabled && pair.name == key).map(|pair| pair.value)
    }

    /// The names of the list block `name`, each on or off (`~name`), with no value.
    pub fn list(&self, name: &str) -> Vec<Pair> {
        let text = self.lines(name).unwrap_or_default().join(",");
        let names = text.split(',').map(str::trim).filter(|name| !name.is_empty());
        names.map(|name| switched(name, String::new())).collect()
    }

    fn lines(&self, name: &str) -> Option<&[String]> {
        self.blocks.iter().find(|(block, _)| block == name).map(|(_, lines)| lines.as_slice())
    }
}

/// The block a line outside any block opens: one that starts at the start of the line and ends
/// with `{` or `[`.
fn opener(line: &str) -> Option<Opener> {
    let line = line.trim_end();
    if line.starts_with(char::is_whitespace) {
        return None;
    }
    let named = |name: &str| Some(name.trim().to_owned()).filter(|name| !name.is_empty());
    if let Some(name) = line.strip_suffix("{}").or_else(|| line.strip_suffix("[]")) {
        return named(name).map(Opener::Empty);
    }
    let (name, closer) =
        line.strip_suffix('{').map(|name| (name, "}")).or_else(|| Some((line.strip_suffix('[')?, "]")))?;
    named(name).map(|name| Opener::Block(name, closer))
}

/// The entry a dictionary line holds: the key before the first `:` and the value after it.
fn pair(line: &str) -> Option<Pair> {
    let (key, value) = line.trim().split_once(':')?;
    Some(switched(key.trim(), value.trim().to_owned())).filter(|pair| !pair.name.is_empty())
}

/// An entry named `name`, off when the name starts with `~`.
fn switched(name: &str, value: String) -> Pair {
    let (name, enabled) = name.strip_prefix('~').map_or((name, true), |name| (name, false));
    Pair { name: name.trim().to_owned(), value, enabled }
}

#[cfg(test)]
#[path = "bru_tests.rs"]
mod tests;
