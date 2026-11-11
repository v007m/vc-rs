use serde_json::json;

use super::accepts;

#[test]
fn each_json_type_is_accepted_by_its_own_schema_type_and_a_number_takes_an_integer() {
    let values = [json!(null), json!(true), json!(1.5), json!(1), json!("a"), json!([]), json!({})];
    let types = ["null", "boolean", "number", "integer", "string", "array", "object"];
    let accepted: Vec<Vec<&str>> =
        values.iter().map(|value| types.iter().copied().filter(|ty| accepts(ty, value)).collect()).collect();
    let expected = [
        vec!["null"],
        vec!["boolean"],
        vec!["number"],
        vec!["number", "integer"],
        vec!["string"],
        vec!["array"],
        vec!["object"],
    ];
    assert_eq!(accepted, expected);
}
