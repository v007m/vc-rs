//! The redactor: masks what must not leave the executor before an event does (D106, D113).
//!
//! Until secrets land in M3 it masks headers only: `Authorization`, `Cookie`, `Set-Cookie` and
//! the config's `redact.headers`. The masks sit behind an `arc-swap`, so readers never wait
//! when a later milestone swaps in more.

use std::collections::BTreeSet;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::Value;
use vc_model::{CheckKind, CheckResult, Event, Segment};

use crate::EventBus;

/// The headers every run masks, lowercase.
const ALWAYS: [&str; 3] = ["authorization", "cookie", "set-cookie"];

/// What a masked value reads as.
pub const MASKED: &str = "••••";

/// The masks in force: header names, lowercase.
#[derive(Debug, Default)]
struct Masks {
    headers: BTreeSet<String>,
}

/// Masks header values in events.
#[derive(Debug)]
pub struct Redactor {
    masks: ArcSwap<Masks>,
}

impl Redactor {
    /// A redactor masking the always-masked headers and `headers` (any case).
    #[must_use]
    pub fn new(headers: &[String]) -> Self {
        let redactor = Self { masks: ArcSwap::from_pointee(Masks::default()) };
        redactor.hide_headers(ALWAYS.iter().copied().chain(headers.iter().map(String::as_str)));
        redactor
    }

    /// Masks `headers` (any case) from now on, in every event that has not left yet.
    pub fn hide_headers(&self, headers: impl IntoIterator<Item = impl AsRef<str>>) {
        let added: Vec<String> = headers.into_iter().map(|name| name.as_ref().to_ascii_lowercase()).collect();
        self.masks.rcu(|masks| {
            let mut headers = masks.headers.clone();
            headers.extend(added.iter().cloned());
            Masks { headers }
        });
    }

    /// `event` with what it must not show masked.
    #[must_use]
    pub fn event(&self, event: Event) -> Event {
        match event {
            Event::CheckEvaluated { action, owner, check } => {
                Event::CheckEvaluated { action, owner, check: Box::new(self.check(*check)) }
            }
            other => other,
        }
    }

    /// `check` with a masked header's expected and actual values replaced by [`MASKED`].
    #[must_use]
    pub fn check(&self, check: CheckResult) -> CheckResult {
        let header = match check.target.as_ref().map(|path| path.segments.as_slice()) {
            Some([Segment::Key(name)]) if check.kind == CheckKind::Header => name.to_ascii_lowercase(),
            _ => return check,
        };
        if !self.masks.load().headers.contains(&header) {
            return check;
        }
        let masked = || Value::String(MASKED.to_owned());
        CheckResult { expected: masked(), actual: check.actual.as_ref().map(|_| masked()), diff: None, ..check }
    }
}

/// The executor's way out for events: each passes through the [`Redactor`] onto the
/// [`EventBus`].
#[derive(Clone, Debug)]
pub struct RedactingSink {
    bus: EventBus,
    redactor: Arc<Redactor>,
}

impl RedactingSink {
    /// A sink masking with `redactor` before publishing on `bus`.
    #[must_use]
    pub const fn new(bus: EventBus, redactor: Arc<Redactor>) -> Self {
        Self { bus, redactor }
    }

    /// Publishes `event`, masked.
    pub fn publish(&self, event: Event) {
        self.bus.publish(self.redactor.event(event));
    }

    /// Waits until every event published so far has reached the subscribers.
    pub async fn flush(&self) {
        self.bus.flush().await;
    }
}

#[cfg(test)]
#[path = "redactor_tests.rs"]
mod tests;
