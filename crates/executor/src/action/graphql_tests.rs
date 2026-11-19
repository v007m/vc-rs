use std::sync::Arc;

use http::header::{ACCEPT, CONTENT_TYPE};
use serde_json::{Value, json};
use vc_model::{ActionId, BodySpec, GraphqlBody, Method, OwnerKey, Protocol, RenderedRequest, TemplateValue};
use vc_testkit::{FakeTransport, RecordingTransport};

use super::ACCEPTS;
use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionRequest, Frame, HostLimits, PipelineParts};

const QUERY: &str = "query GetUser($id: ID!) { user(id: $id) { id name } }";

/// A call of a `graphql` api at `/graphql` sending `body`, as the compiler builds one.
fn graphql(body: GraphqlBody) -> (Fixture, ActionId) {
    let mut fixture = Fixture::new();
    let call = fixture.call("/graphql");
    let api = fixture.api_mut(call);
    (api.protocol, api.method, api.body) = (Protocol::Graphql, Method::Post, BodySpec::Graphql(body));
    (fixture, call)
}

/// What the call sent.
async fn sent((fixture, call): (Fixture, ActionId)) -> RenderedRequest {
    let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    assert_eq!(report.outcome.failure(), None);
    recordings.take().remove(0).request
}

fn header(request: &RenderedRequest, name: http::HeaderName) -> &str {
    request.headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default()
}

fn body(request: &RenderedRequest) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

#[tokio::test]
async fn a_graphql_call_posts_its_query_variables_and_operation_name_as_json() {
    let fixture = Fixture::new();
    let variables = TemplateValue::Map([("id".to_owned(), fixture.value("${var.userId}"))].into());
    let get_user = GraphqlBody { document: QUERY.to_owned(), variables, operation_name: Some("GetUser".to_owned()) };
    let (mut fixture, call) = graphql(get_user);
    fixture.program.config.vars.values = fixture.vars(&[("userId", "u-1")]);
    let request = sent((fixture, call)).await;
    assert_eq!((request.method.as_str(), request.uri.to_string()), ("POST", "http://api.test/graphql".to_owned()));
    assert_eq!(body(&request), json!({ "query": QUERY, "variables": { "id": "u-1" }, "operationName": "GetUser" }));
    assert_eq!((header(&request, CONTENT_TYPE), header(&request, ACCEPT)), ("application/json", ACCEPTS));
    assert_eq!(ACCEPTS, "application/graphql-response+json, application/json;q=0.9");
}

#[tokio::test]
async fn unset_variables_are_sent_as_an_empty_object_and_no_operation_name() {
    let empty = TemplateValue::Map(indexmap::IndexMap::new());
    let request =
        sent(graphql(GraphqlBody { document: "{ me { id } }".to_owned(), variables: empty, operation_name: None }))
            .await;
    assert_eq!(request.body, bytes::Bytes::from(r#"{"query":"{ me { id } }","variables":{}}"#));
}

#[tokio::test]
async fn a_written_content_type_and_accept_are_sent_as_written() {
    let empty = TemplateValue::Map(indexmap::IndexMap::new());
    let (mut fixture, call) =
        graphql(GraphqlBody { document: "{ me { id } }".to_owned(), variables: empty, operation_name: None });
    fixture.action_mut(call).headers =
        fixture.templates(&[("content-type", "text/plain"), ("accept", "application/json")]);
    let request = sent((fixture, call)).await;
    assert_eq!((header(&request, CONTENT_TYPE), header(&request, ACCEPT)), ("text/plain", "application/json"));
    assert_eq!(body(&request), json!({ "query": "{ me { id } }", "variables": {} }), "the body stays the GraphQL JSON");
}

#[tokio::test]
async fn an_accept_that_renders_null_removes_it_and_the_default_stays_out() {
    let empty = TemplateValue::Map(indexmap::IndexMap::new());
    let (mut fixture, call) =
        graphql(GraphqlBody { document: "{ me { id } }".to_owned(), variables: empty, operation_name: None });
    fixture.program.config.defaults.headers = fixture.templates(&[("Accept", "application/json")]);
    let none = fixture.var("region");
    drop(fixture.program.config.vars.values.insert(none, TemplateValue::Null));
    fixture.action_mut(call).headers = fixture.templates(&[("accept", "${var.region}")]);
    let request = sent((fixture, call)).await;
    assert_eq!(request.headers.get(ACCEPT), None, "a null removes the config's accept, and no default comes back");
}

#[tokio::test]
async fn an_accept_written_on_any_level_in_any_case_is_kept() {
    let empty = TemplateValue::Map(indexmap::IndexMap::new());
    let (mut fixture, call) =
        graphql(GraphqlBody { document: "{ me { id } }".to_owned(), variables: empty, operation_name: None });
    fixture.program.config.defaults.headers = fixture.templates(&[("ACCEPT", "application/json")]);
    let request = sent((fixture, call)).await;
    assert_eq!(header(&request, ACCEPT), "application/json");
}
