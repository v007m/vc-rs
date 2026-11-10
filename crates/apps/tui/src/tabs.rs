//! The tabs, in the order the LLD lists them; the digit keys choose them by position. Each is
//! a [`Screen`].

use crate::TuiOptions;
use crate::explore_screen::ExploreScreen;
use crate::history_screen::HistoryScreen;
use crate::import_screen::ImportScreen;
use crate::load_screen::LoadScreen;
use crate::run_screen::RunScreen;
use crate::screen::Screen;

/// Every tab's screen, for `options`.
pub fn screens(options: &TuiOptions) -> Vec<Box<dyn Screen>> {
    vec![
        Box::new(RunScreen::new(options.profiles.clone())),
        Box::new(ExploreScreen::new(options.profiles.clone())),
        Box::new(HistoryScreen::default()),
        Box::new(LoadScreen::new(options.profiles.clone())),
        Box::new(ImportScreen::new(options.profiles.clone(), options.importers.clone())),
    ]
}
