use bytes::Bytes;
use serde_json::{Value, json};
use vc_interfaces::StreamFrame;

use super::super::stream::StreamRuntime;
use super::WebsocketRuntime;

fn messages(frame: StreamFrame) -> Vec<Value> {
    WebsocketRuntime.decoder().push(frame).into_iter().map(|message| Value::Object(message.fields)).collect()
}

#[test]
fn each_frame_is_one_message_with_its_type_and_its_data() {
    let cases = [
        (StreamFrame::Text("{\"n\":1}".to_owned()), json!({"type": "text", "data": {"n": 1}, "text": "{\"n\":1}"})),
        (StreamFrame::Text("hi".to_owned()), json!({"type": "text", "data": "hi", "text": "hi"})),
        (StreamFrame::Binary(Bytes::from_static(b"\x00\x01")), json!({"type": "binary", "data": "AAE="})),
        (
            StreamFrame::Close { code: 1000, reason: "bye".to_owned() },
            json!({"type": "close", "code": 1000, "reason": "bye"}),
        ),
    ];
    for (frame, message) in cases {
        assert_eq!(messages(frame), vec![message]);
    }
    assert_eq!(messages(StreamFrame::Chunk(Bytes::from_static(b"x"))), Vec::<Value>::new());
}

#[test]
fn a_send_goes_out_as_its_text_or_as_json_text() {
    use vc_interfaces::OutgoingFrame;
    assert_eq!(WebsocketRuntime.encode(&json!("ping")), OutgoingFrame::Text("ping".to_owned()));
    assert_eq!(WebsocketRuntime.encode(&json!({"op": "sub"})), OutgoingFrame::Text("{\"op\":\"sub\"}".to_owned()));
}
