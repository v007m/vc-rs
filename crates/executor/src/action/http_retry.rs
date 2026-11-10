//! The `http.retry` policy a call is sent with (D143): the api's, else the config's.

use std::sync::Arc;

use vc_interfaces::Random;
use vc_model::{Api, HttpRetry, Program};

use super::request;
use crate::{HttpRetryPolicy, Jittered, NoRetry, RetryOn, RetryPolicy};

/// The transport failures `on` may name.
const FAILURES: [(&str, RetryOn); 2] = [("connect", RetryOn::Connect), ("timeout", RetryOn::Timeout)];

/// The policy for calls to `api`: none when `attempts` is 0, its waits spread with draws from
/// `random` when `jitter` is set.
pub fn policy(program: &Program, api: &Api, random: &Arc<dyn Random>) -> Arc<dyn RetryPolicy> {
    let spec = api.http_retry.as_ref().unwrap_or(&program.config.http_retry);
    if spec.attempts == 0 {
        return Arc::new(NoRetry);
    }
    let policy: Arc<dyn RetryPolicy> = Arc::new(described(spec));
    if spec.jitter { Arc::new(Jittered::new(policy, Arc::clone(random))) } else { policy }
}

/// The policy `spec` describes, without jitter.
fn described(spec: &HttpRetry) -> HttpRetryPolicy {
    let failure = |name: &String| FAILURES.iter().find(|(named, _)| named == name).map(|(_, on)| *on);
    let failures = spec.on_transport.iter().filter_map(failure);
    HttpRetryPolicy {
        attempts: spec.attempts,
        on: failures.chain(spec.on_status.iter().copied().map(RetryOn::Status)).collect(),
        methods: spec.methods.iter().map(|method| request::method(*method)).collect(),
        base: spec.base.get(),
        max: spec.max.get(),
    }
}

#[cfg(test)]
#[path = "http_retry_tests.rs"]
mod tests;
