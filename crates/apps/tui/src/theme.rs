//! The colours of the mockups' terminal screens.

use ratatui::style::{Color, Modifier, Style};

const BACKGROUND: Color = Color::Rgb(0x0F, 0x11, 0x15);
const TEXT: Color = Color::Rgb(0xD7, 0xDA, 0xE0);
const LINE: Color = Color::Rgb(0x2C, 0x31, 0x3C);
const ACCENT: Color = Color::Rgb(0xA9, 0x9B, 0xFF);
const MUTED: Color = Color::Rgb(0x8B, 0x92, 0xA0);
const HIGHLIGHT: Color = Color::Rgb(0x25, 0x2A, 0x36);

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
