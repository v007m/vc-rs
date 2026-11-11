//! What an operation's responses say a test should expect: the success status, the status a
//! rejected body gets, and where the success body's JSON schema is in the spec.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::Value;

use super::draft::{Reader, Route};
use super::values::local;

/// How many `$ref`s in a row are followed before giving up on a response or schema.
const MAX_HOPS: usize = 16;

/// The statuses a spec documents for a body it rejects, in the order they are preferred.
const REJECTED: [&str; 2] = ["400", "422"];

/// The status a rejected body gets when the spec documents none of [`REJECTED`].
const REJECTED_DEFAULT: u16 = 400;

/// What a JSON pointer cannot hold unescaped as a URI fragment (`{id}` or `[]` in a path).
const FRAGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

impl Reader<'_> {
    /// Where the JSON schema of the operation's first 2xx response is in the spec, as a
    /// `#/pointer` safe in a URI (`/components/schemas/User` for a `$ref`, else the pointer
    /// into the operation), and the schema; none when that response has no JSON schema.
    pub fn success_schema(&self, route: &Route<'_>) -> Option<(String, &Value)> {
        let code = success_code(route)?;
        let method = route.method.name().to_ascii_lowercase();
        let at = format!("/paths/{}/{method}/responses/{}", escaped(&route.path), escaped(code));
        let (at, response) = followed(self.doc, at)?;
        let content = response.get("content")?.as_object()?;
        let media = content.keys().find(|media| media.contains("json"))?;
        let (pointer, schema) = followed(self.doc, format!("{at}/content/{}/schema", escaped(media)))?;
        Some((utf8_percent_encode(&pointer, FRAGMENT).to_string(), schema))
    }
}

/// The status a call should get: the first 2xx the operation lists, 200 when it lists none.
pub fn success(route: &Route<'_>) -> u16 {
    success_code(route).and_then(|code| code.parse().ok()).unwrap_or(200)
}

/// The status a body the schema rejects should get: the first of [`REJECTED`] the operation
/// lists, else [`REJECTED_DEFAULT`].
pub fn rejected(route: &Route<'_>) -> u16 {
    let responses = route.op.responses.as_ref();
    let listed = REJECTED.into_iter().find(|code| responses.is_some_and(|responses| responses.contains_key(*code)));
    listed.and_then(|code| code.parse().ok()).unwrap_or(REJECTED_DEFAULT)
}

/// `token` escaped as one JSON pointer token (`/users` gives `~1users`).
pub fn escaped(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// The first 2xx response key the operation lists, as written.
fn success_code<'r>(route: &Route<'r>) -> Option<&'r str> {
    let op = route.op;
    let codes = op.responses.iter().flat_map(|responses| responses.keys());
    codes.map(String::as_str).find(|code| code.starts_with('2'))
}

/// The node at `pointer` in `doc` with its local `$ref`s followed, and the pointer it ends at,
/// percent-decoded.
fn followed(doc: &Value, mut pointer: String) -> Option<(String, &Value)> {
    for _ in 0..MAX_HOPS {
        let node = doc.pointer(&pointer)?;
        match node.get("$ref").and_then(Value::as_str).and_then(local) {
            Some(next) => pointer = next.into_owned(),
            None => return Some((pointer, node)),
        }
    }
    None
}
