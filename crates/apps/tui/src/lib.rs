//! The terminal UI (`vc tui`): one screen per tab (Run, Explore, History, Load, Import), drawn
//! with `ratatui` over `crossterm`.
//!
//! [`Tui`] holds the state and draws it; it does no I/O, so a test drives it with key events
//! and renders it to a test backend. [`run`] owns the terminal: it reads keys on a blocking
//! task and redraws after each one, so the UI never waits on I/O.
//!
//! ```
//! use vc_tui::{Tui, TuiOptions};
//!
//! let tui = Tui::new(TuiOptions::new("shop", vec!["staging".to_owned()]));
//! assert_eq!(tui.tab(), "Run");
//! ```

mod app;
mod error;
mod tabs;
mod terminal;
mod theme;
mod view;

pub use app::{Tui, TuiOptions};
pub use error::TuiError;
pub use terminal::run;
