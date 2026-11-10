//! The loopback redirect listener: waits for the browser's `GET /callback?code=…&state=…`.

use std::time::Duration;

use percent_encoding::percent_decode_str;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The most of a request it reads; a redirect's request line is far shorter.
const MAX_REQUEST: usize = 8 * 1024;

/// How long one connection may take to send its request line. A browser's idle preconnect is
/// answered and dropped after it, so the real redirect on another connection is still read.
const READ: Duration = Duration::from_secs(2);

/// What the browser shows when the sign-in completes.
const DONE: &str = "Signed in. You can close this tab and return to the terminal.";

/// What the browser shows when the sign-in fails.
const FAILED: &str = "Sign-in failed. Return to the terminal for details.";

/// What a `/callback` for another sign-in is shown.
const FOREIGN: &str = "This redirect does not belong to the sign-in in progress.";

/// The `code` from the first request to `/callback` whose `state` matches. Other paths get a
/// 404, and a `/callback` with another state a 400; both are skipped.
///
/// # Errors
///
/// The reason, without the code, when the server answered this sign-in with an `error` or no
/// code, or the listener fails.
pub async fn receive(listener: &TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let target = tokio::time::timeout(READ, request_target(&mut stream)).await.ok().flatten();
        let Some(query) = target.as_deref().and_then(|target| target.strip_prefix("/callback?")) else {
            respond(&mut stream, "404 Not Found", "Not found").await;
            continue;
        };
        let Some(outcome) = callback(query, state) else {
            respond(&mut stream, "400 Bad Request", FOREIGN).await;
            continue;
        };
        let (status, shown) = if outcome.is_ok() { ("200 OK", DONE) } else { ("400 Bad Request", FAILED) };
        respond(&mut stream, status, shown).await;
        return outcome;
    }
}

/// The code in a `/callback` query, or the server's error; `None` when its state is not this
/// sign-in's.
fn callback(query: &str, state: &str) -> Option<Result<String, String>> {
    let pairs: Vec<(String, String)> = query.split('&').filter_map(pair).collect();
    let get = |key: &str| pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
    if !get("state").is_some_and(|got| same(got.as_bytes(), state.as_bytes())) {
        return None;
    }
    if let Some(error) = get("error") {
        let error: String = error.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect();
        return Some(Err(format!("the server answered `error={error}`")));
    }
    let code = get("code").filter(|code| !code.is_empty()).map(str::to_owned);
    Some(code.ok_or_else(|| "the redirect has no code".to_owned()))
}

/// Compares in time that does not depend on where the inputs differ.
fn same(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && left.iter().zip(right).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0
}

fn pair(part: &str) -> Option<(String, String)> {
    let (key, value) = part.split_once('=')?;
    let decode = |text: &str| percent_decode_str(&text.replace('+', " ")).decode_utf8_lossy().into_owned();
    Some((decode(key), decode(value)))
}

/// The target of the request line, `GET <target> HTTP/1.1`.
async fn request_target(stream: &mut TcpStream) -> Option<String> {
    let mut read = Vec::new();
    let mut chunk = [0; 1024];
    while !read.windows(2).any(|end| end == b"\r\n") && read.len() < MAX_REQUEST {
        let got = stream.read(&mut chunk).await.ok().filter(|got| *got > 0)?;
        read.extend_from_slice(chunk.get(..got)?);
    }
    let line = String::from_utf8_lossy(&read).lines().next()?.to_owned();
    let mut parts = line.split(' ');
    (parts.next() == Some("GET")).then_some(())?;
    parts.next().map(str::to_owned)
}

async fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::{callback, same};

    #[test]
    fn a_callback_gives_its_code_only_when_the_state_matches() {
        assert_eq!(callback("code=a%2Bb+c&state=s1", "s1"), Some(Ok("a+b c".to_owned())));
        assert_eq!(callback("code=x&state=s2", "s1"), None);
        assert_eq!(callback("code=x", "s1"), None);
        assert_eq!(callback("error=access_denied&state=s2", "s1"), None);
        assert!(callback("state=s1", "s1").unwrap().unwrap_err().contains("no code"));
        assert!(callback("code=&state=s1", "s1").unwrap().is_err());
        let refused = callback("error=access_denied%3Cscript%3E&state=s1", "s1").unwrap().unwrap_err();
        assert_eq!(refused, "the server answered `error=access_deniedscript`");
        assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"abc", b"ab"));
    }
}
