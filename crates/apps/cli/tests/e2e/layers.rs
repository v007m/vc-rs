//! The config layers (D129, D182): the user config under the project's files, then profiles <
//! `.env` < `VIB_*` < `--set`, and `.env` under the process environment for `${env.X}`.

use std::fs;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED};

const LAYERED: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n    vars: {a: profile, b: profile, c: profile, d: profile}\n\
     vars: {baseUrl: 'http://x', a: config, b: config, c: config, d: config}\n",
);

const ECHOES: (&str, &str) = (
    "services/scripted/layers.test.yaml",
    "tests:\n  - id: seesEveryLayer\n    call: scripted#echo\n    body:\n\
     \x20     a: ${var.a}\n      b: ${var.b}\n      c: ${var.c}\n      d: ${var.d}\n\
     \x20     fromFile: ${env.FROM_FILE}\n      shadowed: ${env.SHADOWED}\n\
     \x20   expect:\n      body:\n        match:\n          body:\n\
     \x20           {a: profile, b: dotenv, c: vib, d: set, fromFile: file, shadowed: process}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn each_layer_overrides_the_one_below() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[LAYERED, SCRIPTED, ECHOES]);
    project.write(".env", "VIB_VAR_b=dotenv\nVIB_VAR_c=dotenv\nVIB_VAR_d=dotenv\nFROM_FILE=file\nSHADOWED=file\n");
    let output = project
        .run(&server.base_url(), &["--set", "vars.d=set"])
        .env("VIB_VAR_c", "vib")
        .env("VIB_VAR_d", "vib")
        .env("SHADOWED", "process")
        .assert()
        .get_output()
        .clone();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_user_config_counts_only_where_the_project_is_silent() {
    let server = MockServer::start().await.unwrap();
    let test = ("services/scripted/echo.test.yaml", "tests:\n  - id: echoes\n    call: scripted#echo\n");
    let project = Project::new(&[CONFIG, SCRIPTED, test]);
    let user = project.home().join("vibranium");
    fs::create_dir_all(&user).unwrap();
    fs::write(user.join("config.yaml"), "reports:\n  dir: from-user\n  formats: [junit]\nvars:\n  word: user\n")
        .unwrap();
    project.run(&server.base_url(), &[]).assert().code(0);
    assert!(project.path().join("from-user/1/junit.xml").is_file(), "the user's reports setting applies");
    project.run(&server.base_url(), &[]).env("VIB_REPORTS_DIR", "from-vib").assert().code(0);
    assert!(project.path().join("from-vib/2/junit.xml").is_file(), "`VIB_REPORTS_DIR` beats the user config");
}

#[test]
fn a_broken_user_config_is_reported_with_its_path() {
    let project = Project::new(&[CONFIG, SCRIPTED]);
    let user = project.home().join("vibranium");
    fs::create_dir_all(&user).unwrap();
    fs::write(user.join("config.yaml"), "reports: [unclosed\n").unwrap();
    let output = project.vc(&["plan"]).assert().code(2).get_output().clone();
    assert!(crate::said(&output).contains("config.yaml"), "{}", crate::said(&output));
}
