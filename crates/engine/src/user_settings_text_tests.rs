use super::{HEADER, edited};

fn made(before: Option<&str>, edits: &[(&str, Option<&str>)]) -> String {
    edited(before, edits).unwrap()
}

#[test]
fn a_new_file_starts_with_its_comment_and_takes_nested_settings() {
    let text = made(None, &[("defaults.timeout", Some("12s")), ("defaults.retry.attempts", Some("2"))]);
    assert_eq!(text, format!("{HEADER}defaults:\n  timeout: 12s\n  retry:\n    attempts: 2\n"));
}

#[test]
fn a_set_keeps_the_comments_and_replaces_a_value_in_place() {
    let before = "# mine\ndefaults:\n  timeout: 5s # short\nvars:\n  who: me\n";
    let text = made(Some(before), &[("defaults.timeout", Some("12s")), ("vars.region", Some("007"))]);
    assert_eq!(text, "# mine\ndefaults:\n  timeout: 12s # short\nvars:\n  who: me\n  region: \"007\"\n");
}

#[test]
fn a_list_setting_with_several_values_is_written_as_a_list_over_a_scalar() {
    let text = made(Some("reports:\n  formats: ctrf\n"), &[("reports.formats", Some("ctrf, html"))]);
    assert_eq!(text, "reports:\n  formats:\n    - ctrf\n    - html\n");
    let back = made(Some(&text), &[("reports.formats", Some("junit"))]);
    assert_eq!(back, "reports:\n  formats: junit\n");
}

#[test]
fn a_removal_takes_the_maps_it_leaves_empty_and_a_missing_key_is_no_change() {
    let before = "profile: dev\ndefaults:\n  retry:\n    attempts: 2\n";
    assert_eq!(made(Some(before), &[("defaults.retry.attempts", None)]), "profile: dev\n");
    assert_eq!(made(Some(before), &[("vars.none", None)]), before);
    let kept = "defaults:\n  timeout: 5s\n  retry:\n    attempts: 2\n";
    assert_eq!(made(Some(kept), &[("defaults.retry.attempts", None)]), "defaults:\n  timeout: 5s\n");
}

#[test]
fn a_file_of_only_comments_takes_the_setting_after_them() {
    assert_eq!(made(Some("# empty"), &[("profile", Some("dev"))]), "# empty\nprofile: dev\n");
}

#[test]
fn a_file_that_is_not_a_map_of_settings_is_refused_without_echoing_the_value() {
    for before in ["- a\n- b\n", "just text\n", "defaults: [unclosed\n", "defaults: 5\n"] {
        let refused = edited(Some(before), &[("defaults.timeout", Some("sekrit-9s"))]).unwrap_err();
        assert!(!refused.to_string().contains("sekrit"), "{before}: {refused}");
    }
}
