//! The loopback redirect listener: waits for the browser's `GET /callback?code=…&state=…`.

use percent_encoding::percent_decode_str;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// The most of a request it reads; a redirect's request line is far shorter.
const MAX_REQUEST: usize = 8 * 1024;

/// What the browser shows when the sign-in completes.
const DONE: &str = "Signed in. You can close this tab and return to the terminal.";

/// The `code` from the first request to `/callback` whose `state` matches; other paths get a
/// 404 and are skipped.
///
/// # Errors
///
/// The reason, without the code, when the server answered with an `error`, the state does not
/// match or the listener fails.
pub async fn receive(listener: &TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        let target = request_target(&mut stream).await.unwrap_or_default();
        let Some(query) = target.strip_prefix("/callback?") else {
            respond(&mut stream, "404 Not Found", "Not found").await;
            continue;
        };
        let outcome = callback(query, state);
        let shown = if outcome.is_ok() { DONE } else { "Sign-in failed. Return to the terminal for details." };
        respond(&mut stream, if outcome.is_ok() { "200 OK" } else { "400 Bad Request" }, shown).await;
        return outcome;
    }
}

/// The code in a `/callback` query, once its state is checked.
fn callback(query: &str, state: &str) -> Result<String, String> {
    let pairs: Vec<(String, String)> = query.split('&').filter_map(pair).collect();
    let get = |key: &str| pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str());
    if let Some(error) = get("error") {
        let error: String = error.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect();
        return Err(format!("the server answered `error={error}`"));
    }
    if !get("state").is_some_and(|got| same(got.as_bytes(), state.as_bytes())) {
        return Err("the redirect's state does not match this sign-in".to_owned());
    }
    get("code").filter(|code| !code.is_empty()).map(str::to_owned).ok_or_else(|| "the redirect has no code".to_owned())
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
        assert_eq!(callback("code=a%2Bb+c&state=s1", "s1"), Ok("a+b c".to_owned()));
        assert!(callback("code=x&state=s2", "s1").unwrap_err().contains("state"));
        assert!(callback("code=x", "s1").is_err());
        assert!(callback("state=s1", "s1").unwrap_err().contains("no code"));
        assert!(callback("code=&state=s1", "s1").is_err());
        let refused = callback("error=access_denied%3Cscript%3E&state=s1", "s1").unwrap_err();
        assert_eq!(refused, "the server answered `error=access_deniedscript`");
        assert!(same(b"abc", b"abc") && !same(b"abc", b"abd") && !same(b"abc", b"ab"));
    }
}
