//! The channel kinds by name (D106), each a [`Payload`] posted through one transport.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use http::Uri;
use serde_json::{Map, Value};
use vc_interfaces::{Notice, Notifier, NotifierFactory, NotifyError, Transport};

use crate::channels::{PagerDuty, Slack, Teams, Webhook};
use crate::post::HttpNotifier;

/// One channel kind: where its settings say to post, and the JSON it posts.
pub trait Payload: Send + Sync {
    /// The URL to post to, from the channel's settings.
    ///
    /// # Errors
    ///
    /// Why the settings are not usable, naming keys but never their values.
    fn endpoint(&self, settings: &Map<String, Value>) -> Result<Uri, String>;

    /// The JSON body announcing `notice`.
    fn body(&self, notice: &Notice, settings: &Map<String, Value>) -> Value;
}

/// Every channel kind by the key a `notify` entry gives it, posting through one transport.
#[derive(Clone)]
pub struct NotifierRegistry {
    transport: Arc<dyn Transport>,
    by_channel: BTreeMap<String, Arc<dyn Payload>>,
}

impl NotifierRegistry {
    /// No channels, posting through `transport`.
    #[must_use]
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport, by_channel: BTreeMap::new() }
    }

    /// The standard channels, posting through `transport`.
    #[must_use]
    pub fn standard(transport: Arc<dyn Transport>) -> Self {
        Self::new(transport)
            .with("webhook", Arc::new(Webhook))
            .with("slack", Arc::new(Slack))
            .with("teams", Arc::new(Teams))
            .with("pagerduty", Arc::new(PagerDuty))
    }

    /// These channels, with `channel` added or replaced.
    #[must_use]
    pub fn with(mut self, channel: &str, payload: Arc<dyn Payload>) -> Self {
        self.by_channel.insert(channel.to_owned(), payload);
        self
    }
}

impl fmt::Debug for NotifierRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_channel.keys()).finish()
    }
}

impl NotifierFactory for NotifierRegistry {
    fn channels(&self) -> Vec<String> {
        self.by_channel.keys().cloned().collect()
    }

    fn create(&self, channel: &str, settings: &Value) -> Result<Box<dyn Notifier>, NotifyError> {
        let unknown = || NotifyError::Unknown { channel: channel.to_owned(), known: self.channels().join(", ") };
        let payload = self.by_channel.get(channel).ok_or_else(unknown)?;
        let refused = |reason: String| NotifyError::Settings { channel: channel.to_owned(), reason };
        let settings = settings.as_object().ok_or_else(|| refused("needs a mapping".to_owned()))?;
        let url = payload.endpoint(settings).map_err(refused)?;
        Ok(Box::new(HttpNotifier {
            channel: channel.to_owned(),
            url,
            settings: settings.clone(),
            payload: Arc::clone(payload),
            transport: Arc::clone(&self.transport),
        }))
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
