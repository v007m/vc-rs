//! The colours of the mockups' terminal screens.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Block;
use vc_engine::TestStatus;

const BACKGROUND: Color = Color::Rgb(0x0F, 0x11, 0x15);
const TEXT: Color = Color::Rgb(0xD7, 0xDA, 0xE0);
const LINE: Color = Color::Rgb(0x2C, 0x31, 0x3C);
const ACCENT: Color = Color::Rgb(0xA9, 0x9B, 0xFF);
const MUTED: Color = Color::Rgb(0x8B, 0x92, 0xA0);
const HIGHLIGHT: Color = Color::Rgb(0x25, 0x2A, 0x36);
const GREEN: Color = Color::Rgb(0x4F, 0xC2, 0x7A);
const RED: Color = Color::Rgb(0xF0, 0x6A, 0x5D);
const AMBER: Color = Color::Rgb(0xE5, 0xA1, 0x3A);
const BLUE: Color = Color::Rgb(0x6A, 0xA2, 0xF5);

/// The screen's background and text.
pub const BASE: Style = Style::new().fg(TEXT).bg(BACKGROUND);
/// A pane's border.
pub const BORDER: Style = Style::new().fg(LINE);
/// A pane's title and the product mark.
pub const TITLE: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
/// Secondary text.
pub const DIM: Style = Style::new().fg(MUTED);
/// The chosen tab.
pub const SELECTED: Style = Style::new().fg(TEXT).bg(HIGHLIGHT).add_modifier(Modifier::BOLD);
/// A key in the footer or the help.
pub const KEY: Style = Style::new().fg(TEXT).bg(LINE);
/// Passed.
pub const PASS: Style = Style::new().fg(GREEN);
/// Failed, or an error.
pub const FAIL: Style = Style::new().fg(RED);
/// Blocked or flaky.
pub const WARN: Style = Style::new().fg(AMBER);
/// Running, or a request.
pub const INFO: Style = Style::new().fg(BLUE);

/// A test status's mark and style, as the mockups show it: `✓` passed, `✕` failed, `⊘`
/// blocked, `↻` flaky, `–` skipped, `⚠` allowed to fail (D248).
pub const fn mark(status: TestStatus) -> (&'static str, Style) {
    match status {
        TestStatus::Passed => ("✓", PASS),
        TestStatus::Failed => ("✕", FAIL),
        TestStatus::Blocked => ("⊘", WARN),
        TestStatus::Flaky => ("↻", WARN),
        TestStatus::Skipped => ("–", DIM),
        TestStatus::AllowedFailure => ("⚠", WARN),
    }
}

/// A pane: a border with `title` on it.
pub fn pane(title: &str) -> Block<'static> {
    Block::bordered().border_style(BORDER).title(Span::styled(format!(" {title} "), TITLE))
}

/// `text` with its control characters replaced, so text from a response or a file cannot
/// move the cursor or change the terminal.
pub fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { '·' } else { c }).collect()
}

/// The first line of `text`, cleaned: a failure's headline, without its `help:` lines.
pub fn first_line(text: &str) -> String {
    clean(text.lines().next().unwrap_or_default())
}

/// `  warning: <the first notice> (+2 more)`, as the CLI prints a report's notices: problems
/// that did not stop the engine. Nothing when there are none.
pub fn notices(notices: &[String]) -> Option<Span<'static>> {
    let first = notices.first()?;
    let more = notices.len() - 1;
    let more = if more > 0 { format!(" (+{more} more)") } else { String::new() };
    Some(Span::styled(format!("  warning: {}{more}", first_line(first)), WARN))
}
