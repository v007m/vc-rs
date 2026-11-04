use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use vc_interfaces::{
    CommandError, CommandOutput, CommandRunner, CommandSpec, Secret, SecretError, SecretProvider, SecretQuery, Secrets,
};

/// A [`SecretProvider`] holding the values it was given.
#[derive(Clone, Default)]
pub struct MemorySecrets {
    values: Vec<(String, Secret)>,
}

impl MemorySecrets {
    /// A provider resolving exactly `values`.
    #[must_use]
    pub fn new(values: &[(&str, &str)]) -> Self {
        Self { values: values.iter().map(|(name, value)| ((*name).to_owned(), Secret::new(*value))).collect() }
    }
}

impl fmt::Debug for MemorySecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.values.iter().map(|(name, _)| name)).finish()
    }
}

#[async_trait]
impl SecretProvider for MemorySecrets {
    async fn load(&self, query: &SecretQuery) -> Result<Secrets, SecretError> {
        let mut secrets = Secrets::new();
        for (name, secret) in self.values.iter().filter(|(name, _)| query.names.contains(name)) {
            secrets.insert(name.clone(), secret.clone());
        }
        Ok(secrets)
    }
}

/// What a [`FakeCommands`] answers for one spec; `None` means the program does not exist.
pub type Answer = dyn Fn(&CommandSpec) -> Option<Result<CommandOutput, CommandError>> + Send + Sync;

/// A [`CommandRunner`] that answers from a closure and counts its calls; runs nothing.
#[derive(Clone)]
pub struct FakeCommands {
    answer: Arc<Answer>,
    calls: Arc<AtomicUsize>,
}

impl FakeCommands {
    /// A runner answering each spec with `answer`.
    pub fn new(
        answer: impl Fn(&CommandSpec) -> Option<Result<CommandOutput, CommandError>> + Send + Sync + 'static,
    ) -> Self {
        Self { answer: Arc::new(answer), calls: Arc::new(AtomicUsize::new(0)) }
    }

    /// A runner where only `program` exists, always printing `stdout` and exiting 0.
    #[must_use]
    pub fn printing(program: &'static str, stdout: &'static str) -> Self {
        Self::new(move |spec| (spec.program == program).then(|| Ok(CommandOutput::new(Some(0), stdout))))
    }

    /// How many specs it was asked to run.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl fmt::Debug for FakeCommands {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeCommands").field("calls", &self.calls()).finish_non_exhaustive()
    }
}

#[async_trait]
impl CommandRunner for FakeCommands {
    async fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, CommandError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        (self.answer)(spec).unwrap_or_else(|| Err(CommandError::NotFound { program: spec.program.clone() }))
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{CommandSpec, SecretProvider};

    use super::{FakeCommands, MemorySecrets};
    use crate::{command_runner_suite, secret_provider_suite};

    #[tokio::test]
    async fn memory_secrets_meet_the_secret_provider_contract() {
        secret_provider_suite(&mut |values| Box::new(MemorySecrets::new(values)) as Box<dyn SecretProvider>).await;
        assert!(!format!("{:?}", MemorySecrets::new(&[("A", "s3cr3t")])).contains("s3cr3t"));
    }

    #[tokio::test]
    async fn fake_commands_meet_the_command_runner_contract() {
        let runner = FakeCommands::printing("echo", "hello\n");
        command_runner_suite(&runner, &CommandSpec::new("echo", ["hello"])).await;
        assert_eq!(runner.calls(), 2);
    }
}
