//! Draws the Run tab: the status line, the tests by file, the chosen test and the requests.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use vc_engine::{RunReport, TestStatus, Totals};

use crate::run_model::{CheckLine, FileGroup, Finished, Phase, RequestLine, RunModel};
use crate::run_screen::RunScreen;
use crate::theme::{self, clean};

/// The height of the Requests pane, borders included.
const REQUESTS_HEIGHT: u16 = 8;

/// Draws `screen` into `area`.
pub fn draw(screen: &RunScreen, frame: &mut Frame<'_>, area: Rect) {
    let [status, body] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let [tests, right] = Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).areas(body);
    let [detail, requests] = Layout::vertical([Constraint::Fill(1), Constraint::Length(REQUESTS_HEIGHT)]).areas(right);
    frame.render_widget(Paragraph::new(status_line(screen.model())), status);
    draw_tests(screen, frame, tests);
    frame.render_widget(detail_pane(screen.selected()), detail);
    frame.render_widget(requests_pane(screen.model(), requests.height), requests);
}

/// `running 2/3  ✓ 1  ✕ 1 …`, or what the run is waiting for or how it ended, or why the run
/// asked for did not start.
fn status_line(model: &RunModel) -> Line<'static> {
    let mut spans = model.refused.as_deref().map_or_else(|| phase_spans(model), failed);
    if model.missed > 0 {
        spans.push(Span::styled(format!("  missed {} events; `vc history` has them", model.missed), theme::WARN));
    }
    Line::from(spans)
}

/// `error …`.
fn failed(reason: &str) -> Vec<Span<'static>> {
    vec![Span::styled("error ", theme::FAIL), Span::raw(reason.to_owned())]
}

/// Where the run is, `stopping` once `x` asked it to stop.
fn phase_spans(model: &RunModel) -> Vec<Span<'static>> {
    match &model.phase {
        Phase::Idle => {
            vec![Span::styled("no run yet · a runs every test, R reruns the last run's failures", theme::DIM)]
        }
        Phase::Starting if model.stopping => vec![Span::styled("stopping…", theme::WARN)],
        Phase::Starting => vec![Span::styled("starting…", theme::INFO)],
        Phase::Running if model.stopping => progress("stopping…", theme::WARN, model),
        Phase::Running => progress("running", theme::INFO, model),
        Phase::Done(report) => {
            let (word, style) = ending(report);
            let numbered = progress(word, style, model).into_iter().chain(report.run.map(run_number));
            numbered.chain(theme::notices(&report.notices)).collect()
        }
        Phase::Failed(reason) => failed(reason),
    }
}

fn progress(word: &'static str, style: Style, model: &RunModel) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(word, style), Span::raw(format!(" {}/{} ", model.finished(), model.planned))];
    spans.extend(counts(&model.totals, true));
    spans.push(Span::styled(format!("  {} requests", model.sent), theme::DIM));
    spans
}

/// ` ✓ 2  ✕ 1  ⊘ 0  ↻ 0  – 1`, without the zeros unless `zeros`.
fn counts(totals: &Totals, zeros: bool) -> Vec<Span<'static>> {
    let counted = [
        (TestStatus::Passed, totals.passed),
        (TestStatus::Failed, totals.failed),
        (TestStatus::Blocked, totals.blocked),
        (TestStatus::Flaky, totals.flaky),
        (TestStatus::Skipped, totals.skipped),
    ];
    counted
        .into_iter()
        .filter(|(_, count)| zeros || *count > 0)
        .map(|(status, count)| {
            let (mark, style) = theme::mark(status);
            Span::styled(format!(" {mark} {count} "), style)
        })
        .collect()
}

/// How a finished run ended, by its exit code.
const fn ending(report: &RunReport) -> (&'static str, Style) {
    match report.exit {
        0 => ("passed", theme::PASS),
        2 => ("nothing ran", theme::DIM),
        130 => ("cancelled", theme::WARN),
        _ => ("failed", theme::FAIL),
    }
}

fn run_number(run: u64) -> Span<'static> {
    Span::styled(format!("  run #{run}"), theme::DIM)
}

/// A row of the Tests pane: a file, or a test of it listed.
enum Row<'a> {
    File(&'a FileGroup),
    Test(&'a Finished),
}

/// The Tests pane's rows, each file then its tests listed, and the chosen test's position.
fn rows(screen: &RunScreen) -> (Vec<Row<'_>>, Option<usize>) {
    let (mut rows, mut chosen, mut position) = (Vec::new(), None, 0);
    for group in &screen.model().files {
        let mut listed = group.tests.iter().filter(|test| screen.shows(test)).peekable();
        if listed.peek().is_none() {
            continue;
        }
        rows.push(Row::File(group));
        for test in listed {
            chosen = chosen.or_else(|| (position == screen.cursor()).then_some(rows.len()));
            rows.push(Row::Test(test));
            position += 1;
        }
    }
    (rows, chosen)
}

/// The first of the rows shown when `room` rows fit: the chosen one last when it would be
/// below them, as the list itself scrolls.
fn first_shown(chosen: Option<usize>, room: usize) -> usize {
    chosen.map_or(0, |chosen| chosen.saturating_add(1).saturating_sub(room))
}

/// The tests by file, the chosen one highlighted and kept in view; only the rows in view are
/// drawn.
fn draw_tests(screen: &RunScreen, frame: &mut Frame<'_>, area: Rect) {
    let (rows, chosen) = rows(screen);
    let room = usize::from(area.height.saturating_sub(2)).max(1);
    let first = first_shown(chosen, room);
    let items = rows.iter().skip(first).take(room).map(|row| match row {
        Row::File(group) => ListItem::new(file_row(group)),
        Row::Test(test) => ListItem::new(test_row(test)),
    });
    let files = screen.model().files.len();
    let only = if screen.failures_only() { " · failures only" } else { "" };
    let list =
        List::new(items).block(theme::pane(&format!("Tests · {files} files{only}"))).highlight_style(theme::SELECTED);
    let selected = chosen.map(|chosen| chosen.saturating_sub(first));
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(selected));
}

/// `▾ users.crud  1✕ 2✓`.
fn file_row(group: &FileGroup) -> Line<'static> {
    let mut totals = Totals::default();
    for test in &group.tests {
        totals.add(test.status);
    }
    let head = Span::styled(format!("▾ {} ", clean(&group.file)), theme::TITLE);
    Line::from(std::iter::once(head).chain(counts(&totals, false)).collect::<Vec<_>>())
}

/// `  ✕ createsUser ×2`.
fn test_row(test: &Finished) -> Line<'static> {
    let (mark, style) = theme::mark(test.status);
    let mut spans = vec![Span::raw("  "), Span::styled(mark, style), Span::raw(format!(" {}", clean(&test.label)))];
    if test.attempts > 1 {
        spans.push(Span::styled(format!(" ×{}", test.attempts), theme::DIM));
    }
    Line::from(spans)
}

/// The chosen test: its ID, where it is written, why it ended so, and its checks.
fn detail_pane(test: Option<&Finished>) -> Paragraph<'static> {
    let Some(test) = test else {
        return Paragraph::new(Span::styled("Choose a test with ↑↓.", theme::DIM)).block(theme::pane("Test"));
    };
    let (mark, style) = theme::mark(test.status);
    let mut lines = vec![Line::from(vec![Span::styled(format!("{mark} "), style), Span::raw(clean(&test.test))])];
    lines.extend(test.location.as_deref().map(|at| Line::styled(clean(at), theme::DIM)));
    let reason_style = if test.status == TestStatus::Skipped { theme::DIM } else { theme::FAIL };
    lines.extend(test.reason.clone().map(|reason| Line::styled(reason, reason_style)));
    if !test.checks.is_empty() {
        lines.push(Line::default());
    }
    lines.extend(test.checks.iter().flat_map(check_lines));
    Paragraph::new(lines).block(theme::pane(&clean(&test.label))).wrap(Wrap { trim: false })
}

/// `✕ status :5`, then why it failed.
fn check_lines(check: &CheckLine) -> Vec<Line<'static>> {
    let (mark, style) = if check.passed { ("✓", theme::PASS) } else { ("✕", theme::FAIL) };
    let line = check.line.map(|line| format!(" :{line}")).unwrap_or_default();
    let head = Line::from(vec![
        Span::styled(format!("{mark} "), style),
        Span::raw(check.label.clone()),
        Span::styled(line, theme::DIM),
    ]);
    let why = check.detail.clone().map(|detail| Line::styled(format!("    {detail}"), theme::FAIL));
    std::iter::once(head).chain(why).collect()
}

/// The latest requests that fit in `height`, newest last.
fn requests_pane(model: &RunModel, height: u16) -> Paragraph<'static> {
    let room = usize::from(height.saturating_sub(2));
    let skip = model.requests.len().saturating_sub(room);
    let lines: Vec<Line<'static>> = model.requests.iter().skip(skip).map(request_line).collect();
    Paragraph::new(lines).block(theme::pane("Requests"))
}

/// `GET /users  200  0ms  call users#list`.
fn request_line(request: &RequestLine) -> Line<'static> {
    let style = match request.status {
        200..=399 => theme::PASS,
        _ => theme::FAIL,
    };
    Line::from(vec![
        Span::raw(format!("{}  ", request.request)),
        Span::styled(request.status.to_string(), style),
        Span::styled(format!("  {}ms  {}", request.millis, request.by), theme::DIM),
    ])
}

#[cfg(test)]
#[path = "run_view_tests.rs"]
mod tests;
