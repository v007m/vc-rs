use vc_interfaces::{CommandError, CommandRunner, CommandSpec, Secret, SecretProvider, SecretQuery, Secrets};

/// Builds a provider that resolves exactly these names to these values.
pub type MakeSecretProvider<'a> = dyn FnMut(&[(&str, &str)]) -> Box<dyn SecretProvider> + Send + 'a;

/// The contract every [`SecretProvider`] meets (D112). `make` builds one resolving exactly the
/// pairs given.
///
/// # Panics
///
/// When the provider breaks the contract.
pub async fn secret_provider_suite(make: &mut MakeSecretProvider<'_>) {
    let provider = make(&[("API_TOKEN", "s3cr3t-token"), ("EMPTY", "")]);
    let query = SecretQuery::new(["API_TOKEN", "EMPTY", "MISSING"]);
    let loaded = provider.load(&query).await;
    assert!(loaded.is_ok(), "loading failed: {loaded:?}");
    let secrets = &loaded.unwrap_or_default();
    assert_resolved(secrets);
    assert_eq!(provider.load(&query).await.as_ref().ok(), Some(secrets), "loading again gives the same values");
    let narrow = provider.load(&SecretQuery::new(["EMPTY"])).await;
    assert!(narrow.is_ok_and(|secrets| secrets.get("API_TOKEN").is_none()), "only the names asked for");
}

/// Checks the suite's names resolved as given, and that `Debug` hides the values.
fn assert_resolved(secrets: &Secrets) {
    assert_eq!(secrets.get("API_TOKEN").map(Secret::expose), Some("s3cr3t-token"));
    assert_eq!(secrets.get("EMPTY").map(Secret::expose), Some(""), "an empty value is still set");
    assert!(secrets.get("MISSING").is_none(), "a name it cannot resolve is left out");
    assert!(!format!("{secrets:?}").contains("s3cr3t-token"), "Debug never shows a value");
}

/// The contract every [`CommandRunner`] meets (D112). `prints_hello` is a spec that writes
/// `hello` to stdout and exits 0.
///
/// # Panics
///
/// When the runner breaks the contract.
pub async fn command_runner_suite(runner: &dyn CommandRunner, prints_hello: &CommandSpec) {
    let ran = runner.run(prints_hello).await;
    assert!(ran.is_ok(), "running {prints_hello:?} failed: {ran:?}");
    let output = ran.unwrap_or_default();
    assert!(output.success(), "exits 0: {output:?}");
    assert!(output.stdout.starts_with(b"hello"), "stdout is collected: {output:?}");
    let missing = CommandSpec::new("vc-surely-not-a-program-9f2c", ["x"]);
    assert!(
        matches!(runner.run(&missing).await, Err(CommandError::NotFound { program }) if program == missing.program),
        "a missing program is NotFound"
    );
}
