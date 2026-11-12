use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};
use vc_interfaces::{CommandError, CommandOutput, Secret, SecretError, SecretProvider, SecretQuery};
use vc_testkit::{FakeCommands, secret_provider_suite};

use super::VarlockSecrets;

fn json(values: &[(&str, &str)]) -> String {
    let items: Map<String, Value> =
        values.iter().map(|(name, value)| ((*name).to_owned(), Value::from(*value))).collect();
    Value::Object(items).to_string()
}

/// Varlock, answering only when it runs as a list in the root with exactly `env` added to vc's
/// own environment.
fn answering_with(env: &[(&str, &str)], stdout: String) -> FakeCommands {
    let env: Vec<(String, String)> =
        env.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect();
    FakeCommands::new(move |spec| {
        let as_list = spec.program == "varlock" && spec.args == ["load", "--format", "json"];
        let in_root = spec.cwd.as_deref() == Some(Path::new("/project"));
        (as_list && in_root && spec.env == env).then(|| Ok(CommandOutput::new(Some(0), stdout.clone())))
    })
}

fn answering(stdout: String) -> FakeCommands {
    answering_with(&[], stdout)
}

fn varlock(runner: FakeCommands) -> VarlockSecrets {
    VarlockSecrets::new(Arc::new(runner), "/project")
}

#[tokio::test]
async fn varlock_secrets_meet_the_secret_provider_contract() {
    secret_provider_suite(&mut |values| Box::new(varlock(answering(json(values)))) as Box<dyn SecretProvider>).await;
}

#[tokio::test]
async fn varlock_runs_once_per_run_without_a_shell() {
    let runner = answering(r#"{"API_TOKEN":"t","PORT":8080,"DEBUG":true,"OPTIONAL":null}"#.to_owned());
    let provider = varlock(runner.clone());
    let query = SecretQuery::new(["API_TOKEN", "PORT", "DEBUG", "OPTIONAL"]);
    let first = provider.load(&query).await.unwrap();
    let again = provider.clone().load(&SecretQuery::new(["API_TOKEN"])).await.unwrap();
    assert_eq!(runner.calls(), 1, "clones share the one run");
    assert_eq!(first.get("PORT").map(Secret::expose), Some("8080"));
    assert_eq!(first.get("DEBUG").map(Secret::expose), Some("true"));
    assert!(first.get("OPTIONAL").is_none());
    assert_eq!(again.len(), 1);
    assert!(!format!("{provider:?}").contains("API_TOKEN"));
}

#[tokio::test]
async fn varlock_is_given_the_run_s_profile_as_vc_profile_and_nothing_else() {
    let stdout = json(&[("TOKEN", "qa-token")]);
    let runner = answering_with(&[("VC_PROFILE", "qa")], stdout.clone());
    let provider = varlock(runner).for_profile(Some("qa".to_owned()));
    let secrets = provider.load(&SecretQuery::new(["TOKEN"])).await.unwrap();
    assert_eq!(secrets.get("TOKEN").map(Secret::expose), Some("qa-token"), "only VC_PROFILE is added");
    let unset = varlock(answering(stdout)).for_profile(None);
    assert_eq!(unset.load(&SecretQuery::new(["TOKEN"])).await.unwrap().len(), 1, "no profile: nothing is added");
    assert!(format!("{provider:?}").contains(r#"profile: Some("qa")"#), "{provider:?}");
}

#[tokio::test]
async fn a_missing_varlock_is_unavailable_with_its_help() {
    let error = varlock(FakeCommands::new(|_| None)).load(&SecretQuery::new(["A"])).await.unwrap_err();
    assert_eq!(error.to_string(), "the varlock secrets provider is unavailable: `varlock` is not on PATH");
    let timed_out = FakeCommands::new(|spec| {
        Some(Err(CommandError::TimedOut { program: spec.program.clone(), after: std::time::Duration::from_secs(1) }))
    });
    let error = varlock(timed_out).load(&SecretQuery::new(["A"])).await.unwrap_err();
    assert!(error.to_string().contains("did not finish within 1000ms"), "{error}");
}

#[tokio::test]
async fn a_failed_run_or_bad_output_never_echoes_the_output() {
    let failing = FakeCommands::new(|_| {
        let mut output = CommandOutput::new(Some(3), "{\"A\":\"s3cr3t\"}");
        output.stderr = b"A=s3cr3t is invalid".to_vec();
        Some(Ok(output))
    });
    let error = varlock(failing).load(&SecretQuery::new(["A"])).await.unwrap_err();
    assert!(matches!(&error, SecretError::Unavailable { reason, .. } if reason.contains("status 3")), "{error}");
    assert!(!error.to_string().contains("s3cr3t"));
    let signal = FakeCommands::new(|_| Some(Ok(CommandOutput::new(None, ""))));
    assert!(varlock(signal).load(&SecretQuery::new(["A"])).await.unwrap_err().to_string().contains("a signal"));
    let cut = FakeCommands::new(|_| Some(Ok(CommandOutput::new(Some(0), "{\"A\":\"s3cr3t\"}").capped(4))));
    let error = varlock(cut).load(&SecretQuery::new(["A"])).await.unwrap_err();
    assert!(matches!(&error, SecretError::Malformed { reason, .. } if reason.contains("output cap")), "{error}");
    for (stdout, reason) in
        [("s3cr3t", "stdout is not JSON"), ("[1]", "not a JSON object"), (r#"{"A":["s3cr3t"]}"#, "not text")]
    {
        let error = varlock(answering(stdout.to_owned())).load(&SecretQuery::new(["A"])).await.unwrap_err();
        assert!(matches!(&error, SecretError::Malformed { reason: got, .. } if got.contains(reason)), "{error}");
        assert!(!error.to_string().contains("s3cr3t"));
    }
}
