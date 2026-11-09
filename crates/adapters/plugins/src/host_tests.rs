use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use vc_interfaces::{CALL_DEADLINE, PluginError, PluginHost, PluginPermissions, Secret, Secrets};
use vc_testkit::{plugin_host_suite, suite_plugin_manifest};

use super::StdioPluginHost;

/// The folder of the suite's plugin, a Python script with the standard library only.
fn suite_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/suite")
}

/// Whether `python3` runs here; the tests that start the fixture skip without it.
fn python() -> bool {
    std::process::Command::new("python3").arg("--version").output().is_ok_and(|output| output.status.success())
}

fn granted() -> PluginPermissions {
    suite_plugin_manifest().permissions
}

#[cfg(unix)]
#[tokio::test]
async fn the_stdio_host_meets_the_contract() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    plugin_host_suite(&mut |secrets| {
        Box::new(StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), granted()).secrets(secrets))
            as Box<dyn PluginHost>
    })
    .await;
}

#[tokio::test]
async fn a_manifest_asking_beyond_the_lock_never_starts() {
    let host = StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), PluginPermissions::default());
    let refused = host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err();
    assert_eq!(
        refused.to_string(),
        "plugin `suitePlugin` was refused secret suiteToken, which vibranium.lock does not grant"
    );
}

#[tokio::test]
async fn a_plugin_that_cannot_start_fails_every_call() {
    let manifest =
        vc_interfaces::PluginManifest { command: "no-such-plugin-program".into(), ..suite_plugin_manifest() };
    let host = StdioPluginHost::new(manifest, suite_dir(), granted());
    let first = host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err();
    assert!(matches!(first, PluginError::Start { .. }), "{first:?}");
    assert!(first.to_string().contains("cannot run `no-such-plugin-program`"), "{first}");
    assert_eq!(host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err(), first);
}

#[cfg(unix)]
#[tokio::test]
async fn a_plugin_that_exits_fails_the_call_it_left() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), EXITS.into()],
        ..suite_plugin_manifest()
    };
    let mut secrets = Secrets::new();
    secrets.insert("suiteToken", Secret::new("t0k3n"));
    let host = StdioPluginHost::new(manifest, suite_dir(), granted()).secrets(&secrets);
    let stopped = host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    assert!(matches!(stopped, PluginError::Crashed { .. }), "{stopped:?}");
    assert!(stopped.to_string().contains("exit"), "{stopped}");
    let later = host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err();
    assert_eq!(later.to_string(), "plugin `suitePlugin` stopped: it stopped earlier in this run");
}

/// Answers `initialize`, then exits on the next call.
const EXITS: &str = r#"
import json, sys
def read():
    length = 0
    while True:
        line = sys.stdin.buffer.readline().strip()
        if not line: break
        if line.lower().startswith(b"content-length:"): length = int(line.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(length))
first = read()
body = json.dumps({"jsonrpc": "2.0", "id": first["id"], "result": None}).encode()
sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n%s" % (len(body), body)); sys.stdout.buffer.flush()
read()
sys.exit(3)
"#;
