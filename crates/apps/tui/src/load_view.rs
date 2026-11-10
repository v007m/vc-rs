//! Draws the Load tab: the status line, the scenarios, the live line with its bars, and the
//! seconds so far or the run's summary.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Sparkline};
use vc_engine::{Listed, LoadEnding, LoadReport};

use crate::history_screen::ReadState;
use crate::history_view::draw_bars;
use crate::load_screen::{LoadPhase, LoadScreen, Tick};
use crate::theme::{self, clean};

/// The height of the Live pane, borders included: the line, then the rps and p95 bars.
const LIVE_HEIGHT: u16 = 5;

/// Draws `screen` into `area`.
pub fn draw(screen: &LoadScreen, frame: &mut Frame<'_>, area: Rect) {
    let [status, body] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    let [scenarios, right] = Layout::horizontal([Constraint::Percentage(35), Constraint::Fill(1)]).areas(body);
    let [live, below] = Layout::vertical([Constraint::Length(LIVE_HEIGHT), Constraint::Fill(1)]).areas(right);
    frame.render_widget(Paragraph::new(status_line(screen)), status);
    draw_scenarios(screen, frame, scenarios);
    draw_live(screen, frame, live);
    frame.render_widget(below_pane(screen, below.height), below);
}

/// What has been read, or where the run has got to.
fn status_line(screen: &LoadScreen) -> Line<'static> {
    match (screen.reading(), screen.phase()) {
        (_, LoadPhase::Failed(reason)) | (ReadState::Failed(reason), LoadPhase::Idle) => {
            Line::from(vec![Span::styled("error ", theme::FAIL), Span::raw(reason.clone())])
        }
        (ReadState::Unread | ReadState::Reading, LoadPhase::Idle) => Line::styled("reading the scenarios…", theme::INFO),
        (_, LoadPhase::Idle) => Line::styled(idle(screen.scenarios()), theme::DIM),
        (_, LoadPhase::Starting(scenario)) => Line::styled(format!("starting {}…", clean(scenario)), theme::INFO),
        (_, LoadPhase::Running(scenario)) => {
            let second = screen.ticks().back().map_or(0, |tick| tick.second);
            Line::styled(format!("running {} · {second}s · x stops it", clean(scenario)), theme::INFO)
        }
        (_, LoadPhase::Done(report)) => ended(report),
    }
}

/// What `enter` does, or that there is nothing to run.
fn idle(scenarios: &[Listed]) -> String {
    match scenarios.len() {
        0 => "no load scenario in the project: a *.load.yaml file adds one".to_owned(),
        1 => "1 scenario · enter runs it, x stops it".to_owned(),
        count => format!("{count} scenarios · enter runs the chosen one, x stops it"),
    }
}

/// `users.peak finished after 30 s · exit 0 · run #12`.
fn ended(report: &LoadReport) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!("{} {}", clean(&report.scenario), report.ending.as_str()), ending(report)),
        Span::raw(format!(" after {} s · exit {}", report.seconds, report.exit)),
    ];
    spans.extend(report.run_id.map(|run| Span::styled(format!(" · run #{run}"), theme::DIM)));
    Line::from(spans)
}

/// The colour of how a run ended: within its thresholds, breached or aborted, or stopped short.
const fn ending(report: &LoadReport) -> Style {
    match (report.ending, report.exit) {
        (_, 0) => theme::PASS,
        (LoadEnding::Aborted, _) | (_, 3) => theme::FAIL,
        _ => theme::WARN,
    }
}

fn draw_scenarios(screen: &LoadScreen, frame: &mut Frame<'_>, area: Rect) {
    let items = screen.scenarios().iter().map(|scenario| {
        let name = scenario.name.as_deref().map(|name| Span::styled(format!("  {}", clean(name)), theme::DIM));
        ListItem::new(Line::from([Span::raw(clean(&scenario.id))].into_iter().chain(name).collect::<Vec<_>>()))
    });
    let list =
        List::new(items).block(theme::pane("Scenarios")).highlight_style(theme::SELECTED).highlight_symbol("▸ ");
    let chosen = (!screen.scenarios().is_empty()).then_some(screen.cursor());
    frame.render_stateful_widget(list, area, &mut ListState::default().with_selected(chosen));
}

/// The newest second as `vc load` prints it, and the rps and p95 of each second kept.
fn draw_live(screen: &LoadScreen, frame: &mut Frame<'_>, area: Rect) {
    let block = theme::pane("Live");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [line, rps, p95] = Layout::vertical([Constraint::Length(1); 3]).areas(inner);
    let newest = screen.ticks().back().map_or_else(
        || Line::styled("each second of a run shows here, as vc load prints it", theme::DIM),
        tick_line,
    );
    frame.render_widget(Paragraph::new(newest), line);
    let rates: Vec<u64> = screen.ticks().iter().map(|tick| tick.rps).collect();
    let slow: Vec<u64> = screen.ticks().iter().map(|tick| u64::try_from(tick.latencies[1]).unwrap_or(u64::MAX)).collect();
    draw_bars(frame, rps, ("rps", Sparkline::default().data(rates).style(theme::INFO)));
    draw_bars(frame, p95, ("p95 ms", Sparkline::default().data(slow).style(theme::WARN)));
}

/// `   12s  50 users  412 rps  p50 31 ms  p95 88 ms  p99 140 ms  0 errors  breaching: p95`.
fn tick_line(tick: &Tick) -> Line<'static> {
    let [p50, p95, p99] = tick.latencies;
    let mut spans = vec![
        Span::styled(format!("{:>5}s", tick.second), theme::DIM),
        Span::raw(format!("  {} users  {} rps  ", tick.users, tick.rps)),
        Span::styled(format!("p50 {p50} ms  p95 {p95} ms  p99 {p99} ms  "), theme::INFO),
        Span::styled(format!("{} errors", tick.errors), if tick.errors == 0 { theme::DIM } else { theme::FAIL }),
    ];
    if !tick.breaching.is_empty() {
        spans.push(Span::styled(format!("  breaching: {}", tick.breaching.join(", ")), theme::WARN));
    }
    Line::from(spans)
}

/// The run's summary once it is over; until then the seconds kept, newest first.
fn below_pane(screen: &LoadScreen, height: u16) -> Paragraph<'static> {
    if let LoadPhase::Done(report) = screen.phase() {
        let lines: Vec<Line<'static>> = report.text().lines().map(|line| Line::raw(clean(line))).collect();
        return Paragraph::new(lines).block(theme::pane("Summary"));
    }
    let shown = usize::from(height.saturating_sub(2));
    let lines: Vec<Line<'static>> = screen.ticks().iter().rev().take(shown).map(tick_line).collect();
    Paragraph::new(lines).block(theme::pane("Seconds"))
}
