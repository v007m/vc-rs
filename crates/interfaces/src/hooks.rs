//! Running a hook: a `beforeRequest` or `afterResponse` hook, a custom check or a `function:`
//! action, in a sandbox with a time and memory budget (D172).

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use vc_diagnostics::Code;
use vc_model::{HookModules, HookRef};

use crate::Secrets;

/// How long a hook may run when nothing says otherwise.
const DEFAULT_TIME: Duration = Duration::from_millis(200);

/// How much memory a hook's runtime may hold when nothing says otherwise: 64 MiB.
const DEFAULT_MEMORY: usize = 64 << 20;

/// The most time and memory one call may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Budget {
    /// How long the call may run.
    pub time: Duration,
    /// The most bytes the runtime may hold.
    pub memory: usize,
}

impl Budget {
    /// `time` and `memory`.
    #[must_use]
    pub const fn new(time: Duration, memory: usize) -> Self {
        Self { time, memory }
    }
}

impl Default for Budget {
    /// 200 ms and 64 MiB.
    fn default() -> Self {
        Self::new(DEFAULT_TIME, DEFAULT_MEMORY)
    }
}

/// One call of an exported hook function with a JSON input.
///
/// `Debug` shows the hook and the secret names, never the input or a secret value.
#[derive(Clone)]
#[non_exhaustive]
pub struct HookCall {
    /// The program's hook files; the hook's file and every file it imports are among them.
    pub modules: HookModules,
    /// The file and export to call.
    pub hook: HookRef,
    /// The one argument, as JSON.
    pub input: Value,
    /// The most it may use.
    pub budget: Budget,
    /// What `secret(name)` may return.
    pub secrets: Secrets,
}

impl HookCall {
    /// A call of `hook` with `input`, the default budget and no secrets.
    #[must_use]
    pub fn new(modules: HookModules, hook: HookRef, input: Value) -> Self {
        Self { modules, hook, input, budget: Budget::default(), secrets: Secrets::new() }
    }

    /// This call with `budget`.
    #[must_use]
    pub fn with_budget(self, budget: Budget) -> Self {
        Self { budget, ..self }
    }

    /// This call, with `secrets` for `secret(name)`.
    #[must_use]
    pub fn with_secrets(self, secrets: Secrets) -> Self {
        Self { secrets, ..self }
    }
}

impl fmt::Debug for HookCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookCall")
            .field("hook", &format_args!("{}#{}", self.hook.file, self.hook.export))
            .field("budget", &self.budget)
            .field("secrets", &self.secrets)
            .finish_non_exhaustive()
    }
}

/// What a hook returned.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct HookOutput {
    /// The returned value, as JSON; `null` for `undefined`.
    pub value: Value,
    /// The names it read with `secret(name)`, in the order first read; each use is logged and
    /// the values are redacted.
    pub secrets_used: Vec<String>,
}

impl HookOutput {
    /// `value`, having read no secrets.
    #[must_use]
    pub const fn new(value: Value) -> Self {
        Self { value, secrets_used: Vec::new() }
    }

    /// This output, having read the secrets `names`.
    #[must_use]
    pub fn reading(self, names: Vec<String>) -> Self {
        Self { secrets_used: names, ..self }
    }
}

/// Which budget a hook ran past.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Limit {
    /// Its time.
    Time,
    /// Its memory.
    Memory,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Time => "time",
            Self::Memory => "memory",
        })
    }
}

/// Why a hook gave no value. `line` is the TypeScript line it stopped at, when known.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HookError {
    /// It threw, or its promise was rejected.
    #[error("the hook threw: {message}{}", at(*line))]
    Threw {
        /// The error's message.
        message: String,
        /// Where it was thrown.
        line: Option<u32>,
    },
    /// It returned something JSON cannot hold, such as a function.
    #[error("the hook returned {what}, which is not JSON")]
    BadValue {
        /// What it returned.
        what: String,
    },
    /// It ran past its budget and was stopped.
    #[error("the hook ran past its {limit} budget and was stopped{}", at(*line))]
    Overran {
        /// The budget it ran past.
        limit: Limit,
        /// Where it was stopped.
        line: Option<u32>,
    },
    /// Its file or export is not among the modules.
    #[error("no hook `{hook}`")]
    NotFound {
        /// The hook, as `file#export`.
        hook: String,
    },
    /// The runner cannot run hooks right now.
    #[error("the hook runner is unavailable: {reason}")]
    Unavailable {
        /// Why.
        reason: String,
    },
}

/// ` (line n)` when the line is known.
fn at(line: Option<u32>) -> String {
    line.map(|line| format!(" (line {line})")).unwrap_or_default()
}

impl HookError {
    /// The diagnostic code: V5116 for an overrun, V5115 for everything else.
    #[must_use]
    pub const fn code(&self) -> Code {
        match self {
            Self::Overran { .. } => Code::V5116,
            Self::Threw { .. } | Self::BadValue { .. } | Self::NotFound { .. } | Self::Unavailable { .. } => {
                Code::V5115
            }
        }
    }
}

/// Runs hooks, one call at a time per caller, without file, network, process or environment
/// access.
#[async_trait]
pub trait HookRunner: Send + Sync {
    /// Calls `call.hook` with `call.input` and waits for its value, awaiting a returned promise.
    ///
    /// # Errors
    ///
    /// [`HookError`] when the hook throws, overruns its budget, returns a value that is not
    /// JSON, or cannot be found.
    async fn call(&self, call: HookCall) -> Result<HookOutput, HookError>;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use vc_diagnostics::Code;
    use vc_model::{HookModules, HookRef};

    use super::{Budget, HookCall, HookError, Limit};
    use crate::{Secret, Secrets};

    #[test]
    fn a_call_defaults_to_200ms_and_64_mib_and_debug_hides_input_and_secrets() {
        let hook = HookRef { file: "hooks/sign.ts".into(), export: "sign".into() };
        let call = HookCall::new(HookModules::default(), hook, json!({"token": "in-put"}));
        assert_eq!(call.budget, Budget::new(Duration::from_millis(200), 64 * 1024 * 1024));
        let mut secrets = Secrets::new();
        secrets.insert("KEY", Secret::new("s3cr3t"));
        let call = call.with_budget(Budget::new(Duration::from_millis(5), 1024)).with_secrets(secrets);
        let shown = format!("{call:?}");
        assert!(shown.contains("hooks/sign.ts#sign") && shown.contains("KEY"), "{shown}");
        assert!(!shown.contains("in-put") && !shown.contains("s3cr3t"), "{shown}");
        assert_eq!(call.budget.time, Duration::from_millis(5));
    }

    #[test]
    fn an_overrun_is_v5116_and_every_other_error_v5115() {
        let overran = HookError::Overran { limit: Limit::Memory, line: Some(4) };
        assert_eq!(overran.to_string(), "the hook ran past its memory budget and was stopped (line 4)");
        assert_eq!(overran.code(), Code::V5116);
        let threw = HookError::Threw { message: "no".into(), line: None };
        assert_eq!((threw.to_string().as_str(), threw.code()), ("the hook threw: no", Code::V5115));
        for other in [
            HookError::BadValue { what: "a function".into() },
            HookError::NotFound { hook: "a.ts#b".into() },
            HookError::Unavailable { reason: "x".into() },
        ] {
            assert_eq!(other.code(), Code::V5115, "{other}");
        }
        assert_eq!(Limit::Time.to_string(), "time");
    }
}
