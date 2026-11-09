//! The tabs, in the order the LLD lists them; the digit keys choose them by position.

/// One tab: its title and what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tab {
    /// The title, in the tab bar and on the pane.
    pub title: &'static str,
    /// What the tab shows, until its screen is built.
    pub about: &'static str,
}

/// Every tab.
pub const TABS: [Tab; 5] = [
    Tab { title: "Run", about: "Live results of a run: each test, its lifecycle and the requests it sends." },
    Tab { title: "Explore", about: "The apis and tests of the project, edited in place and checked before saving." },
    Tab { title: "History", about: "Stored runs, pass-rate and duration trends, flaky tests, and two runs compared." },
    Tab { title: "Load", about: "The live line of a load run: users, requests per second, latency and errors." },
    Tab { title: "Import", about: "OpenAPI, curl, HAR, .http and Postman files drafted into api and test files." },
];
