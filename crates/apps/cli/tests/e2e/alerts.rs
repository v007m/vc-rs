//! `vc alerts test` sends a sample notice through every channel of each alert rule whose
//! profiles match (D257): one line per channel, exit 1 when one did not take it, 2 when no
//! rule matches.

use std::process::Stdio;

use assert_cmd::cargo::cargo_bin;
use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// A project whose one alert rule sends to `{base}/echo`, which takes every notice.
fn echoing(base: &str) -> Project {
    let alerts = format!("alerts:\n  - on: [recovery]\n    notify:\n      - webhook: {{url: '{base}/echo'}}\n");
    let project = Project::new(&[]);
    project.write(CONFIG.0, &format!("{}{alerts}", CONFIG.1));
    project
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_the_sample_to_each_channel_and_exits_1_when_one_refuses() {
    let server = MockServer::start().await.unwrap();
    let base = server.base_url();
    let alerts = format!(
        "alerts:\n  - on: [recovery]\n    notify:\n      - webhook: {{url: '{base}/echo'}}\n      - slack: {{webhook: '{base}/status/500'}}\n"
    );
    let project = Project::new(&[]);
    project.write(CONFIG.0, &format!("{}{alerts}", CONFIG.1));
    let output = project.vc(&["alerts", "test"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "✓ webhook (rule 1)");
    assert!(lines[1].starts_with("✗ slack (rule 1): ") && lines[1].contains("HTTP 500"), "{stdout}");
    assert!(!stdout.contains("/status/500"), "a channel's URL is never shown: {stdout}");
}

/// `vc alerts test` into a pipe whose reader has gone: the channel took the notice, so it exits
/// 0 and says nothing, though no one reads the line it writes.
#[tokio::test(flavor = "multi_thread")]
async fn a_reader_that_has_gone_ends_the_lines_quietly() {
    let server = MockServer::start().await.unwrap();
    let project = echoing(&server.base_url());
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let mut vc = std::process::Command::new(cargo_bin!("vc"));
    vc.current_dir(project.path()).args(["alerts", "test"]).env("NO_COLOR", "1");
    vc.env("HOME", project.home()).env("XDG_CONFIG_HOME", project.home());
    let output = vc.stdout(Stdio::from(writer)).stderr(Stdio::piped()).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn no_rule_for_the_profiles_exits_2() {
    let project = Project::new(&[]);
    let config =
        format!("{}alerts:\n  - profiles: [prod]\n    notify:\n      - webhook: {{url: 'http://x'}}\n", CONFIG.1);
    project.write(CONFIG.0, &config.replace("    default: true\n", "    default: true\n  - id: prod\n"));
    let output = project.vc(&["alerts", "test"]).assert().code(2).get_output().clone();
    assert!(said(&output).contains("no alert rule matches these profiles"), "{}", said(&output));
}
