//! Opens the user's browser with the platform's opener, run through a `CommandRunner`.

use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{Browser, BrowserError, CommandRunner, CommandSpec};

/// The program that opens a URL in the default browser on this platform.
const OPENER: &str = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };

/// A [`Browser`] that runs `open` (macOS) or `xdg-open` with the URL as its one argument. It
/// opens only `http` and `https` URLs, so a hostile `authorizeUrl` cannot start a program or
/// read a file.
#[derive(Clone)]
pub struct SystemBrowser {
    runner: Arc<dyn CommandRunner>,
}

impl SystemBrowser {
    /// A browser started through `runner`.
    #[must_use]
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self { runner }
    }
}

impl std::fmt::Debug for SystemBrowser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemBrowser").field("opener", &OPENER).finish()
    }
}

/// Whether `url` is an `http` or `https` URL with no spaces or control characters.
fn openable(url: &str) -> bool {
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    web && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[async_trait]
impl Browser for SystemBrowser {
    async fn open(&self, url: &str) -> Result<(), BrowserError> {
        if !openable(url) {
            return Err(BrowserError::new("only http and https URLs open"));
        }
        let output = self.runner.run(&CommandSpec::new(OPENER, [url])).await;
        let output = output.map_err(|error| BrowserError::new(error.to_string()))?;
        if output.success() {
            return Ok(());
        }
        Err(BrowserError::new(format!("`{OPENER}` failed; open the sign-in URL by hand")))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::{Browser, CommandOutput};
    use vc_testkit::{FakeCommands, browser_suite};

    use super::{OPENER, SystemBrowser};

    #[tokio::test]
    async fn the_system_browser_meets_the_browser_contract() {
        let runner = FakeCommands::new(|spec| {
            assert_eq!((spec.program.as_str(), spec.args.len()), (OPENER, 1));
            Some(Ok(CommandOutput::new(Some(0), "")))
        });
        browser_suite(&SystemBrowser::new(Arc::new(runner.clone()))).await;
        assert_eq!(runner.calls(), 1, "only the http URL reached the opener");
    }

    #[tokio::test]
    async fn it_refuses_odd_urls_and_reports_a_failed_opener() {
        let failing = SystemBrowser::new(Arc::new(FakeCommands::new(|_| Some(Ok(CommandOutput::new(Some(3), ""))))));
        assert!(failing.open("HTTPS://id.test/a").await.unwrap_err().reason.contains("by hand"));
        assert!(failing.open("https://id.test/a b").await.is_err());
        assert!(failing.open("https://id.test/\n").await.is_err());
        assert!(failing.open("http:/").await.is_err());
        let missing = SystemBrowser::new(Arc::new(FakeCommands::new(|_| None)));
        assert!(missing.open("http://id.test/").await.unwrap_err().reason.contains(OPENER));
        assert!(format!("{missing:?}").contains(OPENER));
    }
}
