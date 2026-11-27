use vc_interfaces::ExchangeRow;

use super::{Retyped, ShapeChange, shape_changes};

/// Exchanges that answered `api` with `status` and each of `bodies`.
fn answered(api: &str, status: u16, bodies: &[&str]) -> Vec<ExchangeRow> {
    let exchange = |body: &&str| ExchangeRow {
        api_id: Some(api.to_owned()),
        status: Some(status),
        response_body: Some(body.as_bytes().to_vec()),
        ..ExchangeRow::default()
    };
    bodies.iter().map(exchange).collect()
}

fn paths(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
fn finds_the_paths_added_removed_and_retyped_in_nested_objects_and_arrays() {
    let before = answered("users#list", 200, &[r#"{"users": [{"id": 1, "tags": ["a"]}], "next": null}"#]);
    let after = answered("users#list", 200, &[r#"{"users": [{"id": 1, "tags": [{"k": 1}], "role": "x"}]}"#]);
    let [change] = shape_changes((&before, &after)).try_into().unwrap();
    let ShapeChange { api_id, status, added, removed, retyped, .. } = change;
    assert_eq!((api_id.as_str(), status), ("users#list", 200));
    assert_eq!(added, paths(&["/users/*/role", "/users/*/tags/*/k"]));
    assert_eq!(removed, paths(&["/next"]));
    let tags = Retyped { path: "/users/*/tags/*".to_owned(), before: "string".to_owned(), after: "object".to_owned() };
    assert_eq!(retyped, [tags]);
}

#[test]
fn joins_the_types_every_body_of_a_run_had_at_a_path() {
    let before = answered("a#get", 200, &[r#"{"n": 1}"#, r#"{"n": null}"#]);
    let after = answered("a#get", 200, &[r#"{"n": 1}"#]);
    let [change] = shape_changes((&before, &after)).try_into().unwrap();
    assert_eq!((change.retyped[0].before.as_str(), change.retyped[0].after.as_str()), ("null|number", "number"));
    assert_eq!(shape_changes((&before, &before)), []);
}

#[test]
fn an_empty_array_hides_no_element_paths_and_shows_none_as_removed() {
    let before = answered("a#list", 200, &[r#"{"items": [{"id": 1}]}"#]);
    let after = answered("a#list", 200, &[r#"{"items": []}"#]);
    assert_eq!(shape_changes((&before, &after)), []);
    assert_eq!(shape_changes((&after, &before)), []);
}

#[test]
fn compares_each_api_and_status_apart_and_only_when_both_runs_kept_a_json_body() {
    let before = [answered("a#get", 200, &[r#"{"id": 1}"#]), answered("a#get", 404, &[r#"{"error": "x"}"#])].concat();
    let after = [
        answered("a#get", 200, &[r#"{"id": 1}"#]),
        answered("a#get", 404, &[r#"{"error": "x", "code": 4}"#]),
        answered("b#get", 200, &[r#"{"new": true}"#]),
    ]
    .concat();
    let changes = shape_changes((&before, &after));
    assert_eq!(
        changes.iter().map(|change| (change.api_id.as_str(), change.status)).collect::<Vec<_>>(),
        [("a#get", 404)]
    );
    let unread = answered("a#get", 200, &["not json"]);
    assert_eq!(shape_changes((&unread, &after)), []);
    let mut truncated = answered("a#get", 200, &[r#"{"id": 1, "more": 2}"#]);
    truncated[0].body_truncated = true;
    assert_eq!(shape_changes((&before, &truncated)), []);
}

#[test]
fn escapes_field_names_as_json_pointer_segments_and_never_shows_values() {
    let before = answered("a#get", 200, &[r#"{"x": 1}"#]);
    let after = answered("a#get", 200, &[r#"{"x": 1, "a/b~c": "tok-secret-91"}"#]);
    let [change] = shape_changes((&before, &after)).try_into().unwrap();
    assert_eq!(change.added, paths(&["/a~1b~0c"]));
    assert!(!format!("{change:?}").contains("tok-secret-91"));
}
