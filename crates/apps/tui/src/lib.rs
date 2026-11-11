//! The terminal UI (`vc tui`): one screen per tab (Run, Explore, History, Load, Import), drawn
//! with `ratatui` over `crossterm`.
//!
//! [`Tui`] holds the state and draws it; it does no I/O, so a test drives it with key events
//! and the engine's [`Update`]s and renders it to a test backend. A key that needs the engine
//! gives back an [`Ask`]. [`run`] owns the terminal and the engine: it reads keys on a
//! blocking task, carries out each ask on a task of its own and redraws after every key and
//! update, so the UI never waits on I/O.
//!
//! ```
//! use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
//! use vc_tui::{Ask, Tui, TuiOptions};
//!
//! let mut tui = Tui::new(TuiOptions::new("shop", vec!["staging".to_owned()]));
//! assert_eq!(tui.tab(), "Run");
//! let ask = tui.press(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
//! let Some(Ask::Run(request)) = ask else { panic!("`a` runs every test") };
//! assert_eq!(request.plan.profiles, ["staging"]);
//! ```

mod app;
mod driver;
mod driver_edit;
mod edit_session;
mod edit_view;
mod error;
mod explore_screen;
mod explore_view;
mod history_model;
mod history_screen;
mod history_view;
mod import_screen;
mod import_view;
mod input;
mod load_screen;
mod load_view;
mod run_model;
mod run_screen;
mod run_view;
mod screen;
mod tabs;
mod terminal;
mod text_area;
mod theme;
mod view;

pub use app::{Tui, TuiOptions};
pub use error::TuiError;
pub use screen::{Ask, Flight, Update};
pub use terminal::run;
