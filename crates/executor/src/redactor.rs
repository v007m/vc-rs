//! The redactor: masks what must not leave the executor before an event does (D106, D113,
//! D171, D185).
//!
//! Its built-in masking comes first and is synchronous: the values of `Authorization`,
//! `Cookie`, `Set-Cookie` and the config's `redact.headers`; the config's `redact.fields`
//! wherever they appear in JSON; every secret value and credential it is told, inside any
//! text. Events then go through the config's `redact.rules` in order, which are async (a
//! `function:` rule awaits a hook). What cannot wait (Debug output, error text, a call's check
//! results) gets the built-in masking only. The masks sit behind an `arc-swap`, so readers
//! never wait when a credential arrives mid-run.

use std::fmt;
use std::sync::Arc;

use arc_swap::ArcSwap;
use serde_json::Value;
use vc_model::{ActionOutcome, CheckKind, CheckResult, Config, Event, Failure, Segment, TestResult};

use crate::EventBus;
use crate::mask::{MaskRule, MaskRules, Masking, Masks};

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

    /// A redactor for the config's `redact` settings: its headers, fields and rules, the rules
    /// made by the standard [`MaskRules`].
    #[must_use]
    pub fn for_config(config: &Config) -> Self {
        let redactor = Self::new(&config.redact_headers).with_rules(MaskRules::standard().for_config(config));
        redactor.hide_fields(&config.redact_fields);
        redactor
    }

    /// A redactor with these masks, and every value told so far, applying `rules` after them:
    /// how a run takes rules made by a registry with more makers (a `function:` rule, M4).
    #[must_use]
    pub fn with_rules(&self, rules: Vec<Arc<dyn MaskRule>>) -> Self {
        Self { masks: ArcSwap::new(self.masks.load_full()), rules }
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

    /// `text` with every secret value masked: the built-in masking only, for what cannot
    /// wait; events get the rules too.
    #[must_use]
    pub fn text(&self, text: &str) -> String {
        self.masks.load().without_values(text)
    }

    /// `value` with every secret value and hidden field masked: the built-in masking only.
    #[must_use]
    pub fn value(&self, value: Value) -> Value {
        self.masks.load().value(value)
    }

    /// `value` with the built-in masking, then the rules in order: what leaves the run as an
    /// event's value would, for a value stored outside one (an agent call's arguments).
    pub async fn masked(&self, value: Value) -> Value {
        let masks = self.masks.load_full();
        Masking { masks: &masks, rules: &self.rules }.value(value).await
    }

    /// `check` with the built-in masking: a masked header's expected and actual values
    /// replaced by [`MASKED`], and every secret value and hidden field masked in the rest; its
    /// diff is made again from the masked values. Its event gets the rules too.
    #[must_use]
    pub fn check(&self, check: CheckResult) -> CheckResult {
        let masks = self.masks.load();
        if hides_header(&masks, &check) {
            return header_masked(check);
        }
        let (expected, actual) = (masks.value(check.expected), check.actual.map(|actual| masks.value(actual)));
        let error = check.error.map(|error| masks.without_values(&error));
        rediffed(CheckResult { expected, actual, error, ..check })
    }

    /// `event` with what it must not show masked: the built-in masking, then the rules in
    /// order (D185).
    pub async fn event(&self, event: Event) -> Event {
        let masks = self.masks.load_full();
        let masking = Masking { masks: &masks, rules: &self.rules };
        match event {
            Event::CheckEvaluated { action, owner, check } => {
                Event::CheckEvaluated { action, owner, check: Box::new(checked(&masking, *check).await) }
            }
            Event::ActionFinished { action, owner, outcome } => {
                Event::ActionFinished { action, owner, outcome: outcome_masked(&masking, outcome).await }
            }
            Event::TestFinished { result } => Event::TestFinished { result: tested(&masking, result).await },
            Event::Exchanged { action, owner, exchange } => {
                let exchange = Box::new(crate::exchange_mask::exchange(&masking, *exchange).await);
                Event::Exchanged { action, owner, exchange }
            }
            Event::Warned { action, owner, warning } => {
                Event::Warned { action, owner, warning: failed(&masking, warning).await }
            }
            Event::CleanupFinished { mut cleanup } => {
                cleanup.outcome = outcome_masked(&masking, cleanup.outcome).await;
                Event::CleanupFinished { cleanup }
            }
            unmasked @ (Event::RunStarted { .. }
            | Event::FileStarted { .. }
            | Event::FileFinished { .. }
            | Event::ActionStarted { .. }
            | Event::CleanupsAbandoned { .. }
            | Event::RunFinished { .. }
            | Event::LoadTick { .. }) => unmasked,
        }
    }
}

/// Whether `check` is on a single header `masks` hides.
fn hides_header(masks: &Masks, check: &CheckResult) -> bool {
    match check.target.as_ref().map(|path| path.segments.as_slice()) {
        Some([Segment::Key(name)]) if check.kind == CheckKind::Header => masks.hides_header(&name.to_ascii_lowercase()),
        _ => false,
    }
}

/// A check on a masked header: its expected and actual values masked whole, and no diff.
fn header_masked(check: CheckResult) -> CheckResult {
    let masked = || Value::String(MASKED.to_owned());
    CheckResult { expected: masked(), actual: check.actual.as_ref().map(|_| masked()), diff: None, ..check }
}

/// `result` with its diff, if it had one, made again from its (masked) values.
fn rediffed(result: CheckResult) -> CheckResult {
    let actual = result.actual.as_ref().filter(|_| result.diff.is_some());
    let diff = actual.and_then(|actual| crate::check::diff(&result.expected, actual));
    CheckResult { diff, ..result }
}

/// `check` masked whole: the built-in masking, then the rules.
async fn checked(masking: &Masking<'_>, check: CheckResult) -> CheckResult {
    if hides_header(masking.masks, &check) {
        return header_masked(check);
    }
    let expected = masking.value(check.expected).await;
    let actual = match check.actual {
        Some(actual) => Some(masking.value(actual).await),
        None => None,
    };
    let error = text_of(masking, check.error).await;
    rediffed(CheckResult { expected, actual, error, ..check })
}

async fn text_of(masking: &Masking<'_>, text: Option<String>) -> Option<String> {
    match text {
        Some(text) => Some(masking.text(&text).await),
        None => None,
    }
}

async fn outcome_masked(masking: &Masking<'_>, outcome: ActionOutcome) -> ActionOutcome {
    match outcome {
        ActionOutcome::Failed(failure) => ActionOutcome::Failed(failed(masking, failure).await),
        ActionOutcome::Passed => ActionOutcome::Passed,
    }
}

async fn failed(masking: &Masking<'_>, failure: Failure) -> Failure {
    Failure { message: masking.text(&failure.message).await, ..failure }
}

async fn tested(masking: &Masking<'_>, mut result: TestResult) -> TestResult {
    result.failure = match result.failure {
        Some(failure) => Some(failed(masking, failure).await),
        None => None,
    };
    result.skip_reason = text_of(masking, result.skip_reason).await;
    result
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

    /// Publishes `event`, masked; a rule may wait on a hook first.
    pub async fn publish(&self, event: Event) {
        self.bus.publish(self.redactor.event(event).await);
    }

    /// Waits until every event published so far has reached the subscribers.
    pub async fn flush(&self) {
        self.bus.flush().await;
    }
}

#[cfg(test)]
#[path = "redactor_tests.rs"]
mod tests;
