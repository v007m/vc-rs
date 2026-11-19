//! The runtime of a `graphql` api (D343): a JSON `POST` of `{query, variables, operationName}`.
//! The response stays the HTTP one; its errors are read normalised, and a 2xx body that is not a
//! GraphQL response fails the call (V5124, D345).

use http::HeaderValue;
use http::header::ACCEPT;
use serde_json::{Map, Value};
use vc_diagnostics::Code;
use vc_model::{Action, ActionId, Api, Exchange, Failure, GraphqlBody, Protocol, RenderedRequest};

use super::protocol::ProtocolRuntime;
use super::request;
use crate::Render;
use crate::check;

/// The `accept` a `graphql` call sends when no level sets one (GraphQL over HTTP).
const ACCEPTS: &str = "application/graphql-response+json, application/json;q=0.9";

/// `graphql`: the request [`request::rendered`] builds, with [`ACCEPTS`] unless a level writes
/// `accept`; a null that removes it wins.
#[derive(Debug)]
pub struct GraphqlRuntime;

impl ProtocolRuntime for GraphqlRuntime {
    fn protocol(&self) -> Protocol {
        Protocol::Graphql
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        let mut request = request::rendered(render, api, call)?;
        if !request::mentions(render, (api, call.1), &ACCEPT) {
            drop(request.headers.insert(ACCEPT, HeaderValue::from_static(ACCEPTS)));
        }
        Ok(request)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        request::received(exchange)
    }

    /// The body's errors. A 2xx body that is not a JSON object with `data` or `errors`, or whose
    /// `errors` is not a list, is V5124; any other status is left to the status check.
    fn errors(&self, response: &Value) -> Result<Vec<Value>, Failure> {
        let status = response.get("status").and_then(Value::as_u64).unwrap_or_default();
        match read(response.get("body")) {
            Ok(errors) => Ok(errors),
            Err(_) if !(200..300).contains(&status) => Ok(Vec::new()),
            Err(what) => Err(Failure::new(format!("The response is not a GraphQL response: {what}"))
                .with_code(Code::V5124.to_string())),
        }
    }
}

/// The errors of a GraphQL response `body`, normalised. A null `errors` is none (D363), so a
/// body whose only key is that is not a GraphQL response either.
fn read(body: Option<&Value>) -> Result<Vec<Value>, &'static str> {
    let object = body.and_then(Value::as_object);
    let has = |key| object.and_then(|object| object.get(key)).is_some_and(|value: &Value| !value.is_null());
    if !(object.is_some_and(|object| object.contains_key("data")) || has("errors")) {
        return Err("the body is not a JSON object with `data` or `errors`");
    }
    check::normalised(body)
}

/// The JSON a `graphql` api sends: its document as written, the rendered `variables`, and its
/// `operationName` when it sets one.
pub fn payload(body: &GraphqlBody, variables: Value) -> Value {
    let mut payload = Map::new();
    drop(payload.insert("query".to_owned(), Value::String(body.document.clone())));
    drop(payload.insert("variables".to_owned(), variables));
    if let Some(name) = &body.operation_name {
        drop(payload.insert("operationName".to_owned(), Value::String(name.clone())));
    }
    Value::Object(payload)
}

#[cfg(test)]
#[path = "graphql_tests.rs"]
mod tests;
