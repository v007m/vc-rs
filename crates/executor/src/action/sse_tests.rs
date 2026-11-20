use std::time::Duration;

use bytes::Bytes;
use http::HeaderValue;
use http::header::CONTENT_TYPE;
use serde_json::{Value, json};
use vc_interfaces::StreamFrame;
use vc_model::{ConnectionInfo, Exchange, RenderedRequest, Timing};

use super::super::stream::{Decoder, Resume, StreamRuntime};
use super::{SseDecoder, SseRuntime};

/// The messages `chunks`, pushed in order through one decoder, make, as JSON objects.
fn events(decoder: &mut SseDecoder, chunks: &[&[u8]]) -> Vec<Value> {
    let frames = chunks.iter().map(|chunk| StreamFrame::Chunk(Bytes::copy_from_slice(chunk)));
    frames.flat_map(|frame| decoder.push(frame)).map(|message| Value::Object(message.fields)).collect()
}

fn decoded(chunks: &[&[u8]]) -> Vec<Value> {
    events(&mut SseDecoder::default(), chunks)
}

#[test]
fn lines_end_in_lf_cr_or_crlf_even_when_a_chunk_splits_them() {
    let one = json!({"event": "message", "data": 1, "text": "1"});
    let two = json!({"event": "message", "data": 2, "text": "2"});
    for chunks in [
        &[b"data: 1\n\ndata: 2\n\n".as_slice()][..],
        &[b"data: 1\r\rdata: 2\r\r"],
        &[b"data: 1\r\n\r\ndata: 2\r\n\r\n"],
        &[b"data: 1\r", b"\n\r", b"\ndata: 2\r\n", b"\r\n"],
        &[b"da", b"ta: 1\n", b"\nd", b"ata: 2\n\n"],
    ] {
        assert_eq!(decoded(chunks), vec![one.clone(), two.clone()], "{chunks:?}");
    }
}

#[test]
fn data_lines_join_with_a_newline_and_parse_as_json_else_stay_text() {
    let joined = decoded(&[b"data: {\"id\":\ndata: 7}\n\ndata: hello\ndata:\n\n"]);
    let expected = [
        json!({"event": "message", "data": {"id": 7}, "text": "{\"id\":\n7}"}),
        json!({"event": "message", "data": "hello\n", "text": "hello\n"}),
    ];
    assert_eq!(joined, expected);
}

/// An empty `event` leaves the type `message`, as an `EventSource` does.
#[test]
fn an_empty_event_field_leaves_the_type_message() {
    let typed = decoded(&[b"event:\ndata: x\n\nevent: a\nevent:\ndata: y\n\n"]);
    let expected =
        [json!({"event": "message", "data": "x", "text": "x"}), json!({"event": "message", "data": "y", "text": "y"})];
    assert_eq!(typed, expected);
}

#[test]
fn a_comment_a_blank_block_and_an_unknown_field_send_nothing() {
    let mut decoder = SseDecoder::default();
    let quiet = events(&mut decoder, &[b": ping\n\nfoo: bar\nevent\n\nevent: tick\n\n"]);
    assert_eq!(quiet, Vec::<Value>::new());
    let next = events(&mut decoder, &[b"data: x\n\n"]);
    assert_eq!(
        next,
        vec![json!({"event": "message", "data": "x", "text": "x"})],
        "a block with no data clears its event"
    );
}

#[test]
fn an_event_names_its_type_and_a_field_without_a_colon_has_an_empty_value() {
    let named = decoded(&[b"event: price\ndata:  two spaces\ndata\n\n"]);
    assert_eq!(named, vec![json!({"event": "price", "data": " two spaces\n", "text": " two spaces\n"})]);
}

#[test]
fn a_leading_bom_is_skipped_once() {
    let bom = decoded(&[b"\xEF\xBB", b"\xBFdata: 1\n\n"]);
    assert_eq!(bom, vec![json!({"event": "message", "data": 1, "text": "1"})]);
    let later = decoded(&[b"data: 1\n\n\xEF\xBB\xBFdata: 2\n\n"]);
    assert_eq!(later.len(), 1, "a BOM after the start is part of a field name, so `data: 2` is unknown");
}

#[test]
fn an_id_carries_over_to_later_events_and_a_retry_belongs_to_its_block() {
    let mut decoder = SseDecoder::default();
    let read = events(&mut decoder, &[b"id: 1\nretry: 2500\ndata: a\n\ndata: b\n\nid\0x: 9\nretry: 1s\ndata: c\n\n"]);
    let expected = [
        json!({"event": "message", "data": "a", "id": "1", "retry": 2500, "text": "a"}),
        json!({"event": "message", "data": "b", "id": "1", "text": "b"}),
        json!({"event": "message", "data": "c", "id": "1", "text": "c"}),
    ];
    assert_eq!(read, expected, "an id holding NUL and a retry that is not digits are ignored");
    let resume = Resume { last_id: Some("1".to_owned()), retry: Some(Duration::from_millis(2500)) };
    assert_eq!(decoder.resume(), resume);
}

#[test]
fn an_empty_id_clears_the_last_id() {
    let mut decoder = SseDecoder::default();
    let read = events(&mut decoder, &[b"id: 4\ndata: a\n\nid\ndata: b\n\n"]);
    assert_eq!(read[1], json!({"event": "message", "data": "b", "text": "b"}));
    assert_eq!(decoder.resume(), Resume::default());
}

#[test]
fn an_unfinished_event_and_a_frame_that_is_not_a_chunk_send_nothing() {
    let mut decoder = SseDecoder::default();
    assert_eq!(events(&mut decoder, &[b"data: 1\n"]), Vec::<Value>::new());
    assert_eq!(decoder.push(StreamFrame::Text("data: 2\n\n".to_owned())), Vec::new());
    assert_eq!(events(&mut decoder, &[b"\n"]), vec![json!({"event": "message", "data": 1, "text": "1"})]);
}

#[test]
fn the_head_opens_a_stream_only_as_an_event_stream() {
    let head = |content_type: Option<&'static str>| {
        let mut response = vc_testkit::status(200);
        if let Some(value) = content_type {
            drop(response.headers.insert(CONTENT_TYPE, HeaderValue::from_static(value)));
        }
        let (timing, connection) = (Timing::default(), ConnectionInfo::default());
        let request = RenderedRequest::new(http::Method::GET, http::Uri::from_static("http://api.test/events"));
        SseRuntime.opened(&Exchange { request, response, timing, connection, messages: Vec::new() })
    };
    assert_eq!(head(Some("text/event-stream")), Ok(()));
    assert_eq!(head(Some("Text/Event-Stream; charset=utf-8")), Ok(()));
    let json = "the response is `application/json`, not `text/event-stream`".to_owned();
    assert_eq!(head(Some("application/json")), Err(json));
    let none = "the response has no content type, not `text/event-stream`".to_owned();
    assert_eq!(head(None), Err(none));
}
