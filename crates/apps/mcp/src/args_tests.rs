use serde_json::json;

use super::{count, flag, known, optional_text, pairs, text, texts};
use crate::fixture::args;

fn reason<T: std::fmt::Debug>(result: Result<T, crate::McpError>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn a_string_argument_is_read_or_named_as_missing_or_wrong() {
    let given = args(json!({"api": "users#list", "n": 3, "none": null}));
    assert_eq!(text(&given, "api").unwrap(), "users#list");
    assert_eq!(optional_text(&given, "none").unwrap(), None);
    assert_eq!(reason(text(&given, "path")), "argument `path`: is required");
    assert_eq!(reason(text(&given, "n")), "argument `n`: must be a string");
}

#[test]
fn a_list_argument_must_hold_only_strings() {
    let given = args(json!({"ids": ["a", "b"], "mixed": ["a", 1], "one": "a"}));
    assert_eq!(texts(&given, "ids").unwrap(), ["a", "b"]);
    assert_eq!(texts(&given, "absent").unwrap(), Vec::<String>::new());
    assert_eq!(reason(texts(&given, "mixed")), "argument `mixed`: must be a list of strings");
    assert_eq!(reason(texts(&given, "one")), "argument `one`: must be a list of strings");
}

#[test]
fn flags_and_counts_default_when_not_given() {
    let given = args(json!({"keep": true, "limit": 5, "bad": "yes", "minus": -1}));
    assert!(flag(&given, "keep").unwrap());
    assert!(!flag(&given, "absent").unwrap());
    assert_eq!(reason(flag(&given, "bad")), "argument `bad`: must be true or false");
    assert_eq!(count(&given, "limit").unwrap(), Some(5));
    assert_eq!(count(&given, "absent").unwrap(), None);
    assert_eq!(reason(count(&given, "minus")), "argument `minus`: must be a whole number");
}

#[test]
fn an_object_argument_becomes_name_value_pairs() {
    let given = args(json!({"vars": {"userId": 7, "name": "Ada"}, "bad": [1]}));
    assert_eq!(pairs(&given, "vars").unwrap(), ["name=Ada", "userId=7"]);
    assert_eq!(pairs(&given, "absent").unwrap(), Vec::<String>::new());
    assert_eq!(reason(pairs(&given, "bad")), "argument `bad`: must be an object of names to values");
}

#[test]
fn an_argument_the_schema_does_not_name_is_refused_naming_those_it_does() {
    let schema = json!({"type": "object", "properties": {"ids": {}, "tags": {}}});
    assert!(known(&schema, &args(json!({"ids": ["a"], "tags": null}))).is_ok());
    assert_eq!(
        reason(known(&schema, &args(json!({"ids": [], "select": "people.listing"})))),
        "argument `select`: is not one this tool takes; it takes `ids`, `tags`"
    );
    let bare = json!({"type": "object", "properties": {}});
    assert_eq!(
        reason(known(&bare, &args(json!({"bogus": 1})))),
        "argument `bogus`: is not one this tool takes; it takes none"
    );
}
