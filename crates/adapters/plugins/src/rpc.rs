//! JSON-RPC 2.0 messages between the host and a plugin.

use serde_json::{Map, Value, json};

/// The JSON-RPC code of a method the receiver does not have.
pub const METHOD_NOT_FOUND: i64 = -32_601;

/// The JSON-RPC code of an error the sender gave no code for.
const INTERNAL_ERROR: i64 = -32_603;

/// A message a plugin sent.
#[derive(Debug, PartialEq, Eq)]
pub enum Incoming {
    /// The answer to the host's request `id`: its result, or the error's code and message.
    Answer { id: u64, outcome: Result<Value, (i64, String)> },
    /// A request the host answers, such as `secrets/get`.
    Request { id: Value, method: String, params: Value },
    /// A notification: nothing to answer.
    Notice,
}

/// The host's request `id`.
pub fn request(id: u64, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// The host's answer to the plugin's request `id`.
pub fn result(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// The host's refusal of the plugin's request `id`.
pub fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// What `message` is.
///
/// # Errors
///
/// Why it is not JSON-RPC: not an object, or an answer without a numeric `id` or an outcome.
pub fn classify(message: Value) -> Result<Incoming, String> {
    let Value::Object(mut object) = message else { return Err("a message is not a JSON object".to_owned()) };
    if let Some(method) = object.get("method").and_then(Value::as_str).map(str::to_owned) {
        let params = object.remove("params").unwrap_or(Value::Null);
        return Ok(match object.remove("id") {
            Some(id) if !id.is_null() => Incoming::Request { id, method, params },
            _ => Incoming::Notice,
        });
    }
    let id = object.get("id").and_then(Value::as_u64).ok_or_else(|| "an answer has no numeric `id`".to_owned())?;
    let outcome = match (object.remove("result"), object.remove("error")) {
        (_, Some(error)) => Err(remote(&error)),
        (Some(result), None) => Ok(result),
        (None, None) => return Err(format!("the answer to {id} has neither `result` nor `error`")),
    };
    Ok(Incoming::Answer { id, outcome })
}

/// An error object's code and message.
fn remote(error: &Value) -> (i64, String) {
    let empty = Map::new();
    let error = error.as_object().unwrap_or(&empty);
    let code = error.get("code").and_then(Value::as_i64).unwrap_or(INTERNAL_ERROR);
    (code, error.get("message").and_then(Value::as_str).unwrap_or_default().to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Incoming, classify, error, request, result};

    #[test]
    fn answers_requests_and_notices_are_told_apart() {
        let answer = classify(result(&json!(3), &json!({"ok": true}))).unwrap();
        assert_eq!(answer, Incoming::Answer { id: 3, outcome: Ok(json!({"ok": true})) });
        let refused = classify(error(&json!(4), -32_001, "no")).unwrap();
        assert_eq!(refused, Incoming::Answer { id: 4, outcome: Err((-32_001, "no".to_owned())) });
        let asked = classify(request(9, "secrets/get", &json!({"name": "a"}))).unwrap();
        let params = json!({"name": "a"});
        assert_eq!(asked, Incoming::Request { id: json!(9), method: "secrets/get".to_owned(), params });
        assert_eq!(classify(json!({"jsonrpc": "2.0", "method": "log"})).unwrap(), Incoming::Notice);
        let bare = classify(json!({"id": 5, "result": null})).unwrap();
        assert_eq!(bare, Incoming::Answer { id: 5, outcome: Ok(json!(null)) });
        let odd = classify(json!({"id": 6, "error": "x"})).unwrap();
        assert_eq!(odd, Incoming::Answer { id: 6, outcome: Err((-32_603, String::new())) });
    }

    #[test]
    fn what_is_not_json_rpc_is_named() {
        assert_eq!(classify(json!([1])).unwrap_err(), "a message is not a JSON object");
        assert_eq!(classify(json!({"id": "a", "result": 1})).unwrap_err(), "an answer has no numeric `id`");
        assert_eq!(classify(json!({"id": 2})).unwrap_err(), "the answer to 2 has neither `result` nor `error`");
    }
}
