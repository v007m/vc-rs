use bytes::Bytes;

use super::{MAX_MESSAGE, end_stream, message, single};
use crate::Code;

fn enveloped(flags: u8, length: u32, payload: &[u8]) -> Bytes {
    let mut body = vec![flags];
    body.extend_from_slice(&length.to_be_bytes());
    body.extend_from_slice(payload);
    Bytes::from(body)
}

#[test]
fn a_message_is_framed_with_its_flags_and_big_endian_length() {
    assert_eq!(message(b"abc").as_ref(), [0, 0, 0, 0, 3, b'a', b'b', b'c']);
    assert_eq!(end_stream(b"{}").as_ref(), [2, 0, 0, 0, 2, b'{', b'}']);
}

#[test]
fn a_single_message_is_unwrapped() {
    let body = single(&enveloped(0, 3, b"abc")).unwrap();
    assert_eq!(body.as_ref(), b"abc");
    assert_eq!(single(&enveloped(0, 0, b"")).unwrap().len(), 0);
}

#[test]
fn a_length_over_the_limit_is_refused_before_the_message_is_read() {
    let over = u32::try_from(MAX_MESSAGE + 1).unwrap();
    assert_eq!(single(&enveloped(0, over, b"abc")).unwrap_err().code, Code::ResourceExhausted);
    assert_eq!(single(&enveloped(0, u32::MAX, b"")).unwrap_err().code, Code::ResourceExhausted);
}

#[test]
fn a_bad_envelope_is_an_invalid_argument() {
    let bad = [
        Bytes::from_static(&[0, 0, 0]),
        enveloped(1, 3, b"abc"),
        enveloped(2, 3, b"abc"),
        enveloped(0, 4, b"abc"),
        enveloped(0, 2, b"abc"),
    ];
    for body in bad {
        assert_eq!(single(&body).unwrap_err().code, Code::InvalidArgument);
    }
}
