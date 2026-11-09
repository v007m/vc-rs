//! Draws the UI: the header with the tab bar, the chosen tab's pane, the footer's keys, and the
//! help over them when it is open.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::app::Tui;
use crate::tabs::TABS;
use crate::theme;

/// The keys every tab has, as the footer and the help show them.
const GLOBAL_KEYS: [(&str, &str); 4] = [("1-5", "choose tab"), ("tab", "next tab"), ("?", "help"), ("q", "quit")];

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

/// Draws `tui` into the whole of `frame`.
pub fn draw(tui: &Tui, frame: &mut Frame<'_>) {
    let [header, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)]).areas(frame.area());
    frame.render_widget(Block::new().style(theme::BASE), frame.area());
    draw_header(tui, frame, header);
    frame.render_widget(pane(tui), body);
    frame.render_widget(Paragraph::new(keys(&GLOBAL_KEYS)), footer);
    if tui.help() {
        draw_help(frame, body);
    }
}

/// The product mark, project and profiles on the left; the tab bar on the right.
fn draw_header(tui: &Tui, frame: &mut Frame<'_>, area: Rect) {
    let options = tui.options();
    let profiles = if options.profiles.is_empty() { "default".to_owned() } else { options.profiles.join(",") };
    let left = Line::from(vec![
        Span::styled("◆ vibranium ", theme::TITLE),
        Span::raw(options.project.clone()),
        Span::styled("  profile ", theme::DIM),
        Span::raw(profiles),
    ]);
    let bar = tab_bar(tui.position());
    let width = u16::try_from(bar.width()).unwrap_or(u16::MAX);
    let [left_area, bar_area] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)]).areas(area);
    frame.render_widget(Paragraph::new(left), left_area);
    frame.render_widget(Paragraph::new(bar), bar_area);
}

/// `1 Run  2 Explore …`, the chosen one highlighted.
fn tab_bar(chosen: usize) -> Line<'static> {
    let spans = TABS.iter().enumerate().flat_map(|(position, tab)| {
        let style = if position == chosen { theme::SELECTED } else { theme::DIM };
        [Span::styled(format!(" {} {} ", position + 1, tab.title), style), Span::raw(" ")]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// The chosen tab's pane: a titled border round what the tab shows.
fn pane(tui: &Tui) -> Paragraph<'static> {
    let tab = tui.current();
    let block =
        Block::bordered().border_style(theme::BORDER).title(Span::styled(format!(" {} ", tab.title), theme::TITLE));
    Paragraph::new(Span::styled(tab.about, theme::DIM)).block(block).wrap(Wrap { trim: true })
}

/// `key label` pairs, each key marked.
fn keys(pairs: &[(&'static str, &'static str)]) -> Line<'static> {
    let spans = pairs.iter().flat_map(|(key, label)| {
        [Span::raw(" "), Span::styled(format!(" {key} "), theme::KEY), Span::styled(format!(" {label}"), theme::DIM)]
    });
    Line::from(spans.collect::<Vec<_>>())
}

/// The help, centred over `area`.
fn draw_help(frame: &mut Frame<'_>, area: Rect) {
    let lines: Vec<Line<'static>> = HELP
        .iter()
        .map(|(key, does)| {
            Line::from(vec![Span::styled(format!("{key:<HELP_KEY_WIDTH$}"), theme::TITLE), Span::raw(*does)])
        })
        .collect();
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX).saturating_add(2);
    let [row] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [help] = Layout::horizontal([Constraint::Length(44)]).flex(Flex::Center).areas(row);
    let block = Block::bordered().border_style(theme::BORDER).title(Span::styled(" Help ", theme::TITLE));
    frame.render_widget(Clear, help);
    frame.render_widget(Paragraph::new(lines).block(block).style(theme::BASE), help);
}
