use std::time::{Duration, Instant};

use serde_json::json;
use vc_interfaces::{
    CALL_DEADLINE, DEADLINE_PASSED, PERMISSION_DENIED, PluginError, PluginHost, PluginKind, PluginManifest,
    PluginPermissions, Secret, Secrets,
};

/// The secret the suite's plugin declares.
const DECLARED: &str = "suiteToken";

/// A secret of the run the suite's plugin does not declare.
const UNDECLARED: &str = "otherKey";

/// How long the suite lets `suite/stall` run.
const SHORT: Duration = Duration::from_millis(200);

/// Builds the host under test for the suite's plugin, given the run's secrets.
pub type MakePluginHost<'a> = dyn FnMut(&Secrets) -> Box<dyn PluginHost> + Send + 'a;

/// The manifest of the plugin [`plugin_host_suite`] calls: `suitePlugin`, a reporter that
/// declares the secret `suiteToken` and nothing else.
#[must_use]
pub fn suite_plugin_manifest() -> PluginManifest {
    PluginManifest {
        id: "suitePlugin".to_owned(),
        version: "1.0.0".to_owned(),
        kind: PluginKind::Reporter,
        command: "suite-plugin".to_owned(),
        args: Vec::new(),
        permissions: PluginPermissions { secrets: vec![DECLARED.to_owned()], ..PluginPermissions::default() },
    }
}

/// The contract every [`PluginHost`] meets (D112).
///
/// `make` starts the suite's plugin, whose manifest is [`suite_plugin_manifest`]: it answers `suite/echo` with its params, answers
/// `suite/secret` with the secret `params.name` it asks the host for (passing on the host's
/// refusal), and never answers `suite/stall`. The run has `suiteToken` and `otherKey`.
///
/// # Panics
///
/// When the host breaks the contract.
pub async fn plugin_host_suite(make: &mut MakePluginHost<'_>) {
    let mut secrets = Secrets::new();
    secrets.insert(DECLARED, Secret::new("t0k3n-granted"));
    secrets.insert(UNDECLARED, Secret::new("n0t-for-plugins"));
    let host = make(&secrets);
    assert_eq!(host.manifest(), &suite_plugin_manifest(), "reports the manifest it checked");
    let params = json!({"a": [1, "b", null], "nested": {"x": true}});
    let echoed = host.call("suite/echo", params.clone(), CALL_DEADLINE).await;
    assert_eq!(echoed, Ok(params), "a call returns the plugin's result");
    let granted = host.call("suite/secret", json!({"name": DECLARED}), CALL_DEADLINE).await;
    assert_eq!(granted, Ok(json!("t0k3n-granted")), "gives a declared secret");
    refuses_what_is_not_declared(host.as_ref()).await;
    let unknown = host.call("suite/nothing", json!({}), CALL_DEADLINE).await.err();
    assert_eq!(unknown.as_ref().and_then(PluginError::rpc_code), Some(-32_601), "{unknown:?}");
    gives_up_at_the_deadline(host.as_ref()).await;
}

async fn gives_up_at_the_deadline(host: &dyn PluginHost) {
    let started = Instant::now();
    let stalled = host.call("suite/stall", json!({}), SHORT).await.err();
    assert_eq!(stalled.as_ref().and_then(PluginError::rpc_code), Some(DEADLINE_PASSED), "{stalled:?}");
    assert!(started.elapsed() < SHORT * 20, "gives up at the deadline, not long after");
}

async fn refuses_what_is_not_declared(host: &dyn PluginHost) {
    let refused = host.call("suite/secret", json!({"name": UNDECLARED}), CALL_DEADLINE).await;
    let error = refused.err();
    assert_eq!(error.as_ref().and_then(PluginError::rpc_code), Some(PERMISSION_DENIED), "{error:?}");
    let message = error.map(|error| (error.code().to_string(), error.to_string())).unwrap_or_default();
    assert_eq!(message.0, "V5118", "every plugin failure is V5118");
    assert!(!message.1.contains("n0t-for-plugins"), "a refusal never holds the secret: {}", message.1);
}
