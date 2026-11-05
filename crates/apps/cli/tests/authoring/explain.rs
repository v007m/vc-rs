//! `vc explain X` explains a diagnostic code when X matches `V\d{4}`, and otherwise shows the
//! api or test X resolved for the active profile (D88, D180).

use crate::{CONFIG, CRUD, Project, USERS, said};

#[test]
fn explains_a_code_with_a_wrong_and_a_right_example_outside_any_project() {
    let project = Project::empty();
    let output = project.vc(&["explain", "V2003"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(text.starts_with("V2003: Unknown api or test"), "{text}");
    assert!(text.contains("Wrong") && text.contains("Right") && text.contains("users#getUsr"), "{text}");
}

#[test]
fn explains_a_v4_code_too() {
    let output = Project::empty().vc(&["explain", "V4001"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Test ID breaks the convention"));
}

#[test]
fn an_unknown_code_is_refused() {
    let output = Project::empty().vc(&["explain", "V9999"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("V9999"), "{}", said(&output));
}

#[test]
fn shows_a_test_resolved_for_the_active_profile() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["explain", "users.crud#createsUser"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(text.contains("POST http://x/users"), "the final URL:\n{text}");
    assert!(text.contains("dev"), "the profile:\n{text}");
    assert!(text.contains("201"), "the merged expect:\n{text}");
    assert!(text.contains("users#remove"), "the cleanup it registers:\n{text}");
}

#[test]
fn shows_each_var_with_the_level_that_set_it() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["explain", "users#remove"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let user_id = text.lines().find(|line| line.contains("userId")).unwrap_or_else(|| panic!("{text}"));
    assert!(user_id.contains('0') && user_id.contains("api"), "{user_id}");
    let base = text.lines().find(|line| line.contains("baseUrl")).unwrap_or_else(|| panic!("{text}"));
    assert!(base.contains("http://x") && base.contains("vibranium.config.yaml"), "{base}");
}

#[test]
fn an_unknown_id_is_refused_with_the_closest_one() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["explain", "users.crud#listUsers"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users.crud#listsUsers"), "{}", said(&output));
}
