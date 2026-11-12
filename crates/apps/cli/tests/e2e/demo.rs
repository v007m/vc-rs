//! `vc demo stop` with no demo running is not an error and names the port (D175, D188); `vc demo
//! start` without `vc-demo` says where it looked and how to install it.

use crate::port::free_port;
use crate::{CONFIG, Project, alone, said};

#[test]
fn stopping_no_demo_exits_0_and_names_the_port() {
    let (project, port) = (Project::new(&[CONFIG]), free_port().to_string());
    let said = format!("no demo running on port {port}\n");
    project.vc(&["demo", "stop", "--port", &port]).assert().code(0).stdout(said);
}

#[test]
fn starting_without_vc_demo_names_where_it_looked_and_the_install_command() {
    let project = Project::new(&[CONFIG]);
    let (folder, vc) = alone("vc");
    let empty = tempfile::tempdir().unwrap();
    let mut command = assert_cmd::Command::new(vc);
    command.current_dir(project.path()).env("HOME", project.home()).env("PATH", empty.path());
    let output = command.args(["demo", "start", "--port", &free_port().to_string()]).output().unwrap();
    let text = said(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    let name = folder.path().file_name().unwrap().to_string_lossy().into_owned();
    assert!(text.contains("error: `vc-demo` is not next to vc (in ") && text.contains(") or on PATH"), "{text}");
    assert!(text.lines().next().is_some_and(|line| line.contains(&name)), "names the folder of vc:\n{text}");
    let install = "help: install it with `cargo install --git https://github.com/v007m/vc-rs vc-demo --locked`";
    assert!(text.contains(install), "{text}");
}
