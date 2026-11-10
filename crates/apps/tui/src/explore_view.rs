//! Draws the Explore tab: the status line or the reason being typed, the project by file, the
//! chosen item's details and its explanation.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use vc_engine::Listed;

use crate::explore_screen::{ExploreScreen, Note, is_api};
use crate::history_screen::ReadState;
use crate::input::Input;
use crate::theme::{self, clean};

/// The height of the Details pane, borders included.
const DETAILS_HEIGHT: u16 = 7;

/// Draws `screen` into `area`.
pub fn draw(screen: &ExploreScreen, frame: &mut Frame<'_>, area: Rect) {
    let [status, body] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let [project, right] = Layout::horizontal([Constraint::Percentage(45), Constraint::Fill(1)]).areas(body);
    let [details, explain] = Layout::vertical([Constraint::Length(DETAILS_HEIGHT), Constraint::Fill(1)]).areas(right);
    frame.render_widget(Paragraph::new(status_line(screen)), status);
    draw_project(screen, frame, project);
    frame.render_widget(details_pane(screen.chosen()), details);
    frame.render_widget(explain_pane(screen), explain);
}

/// The reason being typed, or what has been read, or what the last key came to.
fn status_line(screen: &ExploreScreen) -> Line<'static> {
    if let (Some(reason), Some(chosen)) = (screen.reason(), screen.chosen()) {
        return prompt(reason, &chosen.id, screen.note());
    }
    match (screen.reading(), screen.note()) {
        (ReadState::Unread | ReadState::Reading, _) => Line::styled("reading the project…", theme::INFO),
        (ReadState::Failed(reason), _) | (ReadState::Read, Some(Note::Failed(reason))) => failed(reason),
        (ReadState::Read, Some(Note::Said(said))) => Line::styled(clean(said), theme::INFO),
        (ReadState::Read, None) => Line::styled(summary(screen.items()), theme::DIM),
    }
}

/// `error <reason>`.
pub fn failed(reason: &str) -> Line<'static> {
    Line::from(vec![Span::styled("error ", theme::FAIL), Span::raw(clean(reason))])
}

/// `skip users.crud#listsUsers because: flaky upstream▏  enter skips · esc cancels`.
fn prompt(reason: &Input, id: &str, note: Option<&Note>) -> Line<'static> {
    let hint = match note {
        Some(Note::Failed(why)) => Span::styled(format!("  {}", clean(why)), theme::FAIL),
        _ => Span::styled("  enter skips · esc cancels", theme::DIM),
    };
    Line::from(vec![
        Span::styled(format!("skip {} because: ", clean(id)), theme::WARN),
        Span::raw(clean(reason.text())),
        Span::styled("▏", theme::TITLE),
        hint,
    ])
}

/// `5 tests and 2 apis · enter explains, t skips or unskips`.
fn summary(items: &[Listed]) -> String {
    let apis = items.iter().filter(|item| is_api(item)).count();
    format!("{} tests and {apis} apis · enter explains, t skips or unskips", items.len() - apis)
}

/// The items under their files' paths, the chosen one highlighted.
fn draw_project(screen: &ExploreScreen, frame: &mut Frame<'_>, area: Rect) {
    let mut lines = Vec::new();
    let mut chosen = None;
    let mut file = "";
    for (position, item) in screen.items().iter().enumerate() {
        if item.path != file {
            file = &item.path;
            lines.push(ListItem::new(Line::styled(clean(file), theme::DIM)));
        }
        if position == screen.cursor() {
            chosen = Some(lines.len());
        }
        lines.push(ListItem::new(item_line(item)));
    }
    let list = List::new(lines).block(theme::pane("Project")).highlight_style(theme::SELECTED).highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(chosen));
}

/// `  deletesUser  Deletes a user  – skipped`: the ID within its file, the name and the skip.
fn item_line(item: &Listed) -> Line<'static> {
    let short = item.id.split_once('#').map_or(item.id.as_str(), |(_, short)| short);
    let mut spans = vec![Span::raw(format!("  {}", clean(short)))];
    if let Some(name) = &item.name {
        spans.push(Span::styled(format!("  {}", clean(name)), theme::DIM));
    }
    if item.skipped {
        spans.push(Span::styled("  – skipped", theme::WARN));
    }
    Line::from(spans)
}

/// The chosen item's ID, name, file, tags, and for a test whether it runs.
fn details_pane(chosen: Option<&Listed>) -> Paragraph<'static> {
    let Some(item) = chosen else {
        return Paragraph::new(Line::styled("nothing to show yet", theme::DIM)).block(theme::pane("Details"));
    };
    let none = || "—".to_owned();
    let mut lines = vec![
        field("id", clean(&item.id), Style::new()),
        field("name", item.name.as_deref().map_or_else(none, clean), Style::new()),
        field("file", clean(&item.path), Style::new()),
        field("tags", if item.tags.is_empty() { none() } else { clean(&item.tags.join(", ")) }, Style::new()),
    ];
    if !is_api(item) {
        let (runs, style) = if item.skipped { ("skipped · t unskips", theme::WARN) } else { ("runs", theme::PASS) };
        lines.push(field("status", runs.to_owned(), style));
    }
    Paragraph::new(lines).block(theme::pane("Details"))
}

/// `name    List users`: a label, then its value in `style`.
fn field(label: &str, value: String, style: Style) -> Line<'static> {
    Line::from(vec![Span::styled(format!("{label:<8}"), theme::DIM), Span::styled(value, style)])
}

/// How a run sees the chosen item, once explained, or how to ask.
fn explain_pane(screen: &ExploreScreen) -> Paragraph<'static> {
    let block = theme::pane("Explain");
    if let Some(id) = screen.explaining() {
        return Paragraph::new(Line::styled(format!("explaining {}…", clean(id)), theme::INFO)).block(block);
    }
    match (screen.explained(), screen.chosen()) {
        (Some((id, text)), Some(chosen)) if *id == chosen.id => {
            let lines: Vec<Line<'static>> = text.lines().map(|line| Line::raw(clean(line))).collect();
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(block)
        }
        (_, Some(chosen)) => {
            let hint = format!("enter shows how a run sees {}: its URL, vars and checks", clean(&chosen.id));
            Paragraph::new(Line::styled(hint, theme::DIM)).wrap(Wrap { trim: true }).block(block)
        }
        (_, None) => Paragraph::new("").block(block),
    }
}
