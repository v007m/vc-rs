use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver, Sender};

use async_trait::async_trait;
use serde_json::Value;
use vc_interfaces::{Notice, Notifier, NotifierFactory, NotifyError};

/// One notice a [`FakeNotifiers`] notifier was given, with the channel it was built for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sent {
    /// The channel kind.
    pub channel: String,
    /// The settings it was built from.
    pub settings: Value,
    /// The notice.
    pub notice: Notice,
}

/// The receiving end of a [`FakeNotifiers`].
#[derive(Debug)]
pub struct SentLog(Receiver<Sent>);

impl SentLog {
    /// Everything sent since the last call, oldest first.
    #[must_use]
    pub fn take(&self) -> Vec<Sent> {
        self.0.try_iter().collect()
    }
}

/// A [`NotifierFactory`] whose notifiers record what they are given and send nothing, for engine
/// tests. A channel marked [`failing`](Self::failing) refuses every notice.
#[derive(Clone, Debug)]
pub struct FakeNotifiers {
    channels: BTreeSet<String>,
    failing: BTreeSet<String>,
    records: Sender<Sent>,
}

impl FakeNotifiers {
    /// A factory building `channels`, and the log its notifiers record into.
    #[must_use]
    pub fn new(channels: &[&str]) -> (Self, SentLog) {
        let (records, inbox) = mpsc::channel();
        let channels = channels.iter().map(|channel| (*channel).to_owned()).collect();
        (Self { channels, failing: BTreeSet::new(), records }, SentLog(inbox))
    }

    /// This factory, whose `channel` notifiers fail to send.
    #[must_use]
    pub fn failing(mut self, channel: &str) -> Self {
        self.failing.insert(channel.to_owned());
        self
    }
}

impl NotifierFactory for FakeNotifiers {
    fn channels(&self) -> Vec<String> {
        self.channels.iter().cloned().collect()
    }

    fn create(&self, channel: &str, settings: &Value) -> Result<Box<dyn Notifier>, NotifyError> {
        if !self.channels.contains(channel) {
            return Err(NotifyError::Unknown { channel: channel.to_owned(), known: self.channels().join(", ") });
        }
        if !settings.is_object() {
            return Err(NotifyError::Settings { channel: channel.to_owned(), reason: "needs a mapping".to_owned() });
        }
        Ok(Box::new(RecordingNotifier {
            channel: channel.to_owned(),
            settings: settings.clone(),
            fails: self.failing.contains(channel),
            records: self.records.clone(),
        }))
    }
}

struct RecordingNotifier {
    channel: String,
    settings: Value,
    fails: bool,
    records: Sender<Sent>,
}

#[async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        if self.fails {
            return Err(NotifyError::Send { channel: self.channel.clone(), reason: "HTTP 503".to_owned() });
        }
        let sent = Sent { channel: self.channel.clone(), settings: self.settings.clone(), notice: notice.clone() };
        self.records.send(sent).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_interfaces::{Notice, NotifierFactory, NotifyError, RunSummary};
    use vc_model::{AlertTrigger, RunStatus, Totals};

    use super::FakeNotifiers;
    use crate::{ChannelCase, notifier_factory_suite, notifier_suite};

    #[tokio::test]
    async fn fake_notifiers_meet_the_contract() {
        let (factory, _log) = FakeNotifiers::new(&["webhook", "slack"]);
        let settings = json!({"url": "https://hooks.example/s3cr3t-path"});
        let case = ChannelCase::new("webhook", settings.clone(), json!("https://hooks.example/s3cr3t-path"));
        notifier_factory_suite(&factory, &case);
        notifier_suite(&mut || factory.create("slack", &settings).unwrap()).await;
    }

    #[tokio::test]
    async fn records_each_notice_with_its_channel_or_fails_a_failing_one() {
        let (factory, log) = FakeNotifiers::new(&["webhook", "slack"]);
        let factory = factory.failing("slack");
        let notice =
            Notice::new(AlertTrigger::Failure, "shop".into(), RunSummary::new(RunStatus::Failed, Totals::default()));
        let settings = json!({"url": "https://hooks.example/x"});
        factory.create("webhook", &settings).unwrap().notify(&notice).await.unwrap();
        let sent = log.take();
        assert_eq!(sent.len(), 1);
        assert_eq!((sent[0].channel.as_str(), &sent[0].settings, &sent[0].notice), ("webhook", &settings, &notice));
        let failed = factory.create("slack", &settings).unwrap().notify(&notice).await.unwrap_err();
        assert_eq!(failed, NotifyError::Send { channel: "slack".into(), reason: "HTTP 503".into() });
        assert_eq!(log.take(), []);
    }
}
