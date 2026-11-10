use std::path::{Path, PathBuf};
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

#[cfg(unix)]
#[tokio::test]
async fn a_plugin_that_breaks_the_protocol_is_stopped_and_later_calls_fail_at_once() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("{READ}{BREAKS}")],
        ..suite_plugin_manifest()
    };
    let host = StdioPluginHost::new(manifest, suite_dir(), granted());
    let broken = host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    assert!(matches!(broken, PluginError::Protocol { .. }), "{broken:?}");
    let started = std::time::Instant::now();
    let later = host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    assert_eq!(later.to_string(), "plugin `suitePlugin` stopped: it stopped earlier in this run");
    assert!(started.elapsed() < Duration::from_secs(1), "a stopped plugin fails at once");
}

#[cfg(unix)]
#[tokio::test]
async fn a_plugin_that_stops_reading_its_input_is_stopped_and_later_calls_fail_at_once() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("{READ}{STALLS}")],
        ..suite_plugin_manifest()
    };
    let host = StdioPluginHost::new(manifest, suite_dir(), granted());
    let large = json!({"blob": "x".repeat(1 << 20)});
    let stalled = host.call("suite/echo", large, Duration::from_secs(1)).await.unwrap_err();
    assert!(matches!(stalled, PluginError::Deadline { .. } | PluginError::Crashed { .. }), "{stalled:?}");
    let started = std::time::Instant::now();
    let later = host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    assert!(matches!(later, PluginError::Crashed { .. }), "{later:?}");
    assert!(started.elapsed() < Duration::from_secs(1), "a stopped plugin fails at once");
}

#[cfg(unix)]
#[tokio::test]
async fn a_plugin_keeps_its_own_files_in_the_data_folder_it_is_given() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("{READ}{KEEPS}")],
        ..suite_plugin_manifest()
    };
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join("plugin-data/suitePlugin");
    let host = StdioPluginHost::new(manifest, suite_dir(), granted()).data(&data);
    let kept = host.call("suite/keep", json!({}), Duration::from_secs(5)).await.unwrap();
    assert_eq!(kept, json!(data.to_string_lossy()));
    assert_eq!(std::fs::read_to_string(data.join("state.txt")).unwrap(), "kept");
}

/// Reads one framed message from stdin.
const READ: &str = r#"
import json, sys, time
def read():
    length = 0
    while True:
        line = sys.stdin.buffer.readline().strip()
        if not line: break
        if line.lower().startswith(b"content-length:"): length = int(line.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(length))
def send(body):
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n%s" % (len(body), body)); sys.stdout.buffer.flush()
"#;

/// Answers `initialize`, answers the next call with a message that is not an object, then hangs.
const BREAKS: &str = r#"
first = read()
send(json.dumps({"jsonrpc": "2.0", "id": first["id"], "result": None}).encode())
read()
send(b"[1, 2]")
time.sleep(30)
"#;

/// Answers `initialize`, then writes a file in its data folder and answers with the folder.
const KEEPS: &str = r#"
import os
for _ in range(2):
    request = read()
    data = os.environ.get("VIB_PLUGIN_DATA", "")
    if request["method"] == "suite/keep":
        open(os.path.join(data, "state.txt"), "w").write("kept")
    send(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": data}).encode())
"#;

/// Answers `initialize`, then never reads its input again.
const STALLS: &str = r#"
first = read()
send(json.dumps({"jsonrpc": "2.0", "id": first["id"], "result": None}).encode())
time.sleep(30)
"#;

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

/// The suite's plugin, but writing `word` to its stderr and exiting.
fn writes_to_stderr(word: &str) -> vc_interfaces::PluginManifest {
    vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("import sys; sys.stderr.write('{word}')")],
        ..suite_plugin_manifest()
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_logged_stderr_goes_to_the_log_folder_and_the_log_before_is_kept() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let (data, logs) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for word in ["first", "second"] {
        let host = StdioPluginHost::new(writes_to_stderr(word), suite_dir(), granted())
            .data(data.path())
            .stderr_log(logs.path());
        host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
        logged(&logs.path().join(crate::STDERR_LOG), word).await;
    }
    assert_eq!(std::fs::read_to_string(logs.path().join("stderr.log.1")).unwrap(), "first");
    assert!(!data.path().join(crate::STDERR_LOG).exists(), "the plugin's own folder holds no log");
}

#[cfg(unix)]
#[tokio::test]
async fn a_link_planted_at_the_log_is_never_followed() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let (logs, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let target = elsewhere.path().join("notes.txt");
    std::fs::write(&target, "keep me").unwrap();
    for name in [crate::STDERR_LOG, "stderr.log.1"] {
        std::os::unix::fs::symlink(&target, logs.path().join(name)).unwrap();
    }
    let host = StdioPluginHost::new(writes_to_stderr("noise"), suite_dir(), granted()).stderr_log(logs.path());
    host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    logged(&logs.path().join(crate::STDERR_LOG), "noise").await;
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
}

/// Waits up to five seconds for `log` to hold `text`.
async fn logged(log: &Path, text: &str) {
    for _ in 0..100 {
        if std::fs::read_to_string(log).is_ok_and(|logged| logged == text) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{} never held {text}", log.display());
}
