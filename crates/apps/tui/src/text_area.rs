//! A file's text being edited in a tab: its lines and a cursor that keys type at and move.
//! While a tab edits text, the shell hands it every key but `ctrl-c`
//! ([`crate::screen::Screen::typing`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The rows `pgup` and `pgdn` move the cursor by.
const PAGE: usize = 10;
/// What `tab` types: YAML is indented with spaces.
const INDENT: &str = "  ";

/// The text, a line at a time, and where the cursor is: a line and a character within it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextArea {
    lines: Vec<String>,
    row: usize,
    column: usize,
}

impl TextArea {
    /// `text` to be edited, the cursor at its start. Its line ends are kept as they are.
    pub fn new(text: &str) -> Self {
        Self { lines: text.split('\n').map(str::to_owned).collect(), row: 0, column: 0 }
    }

    /// The whole text, as it would be written.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The lines.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The cursor: its line, and the characters before it on that line.
    pub const fn cursor(&self) -> (usize, usize) {
        (self.row, self.column)
    }

    /// Applies `key`: a character is typed at the cursor, `enter` breaks the line, `backspace`
    /// and `delete` remove a character or join two lines, and the arrows, `home`, `end`, `pgup`
    /// and `pgdn` move the cursor. A character typed with `ctrl` or `alt` is ignored, but not one
    /// typed with both, which is how Windows reports `AltGr`.
    pub fn press(&mut self, key: KeyEvent) {
        let held = key.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        let plain = held.is_empty() || held == KeyModifiers::CONTROL | KeyModifiers::ALT;
        match key.code {
            KeyCode::Char(typed) if plain => self.insert(&typed.to_string()),
            KeyCode::Tab => self.insert(INDENT),
            KeyCode::Enter => self.split(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            code => self.move_by(code),
        }
    }

    /// Moves the cursor for a cursor key, keeping it on the text.
    fn move_by(&mut self, code: KeyCode) {
        let last = self.lines.len().saturating_sub(1);
        match code {
            KeyCode::Left if self.column == 0 && self.row > 0 => (self.row, self.column) = (self.row - 1, usize::MAX),
            KeyCode::Left => self.column = self.column.saturating_sub(1),
            KeyCode::Right if self.column >= self.width() && self.row < last => {
                (self.row, self.column) = (self.row + 1, 0)
            }
            KeyCode::Right => self.column += 1,
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => self.row = (self.row + 1).min(last),
            KeyCode::PageUp => self.row = self.row.saturating_sub(PAGE),
            KeyCode::PageDown => self.row = (self.row + PAGE).min(last),
            KeyCode::Home => self.column = 0,
            KeyCode::End => self.column = usize::MAX,
            _ => {}
        }
        self.column = self.column.min(self.width());
    }

    /// The characters on the cursor's line.
    fn width(&self) -> usize {
        self.lines.get(self.row).map_or(0, |line| line.chars().count())
    }

    /// Where the cursor is in its line's bytes.
    fn at(&self) -> usize {
        let line = self.lines.get(self.row).map_or("", String::as_str);
        line.char_indices().nth(self.column).map_or(line.len(), |(at, _)| at)
    }

    /// Types `text`, which holds no line break, at the cursor.
    fn insert(&mut self, text: &str) {
        let at = self.at();
        if let Some(line) = self.lines.get_mut(self.row) {
            line.insert_str(at, text);
            self.column += text.chars().count();
        }
    }

    /// Breaks the line at the cursor, which moves to the start of the new line.
    fn split(&mut self) {
        let at = self.at();
        let Some(line) = self.lines.get_mut(self.row) else { return };
        let rest = line.split_off(at);
        self.lines.insert(self.row + 1, rest);
        (self.row, self.column) = (self.row + 1, 0);
    }

    /// Removes the character before the cursor, or joins its line to the one before.
    fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
            self.delete();
        } else if self.row > 0 {
            self.row -= 1;
            self.column = self.width();
            self.delete();
        }
    }

    /// Removes the character at the cursor, or joins the next line to its line.
    fn delete(&mut self) {
        let at = self.at();
        if self.column < self.width() {
            if let Some(line) = self.lines.get_mut(self.row) {
                line.remove(at);
            }
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            if let Some(line) = self.lines.get_mut(self.row) {
                line.push_str(&next);
            }
        }
    }
}

#[cfg(test)]
#[path = "text_area_tests.rs"]
mod tests;
