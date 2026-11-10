use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use bytes::Bytes;
use http_body_util::BodyExt as _;
use prost::Message as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use vc_contracts::engine::v1::{ReadFileRequest, ReadFileResponse, RunEvent, RunRequest, RunResponse};

use crate::{Code, ConnectError, ConnectService, Sink};

const SECRET: &str = "tok-not-a-real-secret-0123456789";

fn router() -> Router {
    ConnectService::new("test.v1.TestService")
        .unary("ReadFile", |request: ReadFileRequest| async move {
            match request.path.as_str() {
                "missing.yaml" => Err(ConnectError::new(Code::NotFound, "no such file")),
                path => Ok(ReadFileResponse { text: format!("read {path}"), digest: "d1".into() }),
            }
        })
        .server_stream("Run", |request: RunRequest, sink: Sink<RunResponse>| async move {
            for seq in 1..=3 {
                sink.send(&RunResponse { event: Some(RunEvent { seq, kind: None }) }).await.unwrap();
            }
            if request.bail { Err(ConnectError::new(Code::Aborted, "stopped at the first failure")) } else { Ok(()) }
        })
        .server_stream("Unwritable", |_: RunRequest, sink: Sink<Unwritable>| async move {
            sink.send(&Unwritable {}).await?;
            Ok(())
        })
        .into_router()
}

/// A message that cannot be written as protobuf JSON.
#[derive(Clone, PartialEq, prost::Message)]
struct Unwritable {}

impl serde::Serialize for Unwritable {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("unwritable"))
    }
}

struct Reply {
    status: StatusCode,
    content_type: Option<String>,
    body: Bytes,
}

async fn post(method: &str, content_type: &str, body: Vec<u8>) -> Reply {
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/test.v1.TestService/{method}"))
        .header(header::CONTENT_TYPE, content_type)
        .header(header::AUTHORIZATION, format!("Bearer {SECRET}"))
        .body(Body::from(body))
        .unwrap();
    let response = router().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_owned());
    Reply { status, content_type, body: response.into_body().collect().await.unwrap().to_bytes() }
}

fn enveloped(payload: &[u8]) -> Vec<u8> {
    let mut body = vec![0];
    body.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
    body.extend_from_slice(payload);
    body
}

/// The stream's message payloads, then its end-stream JSON.
fn payloads(mut body: &[u8]) -> (Vec<&[u8]>, Value) {
    let mut messages = Vec::new();
    loop {
        let (head, rest) = body.split_at(5);
        let length = usize::try_from(u32::from_be_bytes(head[1..].try_into().unwrap())).unwrap();
        let (payload, rest) = rest.split_at(length);
        if head[0] == 2 {
            assert_eq!(rest, [0_u8; 0]);
            return (messages, serde_json::from_slice(payload).unwrap());
        }
        messages.push(payload);
        body = rest;
    }
}

/// The stream's binary messages, then its end-stream JSON.
fn frames(body: &[u8]) -> (Vec<RunResponse>, Value) {
    let (messages, end) = payloads(body);
    (messages.into_iter().map(|payload| RunResponse::decode(payload).unwrap()).collect(), end)
}

#[tokio::test]
async fn a_unary_call_answers_with_its_response_message() {
    let request = ReadFileRequest { path: "users.api.yaml".into() }.encode_to_vec();
    let reply = post("ReadFile", "application/proto", request).await;
    assert_eq!((reply.status, reply.content_type.as_deref()), (StatusCode::OK, Some("application/proto")));
    let response = ReadFileResponse::decode(reply.body).unwrap();
    assert_eq!(response.text, "read users.api.yaml");
}

#[tokio::test]
async fn a_json_unary_call_answers_in_protobuf_json() {
    let reply = post("ReadFile", "application/json; charset=utf-8", br#"{"path":"users.api.yaml"}"#.to_vec()).await;
    assert_eq!((reply.status, reply.content_type.as_deref()), (StatusCode::OK, Some("application/json")));
    let response: Value = serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(response, json!({ "text": "read users.api.yaml", "digest": "d1" }));
}

#[tokio::test]
async fn a_unary_error_has_its_code_status_and_connect_json() {
    let request = ReadFileRequest { path: "missing.yaml".into() }.encode_to_vec();
    let reply = post("ReadFile", "application/proto", request).await;
    assert_eq!((reply.status, reply.content_type.as_deref()), (StatusCode::NOT_FOUND, Some("application/json")));
    let error: Value = serde_json::from_slice(&reply.body).unwrap();
    assert_eq!(error, json!({ "code": "not_found", "message": "no such file" }));
}

#[tokio::test]
async fn a_stream_sends_each_message_then_an_empty_end_stream() {
    let reply = post("Run", "application/connect+proto", enveloped(&RunRequest::default().encode_to_vec())).await;
    assert_eq!((reply.status, reply.content_type.as_deref()), (StatusCode::OK, Some("application/connect+proto")));
    let (messages, end) = frames(&reply.body);
    let seqs: Vec<u64> = messages.iter().map(|m| m.event.as_ref().unwrap().seq).collect();
    assert_eq!((seqs, end), (vec![1, 2, 3], json!({})));
}

#[tokio::test]
async fn a_json_stream_sends_protobuf_json_messages_then_its_end_stream() {
    let reply = post("Run", "application/connect+json", enveloped(br#"{"bail":true}"#)).await;
    assert_eq!((reply.status, reply.content_type.as_deref()), (StatusCode::OK, Some("application/connect+json")));
    let (messages, end) = payloads(&reply.body);
    let messages: Vec<Value> = messages.into_iter().map(|payload| serde_json::from_slice(payload).unwrap()).collect();
    let seqs: Vec<Value> = ["1", "2", "3"].map(|seq| json!({ "event": { "seq": seq } })).into();
    assert_eq!(messages, seqs);
    assert_eq!(end, json!({ "error": { "code": "aborted", "message": "stopped at the first failure" } }));
}

#[tokio::test]
async fn a_stream_that_fails_ends_with_its_error() {
    let request = RunRequest { bail: true, ..RunRequest::default() }.encode_to_vec();
    let reply = post("Run", "application/connect+proto", enveloped(&request)).await;
    let (messages, end) = frames(&reply.body);
    assert_eq!(messages.len(), 3);
    assert_eq!(end, json!({ "error": { "code": "aborted", "message": "stopped at the first failure" } }));
}

#[tokio::test]
async fn a_message_that_cannot_be_written_ends_the_stream_with_an_internal_error() {
    let reply = post("Unwritable", "application/connect+json", enveloped(b"{}")).await;
    let (messages, end) = payloads(&reply.body);
    assert_eq!(messages, Vec::<&[u8]>::new());
    let error = json!({ "code": "internal", "message": "the response could not be written as JSON" });
    assert_eq!(end, json!({ "error": error }));
}

#[tokio::test]
async fn a_wrong_content_type_is_415_and_an_unknown_method_404_unimplemented() {
    for (method, content_type) in [
        ("ReadFile", "text/plain"),
        ("ReadFile", "application/connect+json"),
        ("Run", "application/proto"),
        ("Run", "application/json"),
    ] {
        assert_eq!(
            post(method, content_type, Vec::new()).await.status,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "{content_type}"
        );
    }
    let unknown = post("Delete", "application/proto", Vec::new()).await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(serde_json::from_slice::<Value>(&unknown.body).unwrap()["code"], json!("unimplemented"));
}

#[tokio::test]
async fn a_get_is_405() {
    let request = Request::get("/api/test.v1.TestService/ReadFile").body(Body::empty()).unwrap();
    assert_eq!(router().oneshot(request).await.unwrap().status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn a_body_over_the_limit_is_resource_exhausted() {
    let reply = post("ReadFile", "application/proto", vec![0; crate::envelope::MAX_MESSAGE + 1]).await;
    assert_eq!(reply.status, StatusCode::TOO_MANY_REQUESTS);
    let mut over = vec![0];
    over.extend_from_slice(&u32::MAX.to_be_bytes());
    let (_, end) = frames(&post("Run", "application/connect+proto", over).await.body);
    assert_eq!(end["error"]["code"], json!("resource_exhausted"));
}

#[tokio::test]
async fn an_error_never_echoes_what_the_request_carried() {
    let garbage = format!("tok {SECRET}").into_bytes();
    let unary = post("ReadFile", "application/proto", garbage.clone()).await;
    assert_eq!(unary.status, StatusCode::BAD_REQUEST);
    let stream = post("Run", "application/connect+proto", enveloped(&garbage)).await;
    let (_, end) = frames(&stream.body);
    assert_eq!(end["error"]["code"], json!("invalid_argument"));
    let json = post("ReadFile", "application/json", format!("{{\"path\": {SECRET}}}").into_bytes()).await;
    assert_eq!(json.status, StatusCode::BAD_REQUEST);
    for body in [unary.body, stream.body, json.body] {
        assert!(!String::from_utf8_lossy(&body).contains(SECRET));
    }
}

#[test]
fn every_code_has_its_connect_name_and_status() {
    let names: Vec<_> =
        [Code::OutOfRange, Code::DataLoss, Code::Canceled, Code::Unauthenticated].into_iter().map(Code::wire).collect();
    assert_eq!(names, [("out_of_range", 400), ("data_loss", 500), ("canceled", 499), ("unauthenticated", 401)]);
}
