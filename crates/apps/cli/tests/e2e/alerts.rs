//! `vc alerts test` sends a sample notice through every channel of each alert rule whose
//! profiles match (D257): one line per channel, exit 1 when one did not take it, 2 when no
//! rule matches.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

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

#[test]
fn no_rule_for_the_profiles_exits_2() {
    let project = Project::new(&[]);
    let config =
        format!("{}alerts:\n  - profiles: [prod]\n    notify:\n      - webhook: {{url: 'http://x'}}\n", CONFIG.1);
    project.write(CONFIG.0, &config.replace("    default: true\n", "    default: true\n  - id: prod\n"));
    let output = project.vc(&["alerts", "test"]).assert().code(2).get_output().clone();
    assert!(said(&output).contains("no alert rule matches these profiles"), "{}", said(&output));
}
