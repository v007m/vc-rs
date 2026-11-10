//! Draws the UI: the header with the tab bar, the chosen tab's screen, the footer's keys, and
//! the help over them when it is open.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::Tui;
use crate::screen::{Flight, Screen};
use crate::theme;

/// The keys every tab has, as the footer and the help show them.
const GLOBAL_KEYS: [(&str, &str); 4] = [("1-5", "choose tab"), ("tab", "next tab"), ("?", "help"), ("q", "quit")];

/// The global keys the footer still shows after a tab's own keys; the help lists the rest.
const SHORT_KEYS: [(&str, &str); 2] = [("?", "help"), ("q", "quit")];

/// The help's lines: a key and what it does.
const HELP: [(&str, &str); 6] = [
    ("1-5", "choose a tab"),
    ("tab", "next tab"),
    ("shift-tab", "previous tab"),
    ("?", "open or close this help"),
    ("esc", "close this help"),
    ("q, ctrl-c", "quit"),
];

/// The width of the help's key column.
const HELP_KEY_WIDTH: usize = 11;

/// The footer while quitting waits on what is in flight: a run, a load run, or both (see
/// [`flown`]).
const STOPPING: [&str; 3] = [
    " stopping the run in progress: its cleanups get a few seconds, then it is abandoned…",
    " stopping the load run in progress: its cleanups get a few seconds, then it is abandoned…",
    " stopping the run and the load run in progress: their cleanups get a few seconds, then they are abandoned…",
];

/// The footer when `q` waits to be pressed again: a run, a load run, or both in flight (see
/// [`flown`]).
const CONFIRM: [&str; 3] = [
    " a run is in progress: press q again to stop it and quit, any other key to stay",
    " a load run is in progress: press q again to stop it and quit, any other key to stay",
    " a run and a load run are in progress: press q again to stop them and quit, any other key to stay",
];

/// Which wording of [`STOPPING`] and [`CONFIRM`] fits `in_flight`: a run when the UI has not
/// heard of either.
const fn flown(in_flight: &[Flight]) -> usize {
    match in_flight {
        [Flight::Load] => 1,
        [_, _, ..] => 2,
        _ => 0,
    }
}

/// Draws `tui` as it was when it quit, its footer saying the run in progress is being ended.
pub fn draw_stopping(tui: &Tui, frame: &mut Frame<'_>) {
    draw(tui, frame);
    let area = frame.area();
    let footer = Rect { y: area.bottom().saturating_sub(1), height: area.height.min(1), ..area };
    frame.render_widget(Clear, footer);
    frame.render_widget(
        Paragraph::new(Span::styled(STOPPING[flown(tui.in_flight())], theme::WARN)).style(theme::BASE),
        footer,
    );
}

/// Draws `tui` into the whole of `frame`.
pub fn draw(tui: &Tui, frame: &mut Frame<'_>) {
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    frame.render_widget(Block::new().style(theme::BASE), frame.area());
    draw_header(tui, frame, header);
    let own = tui.current().map(Screen::keys).unwrap_or_default();
    if let Some(screen) = tui.current() {
        screen.draw(frame, body);
    }
    let global: &[(&str, &str)] = if own.is_empty() { &GLOBAL_KEYS } else { &SHORT_KEYS };
    if tui.confirming() {
        frame.render_widget(Paragraph::new(Span::styled(CONFIRM[flown(tui.in_flight())], theme::WARN)), footer);
    } else {
        frame.render_widget(Paragraph::new(keys(own.iter().chain(global))), footer);
    }
    if tui.help() {
        draw_help(own, frame, body);
    }
}

/// The product mark, project and profiles on the left; the tab bar on the right.
fn draw_header(tui: &Tui, frame: &mut Frame<'_>, area: Rect) {
    let options = tui.options();
    let profiles = if options.profiles.is_empty() { "default".to_owned() } else { options.profiles.join(",") };
    let left = Line::from(vec![
        Span::styled("◆ vibranium ", theme::TITLE),
        Span::raw(theme::clean(&options.project)),
        Span::styled("  profile ", theme::DIM),
        Span::raw(theme::clean(&profiles)),
    ]);
    let bar = tab_bar(tui);
    let width = u16::try_from(bar.width()).unwrap_or(u16::MAX);
    let [left_area, bar_area] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)]).areas(area);
    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(bar), bar_area);
}

/// `1 Run  2 Explore …`, the chosen one highlighted.
fn tab_bar(tui: &Tui) -> Line<'static> {
    let spans = tui.screens().iter().enumerate().flat_map(|(position, screen)| {
        let style = if position == tui.position() { theme::SELECTED } else { theme::DIM };
        [Span::styled(format!(" {} {} ", position + 1, screen.title()), style), Span::raw(" ")]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// `key label` pairs, each key marked.
fn keys<'a>(pairs: impl Iterator<Item = &'a (&'static str, &'static str)>) -> Line<'static> {
    let spans = pairs.flat_map(|(key, label)| {
        [Span::raw(" "), Span::styled(format!(" {key} "), theme::KEY), Span::styled(format!(" {label}"), theme::DIM)]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// The help, centred over `area`: the tab's own keys, then the global ones.
fn draw_help(own: &[(&'static str, &'static str)], frame: &mut Frame<'_>, area: Rect) {
    let lines: Vec<Line<'static>> = own
        .iter()
        .chain(&HELP)
        .map(|(key, does)| {
            Line::from(vec![Span::styled(format!("{key:<HELP_KEY_WIDTH$}"), theme::TITLE), Span::raw(*does)])
        })
        .collect();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX).saturating_add(2);
    let [row] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [help] = Layout::horizontal([Constraint::Length(44)]).flex(Flex::Center).areas(row);
    frame.render_widget(Clear, help);
    frame.render_widget(Paragraph::new(lines).block(theme::pane("Help")).style(theme::BASE), help);
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
