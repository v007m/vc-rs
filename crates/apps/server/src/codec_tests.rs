use prost::Message as _;
use serde_json::{Value, json};
use vc_contracts::engine::v1::{ReadFileResponse, RunRequest};

use super::{Codec, Shape};
use crate::{Code, ConnectError};

#[test]
fn each_content_type_names_its_codec_for_its_shape_only() {
    let found = [
        ("application/proto", Shape::Unary),
        ("application/json", Shape::Unary),
        ("application/connect+proto", Shape::Stream),
        ("Application/Connect+JSON", Shape::Stream),
        ("application/json", Shape::Stream),
        ("application/connect+proto", Shape::Unary),
        ("text/plain", Shape::Unary),
    ]
    .map(|(essence, shape)| Codec::of(essence, shape));
    let proto = Some(Codec::Proto);
    let json = Some(Codec::Json);
    assert_eq!(found, [proto, json, proto, json, None, None, None]);
}

#[test]
fn each_codec_answers_in_its_own_content_type() {
    let answers = [Codec::Proto, Codec::Json].map(|codec| [Shape::Unary, Shape::Stream].map(|s| codec.content_type(s)));
    assert_eq!(
        answers,
        [["application/proto", "application/connect+proto"], ["application/json", "application/connect+json"]]
    );
}

#[test]
fn json_reads_the_protobuf_json_mapping() {
    let body = br#"{"profiles":["staging"],"seed":"42","repeat":3,"noCache":true}"#;
    let request: RunRequest = Codec::Json.decode(body).unwrap();
    let expected = RunRequest {
        profiles: vec!["staging".into()],
        seed: Some(42),
        repeat: Some(3),
        no_cache: true,
        ..RunRequest::default()
    };
    assert_eq!(request, expected);
}

#[test]
fn json_writes_the_protobuf_json_mapping_and_proto_writes_binary() {
    let response = ReadFileResponse { text: "read users.api.yaml".into(), digest: "d1".into() };
    let json: Value = serde_json::from_slice(&Codec::Json.encode(&response).unwrap()).unwrap();
    assert_eq!(json, json!({ "text": "read users.api.yaml", "digest": "d1" }));
    let binary = Codec::Proto.encode(&response).unwrap();
    assert_eq!(ReadFileResponse::decode(binary.as_slice()).unwrap(), response);
}

#[test]
fn a_body_that_is_not_the_message_is_invalid_argument_without_its_text() {
    let secret = "tok-not-a-real-secret-0123456789";
    let bodies = [format!("{{\"profiles\": \"{secret}\"}}"), format!("{secret} {{")];
    for body in bodies {
        let error = Codec::Json.decode::<RunRequest>(body.as_bytes()).unwrap_err();
        let expected = ConnectError::new(Code::InvalidArgument, "the request is not a valid message for this method");
        assert_eq!(error, expected);
    }
}
