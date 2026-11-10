//! A line of text a tab takes from the keyboard, such as the reason for a skip. While a tab
//! takes text, the shell hands it every key but `ctrl-c` ([`crate::screen::Screen::typing`]).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What a key did to an [`Input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typed {
    /// The text changed, or the key meant nothing to it.
    Editing,
    /// `enter`: the text is done.
    Done,
    /// `esc`: the text is dropped.
    Dropped,
}

/// The text typed so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
}

impl Input {
    /// The text typed so far.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Applies `key`: a character is added, `backspace` takes the last one off, `enter` is
    /// done and `esc` drops the text. A character typed with `ctrl` or `alt` is ignored.
    pub fn press(&mut self, key: KeyEvent) -> Typed {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            KeyCode::Enter => return Typed::Done,
            KeyCode::Esc => return Typed::Dropped,
            KeyCode::Backspace => {
                self.text.pop();
            }
            KeyCode::Char(typed) if plain => self.text.push(typed),
            _ => {}
        }
        Typed::Editing
    }
}
