//! A tab whose screen is not built yet: its title and what it will show.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};

use crate::screen::{Ask, Screen, Update};
use crate::theme;

/// A tab that only says what it is for.
#[derive(Debug)]
pub struct About {
    title: &'static str,
    about: &'static str,
}

impl About {
    /// The tab `title`, showing `about`.
    pub const fn new(title: &'static str, about: &'static str) -> Self {
        Self { title, about }
    }
}

impl Screen for About {
    fn title(&self) -> &'static str {
        self.title
    }

    fn keys(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn press(&mut self, _key: KeyEvent) -> Option<Ask> {
        None
    }

    fn take(&mut self, _update: &Update) {}

    fn draw(&self, frame: &mut Frame<'_>, area: Rect) {
        let paragraph = Paragraph::new(Span::styled(self.about, theme::DIM)).wrap(Wrap { trim: true });
        frame.render_widget(paragraph.block(theme::pane(self.title)), area);
    }
}
