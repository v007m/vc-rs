//! The redactor: masks what must not leave the executor before an event does (D106, D113,
//! D171).
//!
//! It masks the values of `Authorization`, `Cookie`, `Set-Cookie` and the config's
//! `redact.headers`; the config's `redact.fields` wherever they appear in JSON; every secret
//! value and credential it is told, inside any text; and then the config's `redact.rules` and
//! any rule a hook adds. The masks sit behind an `arc-swap`, so readers never wait when a
//! credential arrives mid-run.

use std::fmt;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::Value;
use vc_model::{ActionOutcome, CheckKind, CheckResult, Config, Event, Failure, Segment};

use crate::EventBus;
use crate::check;
use crate::mask::{self, MaskRule, Masking, Masks};

/// The headers every run masks, lowercase.
const ALWAYS: [&str; 3] = ["authorization", "cookie", "set-cookie"];

/// What a masked value reads as.
pub const MASKED: &str = "••••";

/// Masks secrets, credentials and what the config names in events.
pub struct Redactor {
    masks: ArcSwap<Masks>,
    rules: Vec<Arc<dyn MaskRule>>,
}

impl fmt::Debug for Redactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Redactor").field("masks", &self.masks.load()).field("rules", &self.rules.len()).finish()
    }
}

impl Redactor {
    /// A redactor masking the always-masked headers and `headers` (any case).
    #[must_use]
    pub fn new(headers: &[String]) -> Self {
        let redactor = Self { masks: ArcSwap::from_pointee(Masks::default()), rules: Vec::new() };
        redactor.hide_headers(ALWAYS.iter().copied().chain(headers.iter().map(String::as_str)));
        redactor
    }

    /// A redactor for the config's `redact` settings: its headers, fields and rules.
    #[must_use]
    pub fn for_config(config: &Config) -> Self {
        let redactor = config
            .redact_rules
            .iter()
            .fold(Self::new(&config.redact_headers), |redactor, rule| redactor.with_rule(mask::rule(rule)));
        redactor.hide_fields(&config.redact_fields);
        redactor
    }

    /// The same redactor with `rule` applied after the others: the slot a hook's rule takes.
    #[must_use]
    pub fn with_rule(mut self, rule: Arc<dyn MaskRule>) -> Self {
        self.rules.push(rule);
        self
    }

    /// Masks `headers` (any case) from now on, in every event that has not left yet.
    pub fn hide_headers(&self, headers: impl IntoIterator<Item = impl AsRef<str>>) {
        let added: Vec<String> = headers.into_iter().map(|name| name.as_ref().to_owned()).collect();
        self.masks.rcu(|masks| masks.with_headers(&added));
    }

    /// Masks the JSON fields called `fields` (any case) from now on.
    pub fn hide_fields(&self, fields: impl IntoIterator<Item = impl AsRef<str>>) {
        let added: Vec<String> = fields.into_iter().map(|name| name.as_ref().to_owned()).collect();
        self.masks.rcu(|masks| masks.with_fields(&added));
    }

    /// Masks `values` wherever they appear from now on, also as a URL's query encodes them:
    /// secrets when the run starts, credentials as they arrive. Values under four characters
    /// are left alone.
    pub fn hide_values(&self, values: impl IntoIterator<Item = impl AsRef<str>>) {
        let encoded = |value: &str| Some(crate::action::encode(value)).filter(|encoded| encoded != value);
        let both = |value: &str| [Some(value.to_owned()), encoded(value)].into_iter().flatten();
        let added: Vec<String> = values.into_iter().flat_map(|value| both(value.as_ref())).collect();
        if !added.is_empty() {
            self.masks.rcu(|masks| masks.with_values(&added));
        }
    }

    /// `text` with every secret value and rule match masked.
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        self.masking(|masking| masking.text(text))
    }

    /// `value` with every secret value, hidden field and rule match masked.
    #[must_use]
    pub fn value(&self, value: Value) -> Value {
        self.masking(|masking| masking.value(value))
    }

    /// `event` with what it must not show masked.
    #[must_use]
    pub fn event(&self, event: Event) -> Event {
        match event {
            Event::CheckEvaluated { action, owner, check } => {
                Event::CheckEvaluated { action, owner, check: Box::new(self.check(*check)) }
            }
            Event::ActionFinished { action, owner, outcome } => {
                Event::ActionFinished { action, owner, outcome: self.outcome(outcome) }
            }
            Event::TestFinished { mut result } => {
                result.failure = result.failure.map(|failure| self.failure(failure));
                result.skip_reason = result.skip_reason.map(|reason| self.text(&reason));
                Event::TestFinished { result }
            }
            Event::Exchanged { action, owner, exchange } => {
                let exchange = self.masking(|masking| crate::exchange_mask::exchange(masking, *exchange));
                Event::Exchanged { action, owner, exchange: Box::new(exchange) }
            }
            Event::Warned { action, owner, warning } => Event::Warned { action, owner, warning: self.failure(warning) },
            Event::CleanupFinished { mut cleanup } => {
                cleanup.outcome = self.outcome(cleanup.outcome);
                Event::CleanupFinished { cleanup }
            }
            unmasked @ (Event::RunStarted { .. }
            | Event::FileStarted { .. }
            | Event::FileFinished { .. }
            | Event::ActionStarted { .. }
            | Event::CleanupsAbandoned { .. }
            | Event::RunFinished { .. }) => unmasked,
        }
    }

    /// `check` with a masked header's expected and actual values replaced by [`MASKED`], and
    /// every secret value, hidden field and rule match masked in the rest; its diff is made
    /// again from the masked values.
    #[must_use]
    pub fn check(&self, check: CheckResult) -> CheckResult {
        if self.masks_header(&check) {
            let masked = || Value::String(MASKED.to_owned());
            return CheckResult { expected: masked(), actual: check.actual.map(|_| masked()), diff: None, ..check };
        }
        let had_diff = check.diff.is_some();
        self.masking(|masking| {
            let expected = masking.value(check.expected);
            let actual = check.actual.map(|actual| masking.value(actual));
            let diff = actual.as_ref().filter(|_| had_diff).and_then(|actual| check::diff(&expected, actual));
            let error = check.error.map(|error| masking.text(&error));
            CheckResult { expected, actual, diff, error, ..check }
        })
    }

    fn masks_header(&self, check: &CheckResult) -> bool {
        match check.target.as_ref().map(|path| path.segments.as_slice()) {
            Some([Segment::Key(name)]) if check.kind == CheckKind::Header => {
                self.masks.load().hides_header(&name.to_ascii_lowercase())
            }
            _ => false,
        }
    }

    fn outcome(&self, outcome: ActionOutcome) -> ActionOutcome {
        match outcome {
            ActionOutcome::Failed(failure) => ActionOutcome::Failed(self.failure(failure)),
            ActionOutcome::Passed => ActionOutcome::Passed,
        }
    }

    fn failure(&self, failure: Failure) -> Failure {
        Failure { message: self.text(&failure.message), ..failure }
    }

    fn masking<R>(&self, use_it: impl FnOnce(&Masking<'_>) -> R) -> R {
        let masks = self.masks.load();
        use_it(&Masking { masks: &masks, rules: &self.rules })
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
