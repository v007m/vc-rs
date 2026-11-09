//! The UI's state and what each key does to it. Nothing here touches the terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;

use crate::tabs::{TABS, Tab};
use crate::view;

/// What `vc tui` was started with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TuiOptions {
    /// The project's name, in the header.
    pub project: String,
    /// The active profiles (`--profile`); the config's default when empty.
    pub profiles: Vec<String>,
}

impl TuiOptions {
    /// Options for `project` under `profiles`.
    #[must_use]
    pub fn new(project: impl Into<String>, profiles: Vec<String>) -> Self {
        Self { project: project.into(), profiles }
    }
}

/// The terminal UI's state: the chosen tab, whether the help is open, and whether to quit.
#[derive(Debug)]
#[non_exhaustive]
pub struct Tui {
    options: TuiOptions,
    tab: usize,
    help: bool,
    quitting: bool,
}

impl Tui {
    /// The UI as it opens: on the first tab, the help closed.
    #[must_use]
    pub const fn new(options: TuiOptions) -> Self {
        Self { options, tab: 0, help: false, quitting: false }
    }

    /// Applies one key: the digits choose a tab, `tab` and `shift-tab` cycle them, `?` opens
    /// and closes the help, `esc` closes it, and `q` or `ctrl-c` quits. A release or repeat
    /// event is ignored.
    pub fn press(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        let interrupt = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match key.code {
            _ if interrupt => self.quitting = true,
            KeyCode::Char('q') => self.quitting = true,
            KeyCode::Char('?') => self.help = !self.help,
            KeyCode::Esc => self.help = false,
            KeyCode::Tab => self.choose(self.tab + 1),
            KeyCode::BackTab => self.choose(self.tab + TABS.len() - 1),
            KeyCode::Char(digit) => {
                let chosen = digit.to_digit(10).and_then(|number| usize::try_from(number).ok());
                if let Some(number) = chosen.filter(|number| (1..=TABS.len()).contains(number)) {
                    self.choose(number - 1);
                }
            }
            _ => {}
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
        self.current().title
    }

    /// Draws the whole screen into `frame`.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        view::draw(self, frame);
    }

    /// What `vc tui` was started with.
    pub(crate) const fn options(&self) -> &TuiOptions {
        &self.options
    }

    /// Whether the help is open.
    pub(crate) const fn help(&self) -> bool {
        self.help
    }

    /// The position of the chosen tab.
    pub(crate) const fn position(&self) -> usize {
        self.tab
    }

    /// The chosen tab.
    pub(crate) fn current(&self) -> Tab {
        TABS.get(self.tab).copied().unwrap_or(TABS[0])
    }

    /// Chooses the tab at `position`, wrapping round, and closes the help.
    const fn choose(&mut self, position: usize) {
        self.tab = position % TABS.len();
        self.help = false;
    }
}
