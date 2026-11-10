//! Draws the file being edited in the Explore tab: its text with the cursor kept in view and
//! why the guard refused the last draft, or the diff of every file the draft writes; and its
//! status line.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use crate::edit_session::{EditSession, Stage};
use crate::import_view::diff_pane;
use crate::theme::{self, clean};

/// The most rows the Refused pane takes, borders included.
const REFUSAL_HEIGHT: usize = 8;

/// Draws the edit into `area`.
pub fn draw(session: &EditSession, frame: &mut Frame<'_>, area: Rect) {
    match session.stage() {
        Stage::Opening => frame.render_widget(Paragraph::new("").block(theme::pane(&title(session))), area),
        Stage::Drafted(changes) | Stage::Saving(changes) => {
            frame.render_widget(diff_pane(&changes.files).scroll((session.scroll(), 0)), area);
        }
        Stage::Text | Stage::Drafting => draw_text(session, frame, area),
    }
}

/// `Edit services/users/users.api.yaml *`, the mark when the text was changed.
fn title(session: &EditSession) -> String {
    format!("Edit {}{}", session.path(), if session.dirty() { " *" } else { "" })
}

/// The text, scrolled to keep the cursor in view and the cursor shown on it; under it, why the
/// last draft was refused.
fn draw_text(session: &EditSession, frame: &mut Frame<'_>, area: Rect) {
    let refusal = session.refusal().map(|reason| reason.lines().map(|line| Line::raw(clean(line))).collect());
    let refused: Vec<Line<'static>> = refusal.unwrap_or_default();
    let height = u16::try_from((refused.len() + 2).min(REFUSAL_HEIGHT)).unwrap_or_default();
    let height = if refused.is_empty() { 0 } else { height };
    let [text, below] = Layout::vertical([Constraint::Fill(1), Constraint::Length(height)]).areas(area);
    let block = theme::pane(&title(session));
    let inner = block.inner(text);
    let (row, column) = session.area().cursor();
    let (top, left) = (kept_in_view(row, inner.height), kept_in_view(column, inner.width));
    let lines: Vec<Line<'static>> = session.area().lines().iter().map(|line| Line::raw(clean(line))).collect();
    frame.render_widget(Paragraph::new(lines).scroll((top, left)).block(block), text);
    if *session.stage() == Stage::Text && !session.confirming() {
        let at = |cursor: usize, first: u16| u16::try_from(cursor).unwrap_or(u16::MAX).saturating_sub(first);
        frame.set_cursor_position(Position::new(inner.x + at(column, left), inner.y + at(row, top)));
    }
    let pane = theme::pane("Refused");
    frame.render_widget(Paragraph::new(refused).wrap(Wrap { trim: false }).block(pane), below);
}

/// The first row or column shown of `span` ones, so that `cursor` is the last when it would be
/// past them.
fn kept_in_view(cursor: usize, span: u16) -> u16 {
    u16::try_from(cursor.saturating_sub(usize::from(span.saturating_sub(1)))).unwrap_or(u16::MAX)
}

/// Where the edit has got to and its keys.
pub fn status_line(session: &EditSession) -> Line<'static> {
    let path = clean(session.path());
    match session.stage() {
        Stage::Opening => Line::styled(format!("opening {path}…"), theme::INFO),
        Stage::Text if session.confirming() => {
            Line::styled(format!("drop the changes to {path}? y drops them, any other key goes back"), theme::WARN)
        }
        Stage::Text => Line::styled("ctrl-s checks the change and shows it · esc leaves", theme::DIM),
        Stage::Drafting => Line::styled(format!("checking the change to {path}…"), theme::INFO),
        Stage::Drafted(changes) if changes.is_empty() => {
            Line::styled("nothing changed · esc goes back to the text", theme::DIM)
        }
        Stage::Drafted(_) => Line::styled(
            "the change passes the checks · w writes it, esc goes back to the text, pgup/pgdn scroll",
            theme::PASS,
        ),
        Stage::Saving(_) => Line::styled(format!("writing {path}…"), theme::INFO),
    }
}

#[cfg(test)]
#[path = "edit_view_tests.rs"]
mod tests;
