//! `vc plugin add` from a folder asks before it installs, records what the plugin may reach in
//! `vibranium.lock`, and `vc plugin list` and `remove` read and undo it.

use std::fs;

use crate::{CONFIG, Project, said};

const MANIFEST: &str = concat!(
    "id: report\nversion: 1.0.0\nkind: reporter\n",
    "entry: {command: python3, args: [report.py]}\n",
    "permissions: {secrets: [reportToken]}\n",
);

#[test]
fn a_plugin_is_added_listed_and_removed() {
    let project =
        Project::new(&[CONFIG, ("plugin/vibranium-plugin.yaml", MANIFEST), ("plugin/report.py", "print()\n")]);
    let asked = project.vc(&["plugin", "add", "path:plugin"]).output().unwrap();
    assert_eq!(asked.status.code(), Some(2), "{}", said(&asked));
    assert!(said(&asked).contains("asks to reach: secret reportToken"), "{}", said(&asked));
    assert!(said(&asked).contains("pass --yes"), "{}", said(&asked));
    assert!(!project.path().join("vibranium.lock").exists());
    let added = project.vc(&["plugin", "add", "path:plugin", "--yes"]).output().unwrap();
    assert_eq!(added.status.code(), Some(0), "{}", said(&added));
    let lock = fs::read_to_string(project.path().join("vibranium.lock")).unwrap();
    assert!(lock.contains("      secrets: [reportToken]\n"), "{lock}");
    assert!(project.path().join(".vibranium/plugins/report/report.py").is_file());
    listed_then_removed(&project);
}

/// `vc check` and `vc plugin list` see the added plugin, and `vc plugin remove` undoes it.
fn listed_then_removed(project: &Project) {
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", said(&check));
    let listed = project.vc(&["plugin", "list"]).output().unwrap();
    let text = String::from_utf8_lossy(&listed.stdout).into_owned();
    assert_eq!(text, "report  reporter 1.0.0  path:plugin  ready  may reach: secret reportToken\n");
    let removed = project.vc(&["plugin", "remove", "report"]).output().unwrap();
    assert_eq!(removed.status.code(), Some(0), "{}", said(&removed));
    let config = fs::read_to_string(project.path().join(CONFIG.0)).unwrap();
    assert_eq!(config.trim_end(), CONFIG.1.trim_end());
    assert!(!project.path().join(".vibranium/plugins/report").exists());
}
