//! The Import tab: what `vc import` takes, filled in as a form, drafted with `d` into the
//! files it would create or change, each shown as a diff, then written with `w` or discarded
//! with `x`. Nothing is written before `w`.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use vc_engine::{Changes, ImportRequest};

use crate::import_view;
use crate::input::{Input, Typed};
use crate::screen::{Ask, Screen, Update, clamp_step, cursor_move, scrolled};
use crate::theme::{clean, first_line};

/// The Import tab's keys.
const KEYS: [(&str, &str); 8] = [
    ("↑↓", "field"),
    ("←→", "importer"),
    ("enter", "edit"),
    ("d", "draft"),
    ("n", "next"),
    ("pgup/pgdn", "scroll"),
    ("w", "write"),
    ("x", "discard"),
];
/// The importer the form starts on, when the engine has it: the spec most imports are of.
const FIRST: &str = "openapi";
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
    /// The importer's position among the engine's importers.
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
    importers: Vec<String>,
    form: Form,
    field: usize,
    editing: Option<Input>,
    phase: ImportPhase,
    shown: usize,
    scroll: u16,
    kept: Option<Changes>,
    note: Option<String>,
}

impl ImportScreen {
    /// The tab with an empty form, importing under `profiles` with one of `importers`, the
    /// engine's importers by name.
    pub fn new(profiles: Vec<String>, importers: Vec<String>) -> Self {
        let kind = importers.iter().position(|name| name == FIRST).unwrap_or_default();
        Self { profiles, importers, form: Form { kind, ..Form::default() }, ..Self::default() }
    }

    /// The chosen importer's name.
    pub fn importer(&self) -> &str {
        self.importers.get(self.form.kind).map_or("", String::as_str)
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

    /// How far the diff shown is scrolled, in lines.
    pub const fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Why writing the draft shown, or drafting another, failed: the first line.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The request the form makes.
    fn request(&self) -> ImportRequest {
        let [tests, negative, auth] = self.form.tests;
        let file_id = Some(self.form.file_id.trim().to_owned()).filter(|id| !id.is_empty());
        let kind = self.importer().to_owned();
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
        self.kept = match std::mem::take(&mut self.phase) {
            ImportPhase::Drafted(changes) => Some(changes),
            _ => None,
        };
        self.note = None;
        self.phase = ImportPhase::Drafting(clean(&request.source));
        Some(Ask::Import(Box::new(request)))
    }

    /// Writes the draft shown.
    fn write(&mut self) -> Option<Ask> {
        let ImportPhase::Drafted(changes) = &self.phase else { return None };
        let changes = changes.clone();
        self.note = None;
        self.phase = ImportPhase::Writing(changes.clone());
        Some(Ask::Write(changes))
    }

    /// Edits the chosen text row, or flips the chosen toggle.
    fn enter(&mut self) {
        match self.field {
            1 => self.editing = Some(Input::new(&self.form.source)),
            2 => self.editing = Some(Input::new(&self.form.file_id)),
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
        let count = if self.importers.is_empty() { 1 } else { self.importers.len() };
        self.form.kind = if on { (self.form.kind + 1) % count } else { (self.form.kind + count - 1) % count };
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
            self.scroll = 0;
        }
    }

    fn step(&mut self, down: bool) {
        self.field = clamp_step(self.field, FIELDS, down);
    }

    /// Drops the draft, or the last failure, back to the form.
    fn discard(&mut self) {
        if !matches!(self.phase, ImportPhase::Drafting(_) | ImportPhase::Writing(_)) {
            self.phase = ImportPhase::Form;
            self.note = None;
        }
    }

    /// Takes the draft, if it is the one asked for.
    fn drafted(&mut self, changes: &Changes) {
        if matches!(self.phase, ImportPhase::Drafting(_)) {
            self.phase = ImportPhase::Drafted(changes.clone());
            (self.shown, self.scroll, self.kept) = (0, 0, None);
        }
    }

    /// Takes why the draft or write asked for failed, if one was asked for: a failed write, or
    /// a failed draft while another was shown, keeps the draft shown, with why on the status
    /// line.
    fn import_failed(&mut self, reason: &str) {
        let kept = match std::mem::take(&mut self.phase) {
            ImportPhase::Writing(changes) => Some(changes),
            ImportPhase::Drafting(_) => self.kept.take(),
            other => {
                self.phase = other;
                return;
            }
        };
        match kept {
            Some(changes) => {
                self.phase = ImportPhase::Drafted(changes);
                self.note = Some(first_line(reason));
            }
            None => self.phase = ImportPhase::Failed(reason.to_owned()),
        }
    }

    /// The lines of the diff shown.
    fn diff_lines(&self) -> usize {
        let (ImportPhase::Drafted(changes) | ImportPhase::Writing(changes)) = &self.phase else { return 0 };
        changes.files.get(self.shown).map_or(0, |file| file.diff().lines().count())
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
        if self.editing.is_some() {
            self.type_into(key);
            return None;
        }
        if let Some(down) = cursor_move(key.code) {
            self.step(down);
            return None;
        }
        if let Some(scroll) = scrolled(self.scroll, key.code, self.diff_lines()) {
            self.scroll = scroll;
            return None;
        }
        match key.code {
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
            Update::ImportFailed(reason) => self.import_failed(reason),
            _ => {}
        }
    }

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        import_view::draw(self, frame, area);
    }
}
