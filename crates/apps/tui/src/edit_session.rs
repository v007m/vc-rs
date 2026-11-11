//! Editing one project file in the Explore tab (A5): it is opened (`Engine::read_file`), its
//! text edited, drafted with `ctrl-s` through the compile guard (`Engine::edit_file`), which
//! gives the change as a diff or refuses it with the error it would add, and written with `w`
//! (`Engine::write`). `esc` leaves the editor, asking first when the text was changed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use vc_engine::{Changes, SaveFile};

use crate::screen::{Ask, Update, scrolled};
use crate::text_area::TextArea;

/// Where the edit has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stage {
    /// The file is being read.
    Opening,
    /// Its text is being edited.
    Text,
    /// The text is being drafted.
    Drafting,
    /// The draft the guard passed, its diff shown.
    Drafted(Changes),
    /// The draft is being written.
    Saving(Changes),
}

/// What a key or an update did to the edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edited {
    /// The edit goes on.
    Going,
    /// The edit goes on, and asks this of the engine.
    Asks(Ask),
    /// The editor is closed: the file was written, or the edit was dropped; what to say.
    Closed(Option<String>),
    /// The file could not be opened, so the editor is closed: why.
    Dropped(String),
    /// The draft could not be written, and is kept: why.
    Failed(String),
}

/// One file being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditSession {
    path: String,
    original: String,
    digest: String,
    area: TextArea,
    stage: Stage,
    refusal: Option<String>,
    confirming: bool,
    scroll: u16,
}

impl EditSession {
    /// The edit of the file at `path`, which is being opened; and the ask that opens it.
    pub fn open(path: &str) -> (Self, Ask) {
        let session = Self {
            path: path.to_owned(),
            original: String::new(),
            digest: String::new(),
            area: TextArea::new(""),
            stage: Stage::Opening,
            refusal: None,
            confirming: false,
            scroll: 0,
        };
        (session, Ask::Open(path.to_owned()))
    }

    /// The file's path in the project.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The text and its cursor.
    pub const fn area(&self) -> &TextArea {
        &self.area
    }

    /// Where the edit has got to.
    pub const fn stage(&self) -> &Stage {
        &self.stage
    }

    /// Why the guard refused the last draft, with its help.
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// Whether `esc` asked to drop changed text and waits for `y`.
    pub const fn confirming(&self) -> bool {
        self.confirming
    }

    /// How far the draft's diff is scrolled, in lines.
    pub const fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Whether the text differs from the file as it was opened.
    pub fn dirty(&self) -> bool {
        self.area.text() != self.original
    }

    /// Applies a key.
    pub fn press(&mut self, key: KeyEvent) -> Edited {
        match &self.stage {
            Stage::Opening if key.code == KeyCode::Esc => Edited::Closed(None),
            Stage::Text => self.edit(key),
            Stage::Drafted(changes) => self.review(key, changes.clone()),
            _ => Edited::Going,
        }
    }

    /// Applies a key to the text: `ctrl-s` drafts it, `esc` leaves, asking first when the text
    /// changed, and any other key edits it.
    fn edit(&mut self, key: KeyEvent) -> Edited {
        if self.confirming {
            self.confirming = false;
            return if key.code == KeyCode::Char('y') { Edited::Closed(None) } else { Edited::Going };
        }
        match key.code {
            KeyCode::Char('s') if key.modifiers == KeyModifiers::CONTROL => self.draft(),
            KeyCode::Esc if self.dirty() => {
                self.confirming = true;
                Edited::Going
            }
            KeyCode::Esc => Edited::Closed(None),
            _ => {
                self.area.press(key);
                Edited::Going
            }
        }
    }

    /// Asks the guard for the change the text makes.
    fn draft(&mut self) -> Edited {
        let request =
            SaveFile { path: self.path.clone(), text: self.area.text(), expected_digest: Some(self.digest.clone()) };
        (self.stage, self.refusal) = (Stage::Drafting, None);
        Edited::Asks(Ask::Edit(request))
    }

    /// Applies a key to the draft shown: `w` writes it, `esc` goes back to the text, and
    /// `pgup`/`pgdn` scroll its diff.
    fn review(&mut self, key: KeyEvent, changes: Changes) -> Edited {
        let lines = changes.files.iter().map(|file| file.diff().lines().count()).sum();
        if let Some(scroll) = scrolled(self.scroll, key.code, lines) {
            self.scroll = scroll;
            return Edited::Going;
        }
        match key.code {
            KeyCode::Char('w') if !changes.is_empty() => {
                self.stage = Stage::Saving(changes.clone());
                Edited::Asks(Ask::Save(changes))
            }
            KeyCode::Esc => {
                self.stage = Stage::Text;
                Edited::Going
            }
            _ => Edited::Going,
        }
    }

    /// Takes an update meant for the edit.
    pub fn take(&mut self, update: &Update) -> Edited {
        match (update, &self.stage) {
            (Update::Opened { path, text, digest }, Stage::Opening) if *path == self.path => {
                (self.area, self.original, self.digest) = (TextArea::new(text), text.clone(), digest.clone());
                self.stage = Stage::Text;
                Edited::Going
            }
            (Update::Edited(changes), Stage::Drafting) => {
                (self.stage, self.scroll) = (Stage::Drafted(changes.clone()), 0);
                Edited::Going
            }
            (Update::Saved(_), Stage::Saving(_)) => Edited::Closed(Some(format!("wrote {}", self.path))),
            (Update::EditFailed(reason), stage) => self.failed(reason, stage.clone()),
            _ => Edited::Going,
        }
    }

    /// The edit's ask failed: the file is not opened, the draft is refused and the text kept, or
    /// the write failed and the draft is kept.
    fn failed(&mut self, reason: &str, stage: Stage) -> Edited {
        match stage {
            Stage::Opening => Edited::Dropped(reason.to_owned()),
            Stage::Drafting => {
                (self.stage, self.refusal) = (Stage::Text, Some(reason.to_owned()));
                Edited::Going
            }
            Stage::Saving(changes) => {
                self.stage = Stage::Drafted(changes);
                Edited::Failed(reason.to_owned())
            }
            Stage::Text | Stage::Drafted(_) => Edited::Going,
        }
    }
}

#[cfg(test)]
#[path = "edit_session_tests.rs"]
mod tests;
