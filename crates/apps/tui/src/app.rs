//! The UI's state and what each key does to it. Nothing here touches the terminal or the
//! engine: a key that needs the engine gives back an [`Ask`], and what the engine sends back
//! comes in as an [`Update`].

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use vc_engine::{Engine, RunFeed};

use crate::screen::{Ask, Flight, Screen, Update};
use crate::{tabs, view};

/// What `vc tui` was started with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TuiOptions {
    /// The project's name, in the header.
    pub project: String,
    /// The active profiles (`--profile`); the config's default when empty.
    pub profiles: Vec<String>,
    /// The importers the Import tab offers, by name.
    pub importers: Vec<String>,
    /// Where a path typed into a tab starts from: the working folder, or the project root
    /// with `--project`; the process's working folder when empty.
    pub base: PathBuf,
}

impl TuiOptions {
    /// Options for `project` under `profiles`, offering the engine's importers.
    #[must_use]
    pub fn new(project: impl Into<String>, profiles: Vec<String>) -> Self {
        Self { project: project.into(), profiles, importers: Engine::importers(), base: PathBuf::new() }
    }
}

/// The terminal UI's state: the tabs' screens, the chosen one, whether the help is open, the
/// run or load run in flight, and whether to quit.
#[derive(Debug)]
#[non_exhaustive]
pub struct Tui {
    options: TuiOptions,
    screens: Vec<Box<dyn Screen>>,
    tab: usize,
    help: bool,
    in_flight: Option<Flight>,
    confirming: bool,
    quitting: bool,
}

impl Tui {
    /// The UI as it opens: on the first tab, the help closed.
    #[must_use]
    pub fn new(options: TuiOptions) -> Self {
        let screens = tabs::screens(&options);
        Self { options, screens, tab: 0, help: false, in_flight: None, confirming: false, quitting: false }
    }

    /// Applies one key: the digits choose a tab, `tab` and `shift-tab` cycle them, `?` opens
    /// and closes the help, `esc` closes it, and `q` (without `ctrl` or `alt`) or `ctrl-c`
    /// quits; any other key goes to the chosen tab, which may ask the engine for something, as
    /// may a tab when it is chosen. While a run or load run is in flight, `q` asks to be pressed
    /// again, as quitting stops it. While the help is open, every other key is dropped. While
    /// the chosen tab takes text, every key but `ctrl-c` goes to it. A release or repeat event
    /// is ignored.
    pub fn press(&mut self, key: KeyEvent) -> Option<Ask> {
        if key.kind != KeyEventKind::Press {
            return None;
        }
        let confirming = std::mem::take(&mut self.confirming);
        let ask = self.apply(key, confirming);
        if let Some(flight) = asked(ask.as_ref()) {
            self.in_flight.get_or_insert(flight);
        }
        ask
    }

    /// [`Tui::press`] for a key pressed; `confirming` when the key before was a `q` that asked
    /// to be pressed again.
    fn apply(&mut self, key: KeyEvent, confirming: bool) -> Option<Ask> {
        let interrupt = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        let bare = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match key.code {
            _ if interrupt => self.quitting = true,
            _ if self.current().is_some_and(Screen::typing) => return self.hand_on(key),
            KeyCode::Char('q') if bare => self.quit(confirming),
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Esc => self.help = false,
            _ if self.help => {}
            KeyCode::Tab => return self.choose(self.tab + 1),
            KeyCode::BackTab => return self.choose(self.tab + self.screens.len() - 1),
            KeyCode::Char(digit) if digit.is_ascii_digit() => return self.choose_number(digit),
            _ => return self.hand_on(key),
        }
        None
    }

    /// Hands `key` to the chosen tab; what it asks for.
    fn hand_on(&mut self, key: KeyEvent) -> Option<Ask> {
        self.screens.get_mut(self.tab).and_then(|screen| screen.press(key))
    }

    /// Quits, unless a run or load run is in flight and the key before was not this `q`: then
    /// asks for `q` again.
    const fn quit(&mut self, confirming: bool) {
        if self.in_flight.is_none() || confirming {
            self.quitting = true;
        } else {
            self.confirming = true;
        }
    }

    /// Hands what the engine sent back to the tabs; the end of the run or load run in flight
    /// lets `q` quit at once again.
    pub fn update(&mut self, update: &Update) {
        if self.in_flight.is_some() && self.in_flight == ended(update) {
            self.in_flight = None;
            self.confirming = false;
        }
        for screen in &mut self.screens {
            screen.take(update);
        }
    }

    /// Whether a key asked to quit.
    #[must_use]
    pub const fn quitting(&self) -> bool {
        self.quitting
    }

    /// The chosen tab's title.
    #[must_use]
    pub fn tab(&self) -> &'static str {
        self.current().map_or("", |screen| screen.title())
    }

    /// Draws the whole screen into `frame`.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        view::draw(self, frame);
    }

    /// What `vc tui` was started with.
    pub(crate) const fn options(&self) -> &TuiOptions {
        &self.options
    }

    /// Whether `q` was pressed during a run or load run and waits to be pressed again.
    pub(crate) const fn confirming(&self) -> bool {
        self.confirming
    }

    /// Whether the help is open.
    pub(crate) const fn help(&self) -> bool {
        self.help
    }

    /// The position of the chosen tab.
    pub(crate) const fn position(&self) -> usize {
        self.tab
    }

    /// Every tab's screen, in order.
    pub(crate) fn screens(&self) -> &[Box<dyn Screen>] {
        &self.screens
    }

    /// The chosen tab's screen.
    pub(crate) fn current(&self) -> Option<&dyn Screen> {
        self.screens.get(self.tab).map(AsRef::as_ref)
    }

    /// Chooses the tab numbered `digit` from 1, when there is one.
    fn choose_number(&mut self, digit: char) -> Option<Ask> {
        let number = digit.to_digit(10).and_then(|number| usize::try_from(number).ok());
        let number = number.filter(|number| (1..=self.screens.len()).contains(number))?;
        self.choose(number - 1)
    }

    /// Chooses the tab at `position`, wrapping round, and closes the help; what the tab asks
    /// for to show.
    fn choose(&mut self, position: usize) -> Option<Ask> {
        self.tab = position % self.screens.len().max(1);
        self.help = false;
        self.screens.get_mut(self.tab).and_then(|screen| screen.shown())
    }
}

/// The work in flight `ask` starts, if the driver takes it.
const fn asked(ask: Option<&Ask>) -> Option<Flight> {
    match ask {
        Some(Ask::Run(_)) => Some(Flight::Run),
        Some(Ask::Load(_)) => Some(Flight::Load),
        _ => None,
    }
}

/// The work in flight `update` ends: a run's last feed item or failure, or a load run's.
const fn ended(update: &Update) -> Option<Flight> {
    match update {
        Update::Run(RunFeed::Done(_)) | Update::RunFailed(_) => Some(Flight::Run),
        Update::Loaded(_) | Update::LoadFailed(_) => Some(Flight::Load),
        _ => None,
    }
}
