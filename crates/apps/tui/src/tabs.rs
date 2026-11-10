//! The tabs, in the order the LLD lists them; the digit keys choose them by position. Each is
//! a [`Screen`]; a tab whose screen is not built yet says what it will show.

use crate::TuiOptions;
use crate::about::About;
use crate::history_screen::HistoryScreen;
use crate::run_screen::RunScreen;
use crate::screen::Screen;

/// Every tab's screen, for `options`.
pub fn screens(options: &TuiOptions) -> Vec<Box<dyn Screen>> {
    vec![
        Box::new(RunScreen::new(options.profiles.clone())),
        Box::new(About::new(
            "Explore",
            "The apis and tests of the project, edited in place and checked before saving.",
        )),
        Box::new(HistoryScreen::default()),
        Box::new(About::new("Load", "The live line of a load run: users, requests per second, latency and errors.")),
        Box::new(About::new("Import", "OpenAPI, curl, HAR, .http and Postman files drafted into api and test files.")),
    ]
}
