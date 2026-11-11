use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    let data = root.path().join(".vibranium/plugin-data/suitePlugin");
    let host =
        StdioPluginHost::new(manifest, suite_dir(), granted()).data(root.path(), ".vibranium/plugin-data/suitePlugin");
    let kept = host.call("suite/keep", json!({}), Duration::from_secs(5)).await.unwrap();
    assert_eq!(kept, json!(data.to_string_lossy()));
    assert_eq!(std::fs::read_to_string(data.join("state.txt")).unwrap(), "kept");
}

#[cfg(unix)]
#[tokio::test]
async fn a_data_folder_reached_through_a_link_is_refused_and_the_plugin_never_starts() {
    let (root, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join(".vibranium")).unwrap();
    let host = StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), granted())
        .data(root.path(), ".vibranium/plugin-data/suitePlugin");
    let refused = host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err();
    let PluginError::Start { reason, .. } = refused else { panic!("{refused:?}") };
    assert!(reason.starts_with("cannot make its data folder: `"), "{reason}");
    assert!(reason.ends_with("/.vibranium` is a link or not a folder"), "{reason}");
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing is made through the link");
}

#[tokio::test]
async fn a_confined_plugin_naming_a_file_outside_the_project_never_starts() {
    let root = tempfile::tempdir().unwrap();
    let mut manifest = suite_plugin_manifest();
    manifest.permissions.files = vec!["../elsewhere".into()];
    let granted = manifest.permissions.clone();
    let host = StdioPluginHost::new(manifest, suite_dir(), granted).confined(root.path(), Arc::default());
    let refused = host.call("suite/echo", json!({}), CALL_DEADLINE).await.unwrap_err();
    let PluginError::Start { reason, .. } = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "its file permission `../elsewhere` is not a path inside the project");
}

#[test]
fn a_plugin_that_cannot_be_confined_is_refused_on_macos_and_elsewhere_runs_unconfined_and_says_so() {
    let (told, mut heard) = tokio::sync::broadcast::channel(4);
    let host = StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), granted()).told(told);
    let entry = (PathBuf::from("python3"), Vec::new());
    let first = host.unconfined(entry.clone(), "no sandbox".to_owned());
    if crate::confine::FAIL_CLOSED {
        let Err(PluginError::Unconfined { plugin, reason }) = &first else { panic!("{first:?}") };
        assert_eq!((plugin.as_str(), reason.as_str()), ("suitePlugin", "no sandbox"));
        assert!(heard.try_recv().is_err(), "a plugin never started says nothing");
    } else {
        assert_eq!(first.unwrap(), entry);
        assert_eq!(heard.try_recv().unwrap(), "plugin `suitePlugin` runs unconfined: no sandbox");
    }
    let allowed = host.allow_unconfined(true).unconfined(entry.clone(), "no sandbox".to_owned());
    assert_eq!(allowed.unwrap(), entry);
    assert_eq!(heard.try_recv().unwrap(), "plugin `suitePlugin` runs unconfined: no sandbox");
}

#[test]
fn a_file_a_confined_plugin_cannot_make_is_named_with_the_remedy() {
    let (told, mut heard) = tokio::sync::broadcast::channel(4);
    let host = StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), granted()).told(told);
    let root = Path::new("/work/shop");
    host.cannot_make(root, &[root.join("out.json")]);
    let said = heard.try_recv().unwrap();
    let expected = "plugin `suitePlugin` cannot make `out.json`, which it may write, until it exists: create the \
                    file first, or grant a path in a folder of the project";
    assert_eq!(said, expected);
    assert!(heard.try_recv().is_err(), "one line a file");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_confined_plugin_without_a_launcher_runs_unconfined_and_says_so_once() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let (root, (told, mut heard)) = (tempfile::tempdir().unwrap(), tokio::sync::broadcast::channel(4));
    let mut secrets = Secrets::new();
    secrets.insert("suiteToken", Secret::new("t0k3n"));
    let host = StdioPluginHost::new(suite_plugin_manifest(), suite_dir(), granted())
        .secrets(&secrets)
        .told(told)
        .confined(root.path(), Arc::default());
    for _ in 0..2 {
        host.call("suite/echo", json!({"said": "hi"}), Duration::from_secs(5)).await.unwrap();
    }
    let said = heard.try_recv().unwrap();
    assert!(said.starts_with("plugin `suitePlugin` runs unconfined: `vc` cannot find its own program"), "{said}");
    assert!(heard.try_recv().is_err(), "one warning a run");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_confined_plugin_writes_its_data_folder_and_nothing_else_in_the_project() {
    let seatbelt = std::process::Command::new("/usr/bin/sandbox-exec")
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .status()
        .is_ok_and(|status| status.success());
    if !python() || !seatbelt {
        return eprintln!("skipped: python3 is not installed, or sandbox-exec cannot apply a profile here");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("{READ}{LEAKS}")],
        ..suite_plugin_manifest()
    };
    let root = tempfile::tempdir().unwrap();
    let data = root.path().join(".vibranium/plugin-data/suitePlugin");
    let host = StdioPluginHost::new(manifest, suite_dir(), granted())
        .data(root.path(), ".vibranium/plugin-data/suitePlugin")
        .confined(root.path(), Arc::default());
    host.call("suite/keep", json!({}), Duration::from_secs(5)).await.unwrap();
    assert_eq!(std::fs::read_to_string(data.join("state.txt")).unwrap(), "kept");
    assert!(!root.path().join("leaked.txt").exists(), "the project outside its data folder is closed");
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

/// Answers `initialize`, then writes a file in its data folder and tries one beside the
/// project's `.vibranium` folder, three folders up.
#[cfg(target_os = "macos")]
const LEAKS: &str = r#"
import os
for _ in range(2):
    request = read()
    if request["method"] == "suite/keep":
        data = os.environ["VIB_PLUGIN_DATA"]
        open(os.path.join(data, "state.txt"), "w").write("kept")
        try: open(os.path.join(data, "..", "..", "..", "leaked.txt"), "w").write("no")
        except OSError: pass
    send(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": None}).encode())
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
            .data(data.path(), "")
            .stderr_log(logs.path(), "");
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
    let host = StdioPluginHost::new(writes_to_stderr("noise"), suite_dir(), granted()).stderr_log(logs.path(), "");
    host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    logged(&logs.path().join(crate::STDERR_LOG), "noise").await;
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep me");
}

#[cfg(unix)]
#[tokio::test]
async fn a_log_that_cannot_be_made_drops_the_stderr_and_says_why_as_the_plugin_starts() {
    let (logs, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), logs.path().join("suite")).unwrap();
    let (told, mut heard) = tokio::sync::broadcast::channel(4);
    let host = StdioPluginHost::new(writes_to_stderr("noise"), suite_dir(), granted())
        .stderr_log(logs.path(), "suite")
        .told(told);
    assert!(heard.try_recv().is_err(), "nothing is said before the plugin starts");
    host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap_err();
    let said = heard.try_recv().unwrap();
    assert!(said.starts_with("plugin `") && said.contains("`: its stderr is dropped: `"), "{said}");
    assert!(said.ends_with("` is a link or not a folder"), "{said}");
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing is made through the link");
}

/// Answers every call; after answering `shutdown`, waits for its input to close, then writes
/// `stopped.txt` in its data folder and exits.
const STOPS: &str = r#"
import os
while True:
    request = read()
    send(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": None}).encode())
    if request["method"] == "shutdown": break
while sys.stdin.buffer.read(1): pass
time.sleep(0.2)
open(os.path.join(os.environ["VIB_PLUGIN_DATA"], "stopped.txt"), "w").write("stopped")
"#;

#[cfg(unix)]
#[tokio::test]
async fn shutdown_closes_the_plugin_s_input_and_returns_once_it_has_exited() {
    if !python() {
        return eprintln!("skipped: python3 is not installed");
    }
    let manifest = vc_interfaces::PluginManifest {
        command: "python3".into(),
        args: vec!["-c".into(), format!("{READ}{STOPS}")],
        ..suite_plugin_manifest()
    };
    let root = tempfile::tempdir().unwrap();
    let host = StdioPluginHost::new(manifest, suite_dir(), granted()).data(root.path(), "data");
    host.call("suite/echo", json!({}), Duration::from_secs(5)).await.unwrap();
    host.shutdown().await;
    assert_eq!(std::fs::read_to_string(root.path().join("data/stopped.txt")).unwrap(), "stopped");
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
