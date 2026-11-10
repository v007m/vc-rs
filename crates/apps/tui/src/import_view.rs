//! Draws the Import tab: the status line, the form, the drafted files and the chosen one's
//! diff.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use vc_engine::{Changes, FileChange};

use crate::explore_view::failed;
use crate::import_screen::{Form, ImportPhase, ImportScreen, KINDS};
use crate::theme::{self, clean};

/// The labels of the form's rows, and what each toggle asks for.
const ROWS: [(&str, &str); 6] = [
    ("importer", ""),
    ("source", ""),
    ("file id", ""),
    ("tests", "a happy-path test per api"),
    ("negative", "bad input the schema rejects"),
    ("auth", "a 401 test per secured api"),
];
/// The most drafted files the Draft pane lists before the diff.
const LISTED: u16 = 6;

/// Draws `screen` into `area`.
pub fn draw(screen: &ImportScreen, frame: &mut Frame<'_>, area: Rect) {
    let [status, body] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let [form, right] = Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).areas(body);
    frame.render_widget(Paragraph::new(status_line(screen)), status);
    draw_form(screen, frame, form);
    let (ImportPhase::Drafted(changes) | ImportPhase::Writing(changes)) = screen.phase() else {
        frame.render_widget(about_pane(screen.phase()), right);
        return;
    };
    let listed = u16::try_from(changes.files.len() + changes.titles.len()).unwrap_or(LISTED).min(LISTED) + 2;
    let [files, diff] = Layout::vertical([Constraint::Length(listed), Constraint::Fill(1)]).areas(right);
    frame.render_widget(files_pane(changes, screen.shown_file()), files);
    let shown = changes.files.get(screen.shown_file());
    frame.render_widget(diff_pane(shown), diff);
}

/// What is being typed, or where the import has got to.
fn status_line(screen: &ImportScreen) -> Line<'static> {
    if screen.editing().is_some() {
        let row = ROWS.get(screen.field()).map_or("", |(label, _)| label);
        return Line::styled(format!("typing the {row} · enter keeps it, esc cancels"), theme::WARN);
    }
    match screen.phase() {
        ImportPhase::Form => Line::styled("choose the importer and the source, then d drafts the import", theme::DIM),
        ImportPhase::Drafting(source) => Line::styled(format!("drafting the import of {source}…"), theme::INFO),
        ImportPhase::Drafted(changes) => Line::styled(drafted(changes), theme::INFO),
        ImportPhase::Writing(changes) => Line::styled(format!("writing {}…", files(changes.files.len())), theme::INFO),
        ImportPhase::Written(titles) => Line::styled(format!("wrote: {}", clean(&titles.join("; "))), theme::PASS),
        ImportPhase::Failed(reason) => failed(reason.lines().next().unwrap_or_default()),
    }
}

/// `2 files drafted · n shows the next diff, w writes them, x discards`.
fn drafted(changes: &Changes) -> String {
    if changes.is_empty() {
        return "nothing to import: the project has it all · x goes back".to_owned();
    }
    format!("{} drafted · n shows the next diff, w writes them, x discards", files(changes.files.len()))
}

/// `1 file`, `2 files`.
fn files(count: usize) -> String {
    if count == 1 { "1 file".to_owned() } else { format!("{count} files") }
}

/// The form's rows, the chosen one highlighted.
fn draw_form(screen: &ImportScreen, frame: &mut Frame<'_>, area: Rect) {
    let typed = screen.editing().map(|input| format!("{}▏", clean(input.text())));
    let rows = ROWS.iter().enumerate().map(|(row, (label, about))| {
        let typing = typed.as_ref().filter(|_| row == screen.field());
        let value =
            typing.map_or_else(|| value(screen.form(), row, about), |text| Span::styled(text.clone(), theme::WARN));
        ListItem::new(Line::from(vec![Span::styled(format!("{label:<9} "), theme::DIM), value]))
    });
    let list = List::new(rows).block(theme::pane("Import")).highlight_style(theme::SELECTED).highlight_symbol("▸ ");
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(Some(screen.field())));
}

/// What the form holds in `row`.
fn value(form: &Form, row: usize, about: &str) -> Span<'static> {
    match row {
        0 => Span::raw(format!("‹ {} ›", KINDS.get(form.kind).copied().unwrap_or_default())),
        1 if form.source.is_empty() => Span::styled("a URL or a project path", theme::DIM),
        1 => Span::raw(clean(&form.source)),
        2 if form.file_id.is_empty() => Span::styled("from the source's name", theme::DIM),
        2 => Span::raw(clean(&form.file_id)),
        _ => {
            let on = form.tests.get(row - 3).copied().unwrap_or_default();
            Span::styled(format!("[{}] {about}", if on { "x" } else { " " }), if on { theme::PASS } else { theme::DIM })
        }
    }
}

/// Before a draft: what `d` does, or why the import failed, with its help.
fn about_pane(phase: &ImportPhase) -> Paragraph<'static> {
    let lines: Vec<Line<'static>> = match phase {
        ImportPhase::Failed(reason) => reason.lines().map(|line| Line::raw(clean(line))).collect(),
        _ => vec![Line::styled(
            "d drafts the api and test files `vc import` would create or change, each shown as a diff; \
             nothing is written before w",
            theme::DIM,
        )],
    };
    Paragraph::new(lines).wrap(Wrap { trim: false }).block(theme::pane("Draft"))
}

/// The drafted files, new or changed, the one whose diff shows marked; then what the importer
/// could not carry over.
fn files_pane(changes: &Changes, shown: usize) -> Paragraph<'static> {
    let rows = changes.files.iter().enumerate().map(|(at, file)| {
        let (mark, style) = if at == shown { ("▸ ", theme::SELECTED) } else { ("  ", Style::new()) };
        let how = if file.before.is_none() { "new    " } else { "changed" };
        Line::from(vec![
            Span::raw(mark),
            Span::styled(how, theme::DIM),
            Span::styled(format!(" {}", clean(&file.path)), style),
        ])
    });
    let notes = changes.titles.iter().map(|title| Line::styled(format!("  {}", clean(title)), theme::DIM));
    Paragraph::new(rows.chain(notes).collect::<Vec<_>>()).block(theme::pane("Draft"))
}

/// The chosen file's diff, its added lines green and its removed ones red.
fn diff_pane(file: Option<&FileChange>) -> Paragraph<'static> {
    let diff = file.map(FileChange::diff).unwrap_or_default();
    let lines: Vec<Line<'static>> = diff.lines().map(|line| Line::styled(clean(line), diff_style(line))).collect();
    Paragraph::new(lines).block(theme::pane("Diff"))
}

/// The style of a line of a unified diff.
fn diff_style(line: &str) -> Style {
    match line.chars().next() {
        _ if line.starts_with("+++") || line.starts_with("---") => theme::DIM,
        Some('+') => theme::PASS,
        Some('-') => theme::FAIL,
        Some('@') => theme::INFO,
        _ => Style::new(),
    }
}
