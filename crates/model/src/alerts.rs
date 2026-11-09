use serde::{Deserialize, Serialize};

use crate::{Tag, TemplateValue};

/// What makes an alert rule fire: a value of its `on:` list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum AlertTrigger {
    /// A test failed or was blocked.
    Failure,
    /// The run passed after the previous run of the same selection and profiles failed.
    Recovery,
    /// A test passed only on a retry.
    Flaky,
    /// A load threshold was breached.
    Threshold,
}

impl AlertTrigger {
    /// Every trigger, in the order the syntax reference lists them.
    pub const ALL: [Self; 4] = [Self::Failure, Self::Recovery, Self::Flaky, Self::Threshold];

    /// The name an `on:` list uses.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Failure => "failure",
            Self::Recovery => "recovery",
            Self::Flaky => "flaky",
            Self::Threshold => "threshold",
        }
    }
}

/// One entry of `alerts`: when to notify, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Alert {
    /// `on`: the triggers that fire it; `[failure]` when it is left out.
    pub on: Vec<AlertTrigger>,
    /// `profiles`: only runs under one of these; every run when empty.
    pub profiles: Vec<String>,
    /// `tags`: only tests with one of these; every test when empty.
    pub tags: Vec<Tag>,
    /// `notify`: the channels it sends to, in written order.
    pub notify: Vec<AlertChannel>,
}

/// One channel of an alert's `notify`, such as `slack: {channel, webhook}`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct AlertChannel {
    /// The channel kind, the entry's key: `webhook`, `slack`, `teams`, `pagerduty`.
    pub kind: String,
    /// Its settings, resolved (secrets included) when the run ends.
    pub settings: TemplateValue,
}

#[cfg(test)]
mod tests {
    use super::AlertTrigger;

    #[test]
    fn a_trigger_is_named_as_the_on_list_writes_it() {
        let names = AlertTrigger::ALL.map(AlertTrigger::name);
        assert_eq!(names, ["failure", "recovery", "flaky", "threshold"]);
        for trigger in AlertTrigger::ALL {
            assert_eq!(serde_json::to_value(trigger).unwrap(), trigger.name());
        }
    }
}
