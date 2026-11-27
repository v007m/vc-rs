//! The config's `schedules` (D520, D700): runs the vc server starts on a cron, or that a CI
//! workflow exported from one starts.

use serde::{Deserialize, Serialize};

use crate::{AlertChannel, Cron};

/// What starts a schedule's runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ScheduleRunner {
    /// `vc ui`'s server, while it is up.
    #[default]
    VcServer,
    /// A GitHub Actions workflow the schedule was exported to; the server leaves it alone.
    GithubActions,
}

impl ScheduleRunner {
    pub const ALL: [Self; 2] = [Self::VcServer, Self::GithubActions];

    /// The name `runner:` takes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::VcServer => "vcServer",
            Self::GithubActions => "githubActions",
        }
    }

    /// The runner `name` names.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|runner| runner.name() == name)
    }
}

/// One entry of `schedules`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Schedule {
    pub id: String,
    pub name: Option<String>,
    pub cron: Cron,
    /// What runs, as `vc run`'s selectors; every test when empty.
    pub selector: Vec<String>,
    /// The profiles it runs under; the default profile when empty.
    pub profiles: Vec<String>,
    /// Channels told when a run fails or is blocked, as an `alerts` rule's `notify`.
    pub notify: Vec<AlertChannel>,
    pub runner: ScheduleRunner,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::ScheduleRunner;

    #[test]
    fn a_runner_is_named_as_the_config_writes_it() {
        for runner in ScheduleRunner::ALL {
            assert_eq!(serde_json::to_value(runner).unwrap(), runner.name());
            assert_eq!(ScheduleRunner::named(runner.name()), Some(runner));
        }
        assert_eq!(ScheduleRunner::named("cron"), None);
    }
}
