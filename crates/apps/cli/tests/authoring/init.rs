//! `vc init` writes a project that checks clean and runs green offline, against the built-in
//! demo service it starts and stops itself (D89, D175).

use crate::{Project, checks_clean, said, schema_line};

/// The files `vc init` writes, and the kind of schema each one's first line names.
const WRITTEN: [(&str, &str); 3] = [
    ("vibranium.config.yaml", "config"),
    ("services/sample/sample.api.yaml", "api"),
    ("services/sample/basics.test.yaml", "test"),
];

const FOLDERS: [&str; 5] = ["schemas", "datasets", "hooks", "openapi", "load"];

/// The demo port the sample uses; only [`a_fresh_project_runs_green_offline`] starts it.
const SAMPLE_PORT: &str = "7421";

#[test]
fn writes_the_documented_files_each_starting_with_its_schema_line() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    for (path, kind) in WRITTEN {
        let text = project.read(path);
        assert_eq!(text.lines().next(), Some(schema_line(kind).as_str()), "{path}");
    }
    for folder in FOLDERS {
        assert!(project.path().join(folder).is_dir(), "{folder}/");
    }
    assert_eq!(project.read(".gitignore"), ".vibranium/\n.env\n", "generated data and secrets stay out of git");
    assert!(project.exists(".env.schema"));
}

/// D286: an existing `.gitignore` keeps its lines and gains only the ones it lacks.
#[test]
fn an_existing_gitignore_gains_only_the_lines_it_lacks() {
    let project = Project::new(&[(".gitignore", "target\n/.vibranium/\n")]);
    let output = project.vc(&["init", "--no-sample"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(project.read(".gitignore"), "target\n/.vibranium/\n.env\n");
    project.vc(&["init", "--no-sample"]).assert().code(2);
    let ignored = Project::new(&[(".gitignore", ".env\n.vibranium/\n")]);
    ignored.vc(&["init", "--no-sample"]).assert().code(0);
    assert_eq!(ignored.read(".gitignore"), ".env\n.vibranium/\n", "nothing to add");
}

#[test]
fn the_config_has_a_default_profile_and_the_sample_url() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    let config = project.read("vibranium.config.yaml");
    for line in ["syntaxVersion: 2", "  - id: default", "  sampleUrl: http://localhost:7421"] {
        assert!(config.lines().any(|l| l.trim_end().starts_with(line)), "missing `{line}`:\n{config}");
    }
}

#[test]
fn the_sample_has_four_apis_and_four_tests() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    let apis = project.read("services/sample/sample.api.yaml");
    for id in ["listUsers", "createUser", "getUser", "deleteUser"] {
        assert!(apis.contains(&format!("  - id: {id}\n")), "{id}:\n{apis}");
    }
    let tests = project.read("services/sample/basics.test.yaml");
    for id in ["listsUsers", "createsUser", "getsCreatedUser", "rejectsMissingEmail"] {
        assert!(tests.contains(&format!("  - id: {id}\n")), "{id}:\n{tests}");
    }
    assert!(tests.contains("vc demo start --port 7421 --detach") && tests.contains("vc demo stop --port 7421"));
}

#[test]
fn a_fresh_project_checks_clean() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    checks_clean(&project);
}

#[test]
fn a_fresh_project_runs_green_offline() {
    let project = Project::empty();
    project.vc(&["init"]).assert().code(0);
    let output = project.vc(&["run"]).output().unwrap();
    project.vc(&["demo", "stop", "--port", SAMPLE_PORT]).output().unwrap();
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("4 passed"), "{text}");
}

#[test]
fn no_sample_writes_the_config_and_folders_only() {
    let project = Project::empty();
    project.vc(&["init", "--no-sample"]).assert().code(0);
    assert!(project.exists("vibranium.config.yaml") && project.path().join("load").is_dir());
    assert!(!project.exists("services/sample"), "no sample");
    let config = project.read("vibranium.config.yaml");
    assert!(!config.contains("sample"), "no sample var or comment:\n{config}");
    project.vc(&["check"]).assert().code(0).stdout("0 errors · 0 warnings · 1 file checked\n");
    project.vc(&["fmt"]).assert().code(0);
    assert_eq!(project.read("vibranium.config.yaml"), config, "already in the house style");
}

#[test]
fn leaves_an_existing_project_alone() {
    let mine = "syntaxVersion: 2\nprofiles:\n  - id: dev  # mine\n";
    let project = Project::new(&[("vibranium.config.yaml", mine)]);
    let output = project.vc(&["init"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("vibranium.config.yaml"), "names the file it would overwrite");
    assert_eq!(project.read("vibranium.config.yaml"), mine);
    assert!(!project.exists("services/sample"));
}
