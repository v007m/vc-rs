//! The Explore tab: the project's apis and tests by file, the chosen one's details and how a
//! run sees it (`vc explain`), `t` to skip a test with a reason or to unskip it, and `e` to
//! edit the chosen item's file (`edit_session.rs`), and `c` to show only what changed in the
//! last week (D249). It reads the project when first shown, again on `r` and `c`, after each
//! skip, unskip or edit written, and when shown after the Import tab wrote files.

use std::cell::Cell;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{FileKind, Listed, SkipRequest};

use crate::edit_session::{EditSession, Edited};
use crate::explore_view::{self, explained_rows};
use crate::history_screen::ReadState;
use crate::input::{Input, Typed};
use crate::screen::{Ask, Screen, Update, clamp_step, cursor_move, refound, scrolled};
use crate::theme::first_line;

/// The Explore tab's keys.
const KEYS: [(&str, &str); 7] = [
    ("↑↓", "move"),
    ("enter", "explain"),
    ("pgup/pgdn", "scroll"),
    ("t", "skip/unskip"),
    ("e", "edit"),
    ("r", "reload"),
    ("c", "recent"),
];

/// How many days back `c` looks for changed files.
pub const RECENT_DAYS: u64 = 7;
/// The window `c` asks for: [`RECENT_DAYS`] days.
const RECENT: Duration = Duration::from_hours(RECENT_DAYS * 24);

/// What the last key came to, for the status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// It worked, or is under way.
    Said(String),
    /// It was refused or failed: the first line of why.
    Failed(String),
}

/// The Explore tab.
#[derive(Debug, Default)]
pub struct ExploreScreen {
    profiles: Vec<String>,
    reading: ReadState,
    items: Vec<Listed>,
    notices: Vec<String>,
    cursor: usize,
    explaining: Option<String>,
    explained: Option<(String, String)>,
    reason: Option<(String, Input)>,
    note: Option<Note>,
    scroll: u16,
    explain_width: Cell<u16>,
    editing: Option<EditSession>,
    recent: bool,
}

/// Whether `item` is an api, not a test.
pub fn is_api(item: &Listed) -> bool {
    item.kind() == Some(FileKind::Api)
}

impl ExploreScreen {
    /// The tab before the project is read, explaining under `profiles`.
    pub fn new(profiles: Vec<String>) -> Self {
        Self { profiles, ..Self::default() }
    }

    /// Where reading the project has got to.
    pub const fn reading(&self) -> &ReadState {
        &self.reading
    }

    /// The apis and tests, by folder, each folder's apis first.
    pub fn items(&self) -> &[Listed] {
        &self.items
    }

    /// What did not stop the last reading of the project.
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// The chosen item's position.
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// The chosen item.
    pub fn chosen(&self) -> Option<&Listed> {
        self.items.get(self.cursor)
    }

    /// The item being explained.
    pub fn explaining(&self) -> Option<&str> {
        self.explaining.as_deref()
    }

    /// The last explanation read: the item's ID and the text.
    pub const fn explained(&self) -> Option<&(String, String)> {
        self.explained.as_ref()
    }

    /// The test being skipped, as chosen when `t` was pressed, and the reason being typed.
    pub fn reason(&self) -> Option<(&str, &Input)> {
        self.reason.as_ref().map(|(target, typed)| (target.as_str(), typed))
    }

    /// How far the explanation is scrolled, in lines.
    pub const fn scroll(&self) -> u16 {
        self.scroll
    }

    /// The rows of the chosen item's explanation, when it is the one shown: its lines as the
    /// Explain pane last drawn wraps them, or as written before it is drawn.
    fn explained_lines(&self) -> usize {
        let shown = self.explained.as_ref().filter(|(id, _)| self.chosen().is_some_and(|chosen| chosen.id == *id));
        let width = self.explain_width.get();
        shown.map_or(0, |(_, text)| if width == 0 { text.lines().count() } else { explained_rows(text, width) })
    }

    /// The Explain pane was drawn `width` columns wide within its borders, which is where its
    /// long lines wrap, so `pgdn` can reach the end of a wrapped explanation.
    pub fn explain_drawn(&self, width: u16) {
        self.explain_width.set(width);
    }

    /// What the last key came to.
    pub const fn note(&self) -> Option<&Note> {
        self.note.as_ref()
    }

    /// Whether only what changed in the last [`RECENT_DAYS`] days is shown.
    pub const fn recent(&self) -> bool {
        self.recent
    }

    /// The file being edited.
    pub const fn editing(&self) -> Option<&EditSession> {
        self.editing.as_ref()
    }

    /// Opens the chosen item's file in the editor.
    fn edit(&mut self) -> Option<Ask> {
        let (session, ask) = EditSession::open(&self.chosen()?.path);
        (self.editing, self.note) = (Some(session), None);
        Some(ask)
    }

    /// Takes what a key or an update did to the edit; what it asks of the engine, if anything.
    fn edited(&mut self, edited: Edited) -> Option<Ask> {
        match edited {
            Edited::Going => None,
            Edited::Asks(ask) => Some(ask),
            Edited::Closed(said) => {
                (self.editing, self.note) = (None, said.map(Note::Said));
                None
            }
            Edited::Dropped(reason) => {
                (self.editing, self.note) = (None, Some(Note::Failed(first_line(&reason))));
                None
            }
            Edited::Failed(reason) => {
                self.note = Some(Note::Failed(first_line(&reason)));
                None
            }
        }
    }

    /// Asks for the project again, unless it is being read.
    fn reload(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Reading {
            return None;
        }
        self.reading = ReadState::Reading;
        Some(if self.recent { Ask::ExploreChanged(RECENT) } else { Ask::Explore })
    }

    /// Shows only what changed lately, or everything again, unless the project is being read.
    fn toggle_recent(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Reading {
            return None;
        }
        self.recent = !self.recent;
        self.reload()
    }

    /// Asks how a run sees the chosen item.
    fn explain(&mut self) -> Option<Ask> {
        let id = self.chosen()?.id.clone();
        self.explaining = Some(id.clone());
        Some(Ask::Explain { id, profiles: self.profiles.clone() })
    }

    /// Unskips the chosen test when it is skipped, or starts on the reason to skip it.
    fn quarantine(&mut self) -> Option<Ask> {
        let chosen = self.chosen()?;
        let (id, api, skipped) = (chosen.id.clone(), is_api(chosen), chosen.skipped);
        if api {
            self.note = Some(Note::Failed(format!("{id} is an api: t skips a test")));
            return None;
        }
        if skipped {
            self.note = Some(Note::Said(format!("unskipping {id}…")));
            return Some(Ask::Unskip(id));
        }
        self.note = None;
        self.reason = Some((id, Input::default()));
        None
    }

    /// Applies a key to the reason being typed; on `enter`, asks to skip with it.
    fn type_reason(&mut self, key: KeyEvent) -> Option<Ask> {
        match self.reason.as_mut()?.1.press(key) {
            Typed::Editing => None,
            Typed::Dropped => {
                self.reason = None;
                None
            }
            Typed::Done => self.skip(),
        }
    }

    /// Asks to skip the test chosen when `t` was pressed with the reason typed, unless the
    /// reason is blank: a re-read meanwhile may have moved what the cursor is on.
    fn skip(&mut self) -> Option<Ask> {
        let (target, typed) = self.reason.as_ref()?;
        let (target, reason) = (target.clone(), typed.text().trim().to_owned());
        if reason.is_empty() {
            self.note = Some(Note::Failed("a skip needs a reason, for the report".to_owned()));
            return None;
        }
        self.reason = None;
        self.note = Some(Note::Said(format!("skipping {target}…")));
        Some(Ask::Skip(SkipRequest { target, reason, until: None }))
    }

    fn step(&mut self, down: bool) {
        self.cursor = clamp_step(self.cursor, self.items.len(), down);
        self.scroll = 0;
    }

    /// Takes the listing: each folder's apis, then its tests, each file in project order; the
    /// cursor kept on the item it was on.
    fn read(&mut self, items: &[Listed], notices: &[String]) {
        let chosen = self.items.get(self.cursor).map(|item| item.id.clone());
        let folder = |item: &Listed| item.path.rsplit_once('/').map_or("", |(folder, _)| folder).to_owned();
        let mut items = items.to_vec();
        items.sort_by_cached_key(|item| (folder(item), !is_api(item)));
        self.items = items;
        self.notices = notices.to_vec();
        self.reading = ReadState::Read;
        self.cursor = refound(&self.items, |item| chosen.as_ref() == Some(&item.id), self.cursor);
    }

    /// Files were written elsewhere: the project shown is stale, so it is read again when the
    /// tab is next shown, unless a read is under way.
    fn written(&mut self) {
        if self.reading != ReadState::Reading {
            self.reading = ReadState::Unread;
        }
    }

    /// The project could not be read: the reading failed, or the re-read after a skip did.
    fn failed(&mut self, reason: &str) {
        let first = first_line(reason);
        if self.reading == ReadState::Reading {
            self.reading = ReadState::Failed(first);
        } else {
            self.note = Some(Note::Failed(first));
        }
    }
}

impl Screen for ExploreScreen {
    fn title(&self) -> &'static str {
        "Explore"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &KEYS
    }

    fn shown(&mut self) -> Option<Ask> {
        if self.reading == ReadState::Unread { self.reload() } else { None }
    }

    fn typing(&self) -> bool {
        self.reason.is_some() || self.editing.is_some()
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if let Some(session) = self.editing.as_mut() {
            let edited = session.press(key);
            self.note = None;
            return self.edited(edited);
        }
        if self.reason.is_some() {
            return self.type_reason(key);
        }
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        if let Some(scroll) = scrolled(self.scroll, key.code, self.explained_lines()) {
            self.scroll = scroll;
            return None;
        }
        match key.code {
            KeyCode::Enter => return self.explain(),
            KeyCode::Char('t') => return self.quarantine(),
            KeyCode::Char('e') => return self.edit(),
            KeyCode::Char('c') => return self.toggle_recent(),
            KeyCode::Char('r') => return self.reload(),
            _ => {}
        }
        None
    }

    fn take(&mut self, update: &Update) {
        if let Some(session) = self.editing.as_mut() {
            let edited = session.take(update);
            self.edited(edited);
        }
        match update {
            Update::Explored { items, notices } => self.read(items, notices),
            Update::Explained { id, text } if self.explaining.as_ref() == Some(id) => {
                self.explained = Some((id.clone(), text.clone()));
                (self.explaining, self.scroll) = (None, 0);
            }
            Update::Quarantined { target, skipped } => {
                let done = if *skipped { "skipped" } else { "unskipped" };
                self.note = Some(Note::Said(format!("{target} {done}")));
            }
            Update::ExploreFailed(reason) => self.failed(reason),
            Update::ExplainFailed { id, reason } if self.explaining.as_ref() == Some(id) => {
                self.explaining = None;
                self.note = Some(Note::Failed(first_line(reason)));
            }
            Update::SkipFailed(reason) => self.note = Some(Note::Failed(first_line(reason))),
            Update::Written(_) => self.written(),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        explore_view::draw(self, frame, area);
    }
}
