//! The wrong-type rows of a negative test, for fields that accept several types.

use serde_json::{Value, json};
use vc_interfaces::{DraftExample, ImportError, ImportOptions};

use super::{drafted, json_response, read_json};

/// A thing whose fields take one type, a type list, or `anyOf` alternatives, one by `$ref`.
fn things() -> Value {
    let fields = json!({
        "amount": { "type": "number" },
        "size": { "type": ["integer", "null"] },
        "code": { "type": ["string", "integer"] },
        "count": { "type": ["string", "number"] },
        "label": { "anyOf": [{ "type": "string" }, { "type": "integer" }] },
        "flag": { "oneOf": [{ "$ref": "#/components/schemas/Flag" }, { "type": "integer" }, { "type": "boolean" }] },
        "tag": { "type": ["string", "boolean", "array"] },
        "note": { "anyOf": [{ "type": "string" }, {}] },
    });
    let names: Vec<&String> = fields.as_object().map(|fields| fields.keys().collect()).unwrap_or_default();
    let thing = json!({ "type": "object", "required": names, "properties": fields });
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Things", "version": "1" },
        "paths": { "/things": { "post": {
            "operationId": "createThing",
            "requestBody": { "content": { "application/json": { "schema": thing } } },
            "responses": { "201": json_response(&json!({ "type": "object" })) },
        } } },
        "components": { "schemas": { "Flag": { "type": "boolean" } } },
    })
}

async fn mistyped_rows() -> Result<Vec<DraftExample>, ImportError> {
    let opts = ImportOptions { file_id: "things".to_owned(), negative: true, ..ImportOptions::default() };
    let draft = read_json(&things(), &opts).await?;
    let group = draft.groups.first().cloned().unwrap_or_default();
    let rows = drafted(&group, "rejectsInvalidCreateThing").examples;
    Ok(rows.into_iter().filter(|row| row.name.contains(" is not ")).collect())
}

#[tokio::test]
async fn a_mistyped_row_sends_a_value_none_of_the_field_s_types_accepts() -> Result<(), ImportError> {
    let rows = mistyped_rows().await?;
    let mut sent: Vec<(&str, Option<&Value>)> = rows
        .iter()
        .map(|row| {
            let field = row.name.split(' ').next().unwrap_or_default();
            (row.name.as_str(), row.body.get(field))
        })
        .collect();
    sent.sort_unstable_by_key(|(name, _)| *name);
    let expected = [
        ("amount is not a number", json!("not a number")),
        ("flag is not a boolean or integer", json!("yes")),
        ("size is not an integer", json!("not a number")),
        ("tag is not a string, boolean or array", json!(123)),
    ];
    let expected: Vec<(&str, Option<&Value>)> = expected.iter().map(|(name, value)| (*name, Some(value))).collect();
    assert_eq!(sent, expected);
    Ok(())
}
