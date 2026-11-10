//! The Import tab: what `vc import` takes, filled in as a form, drafted with `d` into the
//! files it would create or change, each shown as a diff, then written with `w` or discarded
//! with `x`. Nothing is written before `w`.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{Changes, ImportRequest};

use crate::import_view;
use crate::input::{Input, Typed};
use crate::screen::{Ask, Screen, Update};
use crate::theme::clean;

/// The Import tab's keys.
const KEYS: [(&str, &str); 7] = [
    ("↑↓", "field"),
    ("←→", "importer"),
    ("enter", "edit/toggle"),
    ("d", "draft"),
    ("n", "next file"),
    ("w", "write"),
    ("x", "discard"),
];
/// The importers, as `vc import <kind>` names them.
pub const KINDS: [&str; 6] = ["openapi", "curl", "har", "http", "postman", "bruno"];
/// The form's rows: the importer, the source, the file ID, then the three kinds of test.
const FIELDS: usize = 6;
/// The first of the rows that toggle.
const TOGGLES: usize = 3;

/// Where the import has got to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ImportPhase {
    /// The form is being filled in.
    #[default]
    Form,
    /// Asked for the draft.
    Drafting(String),
    /// The draft, shown, not written.
    Drafted(Changes),
    /// Asked to write the draft.
    Writing(Changes),
    /// Written: the change's titles.
    Written(Vec<String>),
    /// Refused or failed: why, with its help.
    Failed(String),
}

/// What `vc import` takes, as the form holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    /// The importer's position in [`KINDS`].
    pub kind: usize,
    /// The URL or path.
    pub source: String,
    /// The file ID; blank derives it from the source.
    pub file_id: String,
    /// `--tests`, `--negative` and `--auth`.
    pub tests: [bool; 3],
}

/// The Import tab.
#[derive(Debug, Default)]
pub struct ImportScreen {
    profiles: Vec<String>,
    form: Form,
    field: usize,
    editing: Option<Input>,
    phase: ImportPhase,
    shown: usize,
}

impl ImportScreen {
    /// The tab with an empty form, importing under `profiles`.
    pub fn new(profiles: Vec<String>) -> Self {
        Self { profiles, ..Self::default() }
    }

    /// What the form holds.
    pub const fn form(&self) -> &Form {
        &self.form
    }

    /// The chosen row.
    pub const fn field(&self) -> usize {
        self.field
    }

    /// The text being typed into the chosen row.
    pub const fn editing(&self) -> Option<&Input> {
        self.editing.as_ref()
    }

    /// Where the import has got to.
    pub const fn phase(&self) -> &ImportPhase {
        &self.phase
    }

    /// The position of the drafted file whose diff shows.
    pub const fn shown_file(&self) -> usize {
        self.shown
    }

    /// The request the form makes.
    fn request(&self) -> ImportRequest {
        let [tests, negative, auth] = self.form.tests;
        let file_id = Some(self.form.file_id.trim().to_owned()).filter(|id| !id.is_empty());
        let kind = KINDS.get(self.form.kind).copied().unwrap_or_default().to_owned();
        let source = self.form.source.trim().to_owned();
        ImportRequest {
            kind,
            source,
            file_id,
            tests,
            negative,
            auth,
            profiles: self.profiles.clone(),
            ..ImportRequest::default()
        }
    }

    /// Drafts the import, unless one is under way or there is no source.
    fn draft(&mut self) -> Option<Ask> {
        if matches!(self.phase, ImportPhase::Drafting(_) | ImportPhase::Writing(_)) {
            return None;
        }
        let request = self.request();
        if request.source.is_empty() {
            self.phase = ImportPhase::Failed("a source is needed: a URL, or a path inside the project".to_owned());
            return None;
        }
        self.phase = ImportPhase::Drafting(clean(&request.source));
        Some(Ask::Import(Box::new(request)))
    }

    /// Writes the draft shown.
    fn write(&mut self) -> Option<Ask> {
        let ImportPhase::Drafted(changes) = &self.phase else { return None };
        let changes = changes.clone();
        self.phase = ImportPhase::Writing(changes.clone());
        Some(Ask::Write(changes))
    }

    /// Edits the chosen text row, or flips the chosen toggle.
    fn enter(&mut self) {
        match self.field {
            1 | 2 => self.editing = Some(Input::default()),
            row => self.toggle(row),
        }
    }

    fn toggle(&mut self, row: usize) {
        if let Some(on) = row.checked_sub(TOGGLES).and_then(|at| self.form.tests.get_mut(at)) {
            *on = !*on;
        }
    }

    /// Steps the importer: on, or back.
    const fn turn(&mut self, on: bool) {
        self.form.kind =
            if on { (self.form.kind + 1) % KINDS.len() } else { (self.form.kind + KINDS.len() - 1) % KINDS.len() };
    }

    /// Hands `key` to the text being typed; `enter` puts it into its row.
    fn type_into(&mut self, key: KeyEvent) {
        let Some(input) = self.editing.as_mut() else { return };
        if input.press(key) == Typed::Editing {
            return;
        }
        let typed = self.editing.take().filter(|_| key.code == KeyCode::Enter);
        let row = if self.field == 1 { &mut self.form.source } else { &mut self.form.file_id };
        if let Some(typed) = typed {
            typed.text().clone_into(row);
        }
    }

    /// Shows the next drafted file's diff.
    fn next_file(&mut self) {
        if let ImportPhase::Drafted(changes) = &self.phase {
            self.shown = (self.shown + 1) % changes.files.len().max(1);
        }
    }

    fn step(&mut self, down: bool) {
        self.field = if down { (self.field + 1).min(FIELDS - 1) } else { self.field.saturating_sub(1) };
    }

    /// Drops the draft, or the last failure, back to the form.
    fn discard(&mut self) {
        if !matches!(self.phase, ImportPhase::Drafting(_) | ImportPhase::Writing(_)) {
            self.phase = ImportPhase::Form;
        }
    }

    /// Takes the draft, if it is the one asked for.
    fn drafted(&mut self, changes: &Changes) {
        if matches!(self.phase, ImportPhase::Drafting(_)) {
            self.phase = ImportPhase::Drafted(changes.clone());
            self.shown = 0;
        }
    }
}

impl Screen for ImportScreen {
    fn title(&self) -> &'static str {
        "Import"
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &KEYS
    }

    fn typing(&self) -> bool {
        self.editing.is_some()
    }

    fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        match key.code {
            _ if self.editing.is_some() => self.type_into(key),
            KeyCode::Up | KeyCode::Char('k') => self.step(false),
            KeyCode::Down | KeyCode::Char('j') => self.step(true),
            KeyCode::Left | KeyCode::Right if self.field == 0 => self.turn(key.code == KeyCode::Right),
            KeyCode::Enter | KeyCode::Char(' ') => self.enter(),
            KeyCode::Char('n') => self.next_file(),
            KeyCode::Char('x') => self.discard(),
            KeyCode::Char('d') => return self.draft(),
            KeyCode::Char('w') => return self.write(),
            _ => {}
        }
        None
    }

    fn take(&mut self, update: &Update) {
        match update {
            Update::Drafted(changes) => self.drafted(changes),
            Update::Written(titles) if matches!(self.phase, ImportPhase::Writing(_)) => {
                self.phase = ImportPhase::Written(titles.clone());
            }
            Update::ImportFailed(reason) => self.phase = ImportPhase::Failed(reason.clone()),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        import_view::draw(self, frame, area);
    }
}
