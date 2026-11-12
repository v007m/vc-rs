//! `vc alerts test`: one sample notice through every channel of every alert rule whose
//! `profiles` match, so a team sees its channels work without a failing run (D257). Nothing
//! runs and nothing is recorded.

use std::time::Duration;

use vc_interfaces::{Notice, NoticeFailure, RunSummary};
use vc_model::{Alert, AlertTrigger, Program, RunStatus, Totals};

use crate::alerts::in_profiles;
use crate::notify::Senders;
use crate::ports::SecretSource;
use crate::{CallRequest, Engine, EngineError, ExplainRequest};

/// What sending the sample to one channel did.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AlertSent {
    /// The rule's place in `alerts`, counting from 1.
    pub rule: usize,
    /// The channel kind: `slack`, `webhook`, …
    pub channel: String,
    /// Why it was not sent, `None` when the channel took it. It never holds a URL or key.
    pub error: Option<String>,
}

impl Engine {
    /// Sends a sample notice, titled `[test]`, through every channel of every alert rule whose
    /// `profiles` match `request`'s, ignoring its `tags` and `on`. Empty when no rule matches.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] for a bad setting or secrets that cannot load, and the errors
    /// of loading the project.
    pub async fn test_alerts(&self, request: &ExplainRequest) -> Result<Vec<AlertSent>, EngineError> {
        let call = CallRequest { profiles: request.profiles.clone(), sets: request.sets.clone(), ..Default::default() };
        let prepared = self.adhoc(&call)?;
        let program = &prepared.program;
        let notice = self.sample(program, &prepared.names);
        let rules = program.config.alerts.iter().enumerate().filter(|(_, rule)| in_profiles(rule, &notice.summary));
        let rules: Vec<(usize, &Alert)> = rules.map(|(index, rule)| (index + 1, rule)).collect();
        if rules.is_empty() {
            return Ok(Vec::new());
        }
        let secrets = self.secrets(SecretSource::of(&prepared), &vc_planner::alert_secrets(program)).await?;
        let senders = self.senders(&prepared, &secrets)?;
        Ok(sent(&senders, (program, &rules), &notice).await)
    }

    /// The sample: one passed test and one failed, `<project>#sampleTest`, under `profiles`.
    fn sample(&self, program: &Program, profiles: &[String]) -> Notice {
        let project = self.project_name(program);
        let totals = Totals { passed: 1, failed: 1, ..Totals::default() };
        let summary = RunSummary::new(RunStatus::Failed, totals).timed(self.clock.now_ms(), Duration::ZERO);
        let failure = NoticeFailure::new(format!("{project}#sampleTest"), "this is a test alert");
        let summary = summary.profiles(profiles.iter().cloned());
        Notice::new(AlertTrigger::Failure, project, summary).failures(vec![failure]).as_test()
    }
}

/// The alert rules a sample goes to, each with its place in `alerts` counting from 1.
type Rules<'a> = [(usize, &'a Alert)];

/// Sends `notice` through each channel of `rules`, in written order.
async fn sent(senders: &Senders, (program, rules): (&Program, &Rules<'_>), notice: &Notice) -> Vec<AlertSent> {
    let mut sent = Vec::new();
    for (rule, alert) in rules {
        for channel in &alert.notify {
            let error = senders.send(program, channel, notice).await.err();
            sent.push(AlertSent { rule: *rule, channel: channel.kind.clone(), error });
        }
    }
    sent
}

#[cfg(test)]
#[path = "alert_samples_tests.rs"]
mod tests;
