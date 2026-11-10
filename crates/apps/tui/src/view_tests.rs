use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::{STOPPING, draw_stopping};
use crate::{Tui, TuiOptions};

/// The rows `tui` shows while quitting waits on a run, on a 120 by 10 terminal.
fn stopping(tui: &Tui) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(120, 10)).unwrap();
    terminal.draw(|frame| draw_stopping(tui, frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..10).map(|y| (0..120).map(|x| buffer[(x, y)].symbol()).collect::<String>().trim_end().to_owned()).collect()
}

#[test]
fn quitting_with_a_run_in_progress_says_it_is_being_stopped() {
    let rows = stopping(&Tui::new(TuiOptions::new("shop", Vec::new())));
    assert_eq!(rows.last().map(String::as_str), Some(STOPPING.trim_end()), "the footer says why it has not closed");
    assert!(rows.first().is_some_and(|row| row.contains("vibranium")), "the UI is still drawn above it");
}
