//! An api's `path` as sent (D278): its literal text as written, and each `${…}` value
//! percent-encoded so that it stays one path segment, whatever it holds.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use vc_model::{Failure, Part, Reference, Template};

use super::request::{self, encode};
use crate::Render;

/// What a value keeps in a path segment: RFC 3986 `pchar`, so `/`, `?`, `#`, `%` and a space
/// are encoded.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@');

/// `path` rendered: literals as written, each value encoded for where it falls.
pub fn rendered(render: &Render<'_>, path: &Template) -> Result<String, Failure> {
    let mut out = String::new();
    for part in &path.parts {
        match part {
            Part::Literal(text) => out.push_str(text),
            Part::Ref(reference) => {
                let value = render.text(&request::lone(reference.clone())).map_err(request::failed)?;
                let placed = placed(&out, reference, &value);
                out.push_str(&placed);
            }
        }
    }
    Ok(out)
}

/// One value after the text `before`. Before any `/` it is the URL's start, such as
/// `${var.usersUrl}`, and is sent as written; after a `?` it is a query value; anywhere else
/// it is one segment, with `.` and `..` encoded too so they name no other path. A value whose
/// last pipe is `urlEncode` is already encoded (V4007).
fn placed(before: &str, reference: &Reference, value: &str) -> String {
    if !before.contains('/') || url_encoded(reference) {
        return value.to_owned();
    }
    if before.contains('?') {
        return encode(value);
    }
    match value {
        "." => "%2E".to_owned(),
        ".." => "%2E%2E".to_owned(),
        _ => utf8_percent_encode(value, SEGMENT).to_string(),
    }
}

/// Whether `reference` ends in the `urlEncode` pipe.
fn url_encoded(reference: &Reference) -> bool {
    reference.pipes.last().is_some_and(|pipe| pipe.name == "urlEncode")
}

#[cfg(test)]
mod tests {
    use super::{SEGMENT, utf8_percent_encode};

    #[test]
    fn a_segment_keeps_pchar_and_encodes_what_would_end_it() {
        let encoded = |text| utf8_percent_encode(text, SEGMENT).to_string();
        assert_eq!(encoded("a/b c"), "a%2Fb%20c");
        assert_eq!(encoded("../x?y#z%"), "..%2Fx%3Fy%23z%25");
        assert_eq!(encoded("a:b@c!$&'()*+,;=-._~"), "a:b@c!$&'()*+,;=-._~");
        assert_eq!(encoded("é"), "%C3%A9");
    }
}
