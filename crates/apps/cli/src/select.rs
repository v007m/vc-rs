//! The flags `vc plan`, `vc run` and `vc list` share: the selectors, and the profiles and
//! settings a plan is made with.

use std::path::Path;

use clap::Args;
use vc_engine::{PlanRequest, SelectRequest};

use crate::project::Located;

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
