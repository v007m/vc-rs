//! The flags `vc plan`, `vc run` and `vc list` share: the selectors, and the profiles and
//! settings a plan is made with.

use std::path::Path;
use std::time::Duration;

use clap::Args;
use vc_engine::{PlanRequest, SelectRequest};

use crate::project::Located;

/// The units `--changed-since` takes, and their length in seconds.
const WINDOW_UNITS: &[(char, u64)] = &[('h', 3_600), ('d', 86_400), ('w', 604_800)];

/// The selectors (cli-workflow "Selecting what to run").
#[derive(Args)]
pub struct SelectArgs {
    /// Test IDs (`users.crud#getsUser`, `:n` for one instance), file IDs, globs over IDs, or
    /// paths to test files and folders.
    selectors: Vec<String>,
    /// Only tests with this tag expression; `!` excludes, `,` means or. Repeat to require
    /// every one.
    #[arg(short = 't', long = "tag", value_name = "TAGS")]
    tags: Vec<String>,
    /// Only tests whose full ID or name contains this text, ignoring case.
    #[arg(short = 'g', long, value_name = "TEXT")]
    grep: Option<String>,
    /// Only files changed since this git ref, or the uncommitted and untracked changes when
    /// none is given.
    #[arg(long, value_name = "REF", num_args = 0..=1, default_missing_value = "")]
    changed: Option<String>,
    /// Only files a commit in this window touched (`12h`, `7d`, `2w`), or that are uncommitted
    /// or untracked (D249).
    #[arg(long, value_name = "DURATION", value_parser = window, conflicts_with = "changed")]
    changed_since: Option<Duration>,
    /// Also plans the tests marked `skip`.
    #[arg(long)]
    include_skipped: bool,
}

impl SelectArgs {
    /// The selection, with each selector that names a file or folder as a project path.
    ///
    /// # Errors
    ///
    /// A path that is outside the project.
    pub fn request(&self, located: &Located) -> Result<SelectRequest, String> {
        let mut select = SelectRequest {
            tags: self.tags.clone(),
            grep: self.grep.clone(),
            changed: self.changed.clone(),
            changed_since: self.changed_since,
            include_skipped: self.include_skipped,
            ..SelectRequest::default()
        };
        for selector in &self.selectors {
            if located.base.join(selector).exists() {
                select.paths.push(located.project_path(Path::new(selector))?);
            } else {
                select.ids.push(selector.clone());
            }
        }
        Ok(select)
    }
}

/// `--changed-since`'s window: a whole number above zero and `h`, `d` or `w`.
fn window(text: &str) -> Result<Duration, String> {
    let wrong = || format!("`{text}` is not a duration such as `12h`, `7d` or `2w`");
    let unit = text.chars().last().ok_or_else(wrong)?;
    let (_, seconds) = WINDOW_UNITS.iter().find(|(name, _)| *name == unit).ok_or_else(wrong)?;
    let digits = text.strip_suffix(unit).filter(|digits| digits.bytes().all(|b| b.is_ascii_digit()));
    let count: u64 = digits.ok_or_else(wrong)?.parse().map_err(|_| wrong())?;
    let total = count.checked_mul(*seconds).filter(|total| *total > 0).ok_or_else(wrong)?;
    Ok(Duration::from_secs(total))
}

/// The selectors, and the profiles and settings a plan is made with.
#[derive(Args)]
pub struct PlanArgs {
    #[command(flatten)]
    select: SelectArgs,
    /// The active profiles, as a comma list or repeated; later ones win.
    #[arg(long = "profile", value_name = "IDS")]
    profiles: Vec<String>,
    /// Sets one config value for this run (`defaults.timeout=5s`, `vars.userId=7`), over
    /// every other config layer (D129); a file's, test's or step's own `vars` still win over a
    /// config var.
    #[arg(long = "set", value_name = "KEY=VALUE")]
    sets: Vec<String>,
    /// Runs every selected test this many times, instead of its own `repeat`.
    #[arg(long, value_name = "N")]
    repeat: Option<u32>,
}

impl PlanArgs {
    /// The plan request.
    ///
    /// # Errors
    ///
    /// A path selector that is outside the project.
    pub fn request(&self, located: &Located) -> Result<PlanRequest, String> {
        Ok(PlanRequest {
            select: self.select.request(located)?,
            repeat: self.repeat,
            profiles: self.profiles.clone(),
            sets: self.sets.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::window;

    #[test]
    fn a_window_is_a_whole_number_of_hours_days_or_weeks() {
        assert_eq!(window("12h"), Ok(Duration::from_hours(12)));
        assert_eq!(window("7d"), Ok(Duration::from_hours(7 * 24)));
        assert_eq!(window("2w"), Ok(Duration::from_hours(14 * 24)));
        for bad in ["", "d", "7", "0d", "-1d", "+1d", "1.5d", "7dd", "7 d", "3m", "99999999999999999w"] {
            assert!(window(bad).is_err(), "{bad:?}");
        }
    }
}
