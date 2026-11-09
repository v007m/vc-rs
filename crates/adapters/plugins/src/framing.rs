//! The LSP framing of each JSON-RPC message on a plugin's stdin and stdout: a
//! `Content-Length: <bytes>` header, a blank line, then the JSON body.

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt as _, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// The largest message a plugin may send: 16 MiB.
pub const MAX_MESSAGE: usize = 16 * 1024 * 1024;

/// Reads the next message: `None` once the stream ends between messages.
///
/// # Errors
///
/// Why the bytes are not a message: a missing or bad `Content-Length`, a body over
/// [`MAX_MESSAGE`], a stream that ends inside one, or a body that is not JSON.
pub async fn read<R: AsyncBufRead + Unpin + Send>(reader: &mut R) -> Result<Option<Value>, String> {
    let Some(length) = content_length(reader).await? else { return Ok(None) };
    if length > MAX_MESSAGE {
        return Err(format!("a message of {length} bytes is over the {MAX_MESSAGE}-byte limit"));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.map_err(|error| format!("the stream ended inside a message: {error}"))?;
    serde_json::from_slice(&body).map(Some).map_err(|error| format!("a message is not JSON: {error}"))
}

/// Writes `message` with its header.
///
/// # Errors
///
/// When the stream cannot be written: the plugin closed its stdin or stopped.
pub async fn write<W: AsyncWrite + Unpin + Send>(writer: &mut W, message: &Value) -> std::io::Result<()> {
    let body = message.to_string();
    writer.write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await?;
    writer.flush().await
}

/// The headers up to the blank line: the `Content-Length` they give, `None` at the end.
async fn content_length<R: AsyncBufRead + Unpin + Send>(reader: &mut R) -> Result<Option<usize>, String> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader.read_line(&mut line).await.map_err(|error| format!("cannot read a header: {error}"))?;
        if read == 0 {
            return length.map_or(Ok(None), |_| Err("the stream ended inside the headers".to_owned()));
        }
        let header = line.trim_end();
        if header.is_empty() {
            return length.map(Some).ok_or_else(|| "a message has no Content-Length".to_owned());
        }
        length = header_length(header)?.or(length);
    }
}

/// The length a `Content-Length` header gives; `None` for any other header.
fn header_length(header: &str) -> Result<Option<usize>, String> {
    match header.split_once(':') {
        Some((name, value)) if name.trim().eq_ignore_ascii_case("content-length") => {
            value.trim().parse().map(Some).map_err(|_| format!("`{}` is not a Content-Length", value.trim()))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MAX_MESSAGE, read, write};

    #[tokio::test]
    async fn a_written_message_reads_back_and_the_end_is_none() {
        let mut bytes = Vec::new();
        write(&mut bytes, &json!({"id": 1, "result": "é"})).await.unwrap();
        write(&mut bytes, &json!([1, 2])).await.unwrap();
        assert!(bytes.starts_with(b"Content-Length: 22\r\n\r\n{"), "{}", String::from_utf8_lossy(&bytes));
        let mut reader = bytes.as_slice();
        assert_eq!(read(&mut reader).await, Ok(Some(json!({"id": 1, "result": "é"}))));
        assert_eq!(read(&mut reader).await, Ok(Some(json!([1, 2]))));
        assert_eq!(read(&mut reader).await, Ok(None));
    }

    #[tokio::test]
    async fn other_headers_are_skipped_and_case_does_not_matter() {
        let mut reader: &[u8] = b"content-type: application/json\r\ncontent-length: 2\r\n\r\n{}";
        assert_eq!(read(&mut reader).await, Ok(Some(json!({}))));
    }

    #[tokio::test]
    async fn broken_frames_say_what_is_wrong() {
        let cases: [(&[u8], &str); 5] = [
            (b"X: 1\r\n\r\n{}", "a message has no Content-Length"),
            (b"Content-Length: two\r\n\r\n", "`two` is not a Content-Length"),
            (b"Content-Length: 2\r\n", "the stream ended inside the headers"),
            (b"Content-Length: 5\r\n\r\n{}", "the stream ended inside a message"),
            (b"Content-Length: 3\r\n\r\nnot", "a message is not JSON"),
        ];
        for (mut reader, error) in cases {
            let read = read(&mut reader).await.unwrap_err();
            assert!(read.starts_with(error), "{read}");
        }
        let huge = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE + 1);
        assert!(read(&mut huge.as_bytes()).await.unwrap_err().contains("over the"));
    }
}
