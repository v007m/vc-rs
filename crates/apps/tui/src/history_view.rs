//! Draws the History tab: the status line, the runs, the trends, the troubled tests and the
//! comparison.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Sparkline, Table, TableState};
use vc_engine::{RunRow, RunStatus, TestTrend, started};

use crate::history_model::{self, Comparison, checkout, mean, percent, seconds, took};
use crate::history_screen::{HistoryScreen, ReadState};
use crate::theme;

/// The runs' columns: number, start, checkout, pass rate, the counts and the time.
const COLUMNS: [&str; 9] = ["run", "started", "branch@commit", "pass", "✓", "✕", "⊘", "↻", "time"];
/// The rate a pass-rate bar is full at: 100.0%, in tenths of a percent.
const FULL: u64 = 1_000;

/// Draws `screen` into `area`.
pub fn draw(screen: &HistoryScreen, frame: &mut Frame<'_>, area: Rect) {
    let [status, runs, below] =
        Layout::vertical([Constraint::Length(1), Constraint::Percentage(45), Constraint::Fill(1)]).areas(area);
    let [left, compare] = Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]).areas(below);
    let [trends, troubled] = Layout::vertical([Constraint::Length(4), Constraint::Fill(1)]).areas(left);
    frame.render_widget(Paragraph::new(status_line(screen)), status);
    draw_runs(screen, frame, runs);
    draw_trends(screen.runs(), frame, trends);
    frame.render_widget(troubled_pane(screen.trends()), troubled);
    frame.render_widget(compare_pane(screen), compare);
}

/// What has been read, or why it could not be.
fn status_line(screen: &HistoryScreen) -> Line<'static> {
    let mut spans = match screen.reading() {
        ReadState::Unread | ReadState::Reading => vec![Span::styled("reading the history…", theme::INFO)],
        ReadState::Failed(reason) => vec![Span::styled("error ", theme::FAIL), Span::raw(reason.clone())],
        ReadState::Read if screen.runs().is_empty() => {
            vec![Span::styled("no run is stored yet · a on the Run tab runs the tests", theme::DIM)]
        }
        ReadState::Read => vec![Span::styled(format!("{} stored runs, newest first", screen.runs().len()), theme::DIM)],
    };
    if screen.stale() && *screen.reading() == ReadState::Read {
        spans.push(Span::styled("  a run has finished since: r reloads", theme::WARN));
    }
    Line::from(spans)
}

fn draw_runs(screen: &HistoryScreen, frame: &mut Frame<'_>, area: Rect) {
    let widths = [
        Constraint::Length(6),
        Constraint::Length(17),
        Constraint::Fill(1),
        Constraint::Length(6),
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Length(4),
        Constraint::Length(8),
    ];
    let header = Row::new(COLUMNS).style(theme::DIM);
    let table = Table::new(screen.runs().iter().map(run_row), widths)
        .header(header)
        .block(theme::pane("Runs"))
        .row_highlight_style(theme::SELECTED)
        .highlight_symbol("▸ ");
    let chosen = (!screen.runs().is_empty()).then_some(screen.cursor());
    frame.render_stateful_widget(table, area, &mut TableState::default().with_selected(chosen));
}

/// `#482  2026-11-05 09:30Z  main@a1c9e4f  96.1%  396  6  4  3  48.2s`.
fn run_row(run: &RunRow) -> Row<'static> {
    let totals = &run.totals;
    let counts = [(totals.passed, theme::PASS), (totals.failed, theme::FAIL), (totals.blocked, theme::WARN)];
    let mut cells = vec![
        Cell::from(format!("#{}", run.id.get())),
        Cell::from(started(run.start.started_ms)),
        Cell::from(checkout(run)),
        Cell::from(Span::styled(percent(totals), ending(run.status))),
    ];
    cells
        .extend(counts.into_iter().chain([(totals.flaky, theme::WARN)]).map(|(count, style)| count_cell(count, style)));
    cells.push(Cell::from(took(run).map_or_else(|| "running".to_owned(), seconds)));
    Row::new(cells)
}

/// A count, dimmed when it is zero.
fn count_cell(count: usize, style: Style) -> Cell<'static> {
    Cell::from(Span::styled(count.to_string(), if count == 0 { theme::DIM } else { style }))
}

/// The colour of how a run ended.
const fn ending(status: Option<RunStatus>) -> Style {
    match status {
        Some(RunStatus::Passed) => theme::PASS,
        Some(RunStatus::Failed) => theme::FAIL,
        Some(RunStatus::Cancelled) => theme::WARN,
        None => theme::INFO,
    }
}

/// The pass rate and the time of each run, oldest first, one bar each.
fn draw_trends(runs: &[RunRow], frame: &mut Frame<'_>, area: Rect) {
    let block = theme::pane(&format!("Trends · last {} runs", runs.len()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [rates, times] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(inner);
    let rate = |run: &RunRow| history_model::pass_rate(&run.totals).and_then(|rate| u64::try_from(rate).ok());
    let rate_bars: Vec<u64> = runs.iter().rev().map(|run| rate(run).unwrap_or(0)).collect();
    let time_bars: Vec<u64> = runs.iter().rev().map(|run| took(run).unwrap_or(0)).collect();
    draw_bars(frame, rates, ("pass rate", Sparkline::default().data(rate_bars).max(FULL).style(theme::PASS)));
    draw_bars(frame, times, ("duration", Sparkline::default().data(time_bars).style(theme::INFO)));
}

/// A label, then its bars.
fn draw_bars(frame: &mut Frame<'_>, area: Rect, (label, bars): (&str, Sparkline<'_>)) {
    let [label_area, bars_area] = Layout::horizontal([Constraint::Length(11), Constraint::Fill(1)]).areas(area);
    frame.render_widget(Span::styled(label.to_owned(), theme::DIM), label_area);
    frame.render_widget(bars, bars_area);
}

/// The tests that failed or flaked over the trend's runs, most often first.
fn troubled_pane(trends: &[TestTrend]) -> Paragraph<'static> {
    let troubled = history_model::troubled(trends);
    let mut lines: Vec<Line<'static>> = troubled.iter().map(|trend| troubled_line(trend)).collect();
    if lines.is_empty() && !trends.is_empty() {
        lines.push(Line::styled("No test failed or flaked in these runs.", theme::DIM));
    }
    Paragraph::new(lines).block(theme::pane("Flaky and failing"))
}

/// `↻ 3  ✕ 1  1.2s  catalog.search#fuzzyQueries`.
fn troubled_line(trend: &TestTrend) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("↻ {}", trend.flaky), if trend.flaky == 0 { theme::DIM } else { theme::WARN }),
        Span::styled(format!("  ✕ {}", trend.failed), if trend.failed == 0 { theme::DIM } else { theme::FAIL }),
        Span::styled(format!("  {}  ", mean(trend.mean_ms)), theme::DIM),
        Span::raw(theme::clean(&trend.test_id)),
    ])
}

/// The chosen run against the one before it, or how to get there.
fn compare_pane(screen: &HistoryScreen) -> Paragraph<'static> {
    if let Some((before, after)) = screen.comparing() {
        let reading = Line::styled(format!("reading #{before} and #{after}…"), theme::INFO);
        return Paragraph::new(reading).block(theme::pane("Compare"));
    }
    if let Some(comparison) = screen.compared() {
        let title = format!("Compare #{} → #{}", comparison.before, comparison.after);
        return Paragraph::new(compared(comparison)).block(theme::pane(&title));
    }
    Paragraph::new(Line::styled(hint(screen), theme::DIM)).block(theme::pane("Compare"))
}

/// What `enter` would compare.
fn hint(screen: &HistoryScreen) -> String {
    let runs = screen.runs();
    match (runs.get(screen.cursor()), runs.get(screen.cursor() + 1)) {
        (Some(after), Some(before)) => format!("enter compares #{} with #{}", after.id.get(), before.id.get()),
        (Some(only), None) => format!("#{} is the oldest run here: nothing to compare it with", only.id.get()),
        _ => String::new(),
    }
}

/// The comparison's sections that have tests in them.
fn compared(comparison: &Comparison) -> Vec<Line<'static>> {
    if comparison.unchanged() {
        let same = format!("#{} matches #{}: nothing failing, fixed or slower", comparison.after, comparison.before);
        return vec![Line::styled(same, theme::PASS)];
    }
    let slower = comparison
        .slower
        .iter()
        .map(|slow| format!("{}  {} → {}", slow.test, mean(slow.before_ms), mean(slow.after_ms)));
    let mut lines = Vec::new();
    section(&mut lines, ("Newly failing", theme::FAIL), &comparison.newly_failing);
    section(&mut lines, ("Newly blocked", theme::WARN), &comparison.newly_blocked);
    section(&mut lines, ("Fixed", theme::PASS), &comparison.fixed);
    section(&mut lines, ("Slower by >20%", theme::WARN), &slower.collect::<Vec<_>>());
    section(&mut lines, ("Still failing", theme::DIM), &comparison.still_failing);
    lines
}

/// `Newly failing · 2` and its tests, when it has any.
fn section(lines: &mut Vec<Line<'static>>, (title, style): (&str, Style), tests: &[String]) {
    if tests.is_empty() {
        return;
    }
    lines.push(Line::styled(format!("{title} · {}", tests.len()), style));
    lines.extend(tests.iter().map(|test| Line::raw(format!("  {test}"))));
}
