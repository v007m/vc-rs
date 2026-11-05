//! `vibranium:std`, the one module a hook may import that is not its own: hashing, HMAC and
//! text encodings, done in Rust.

use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, KeyInit, Mac};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::{Ctx, Exception, Function};
use sha2::{Digest, Sha256};

/// The module's import name.
pub const STD: &str = "vibranium:std";

/// The module's exports, each `(text, ...) => string`.
const EXPORTS: [&str; 6] = ["hmacSha256Hex", "sha256Hex", "base64Encode", "base64Decode", "urlEncode", "urlDecode"];

/// `vibranium:std` as a native module.
pub struct Std;

impl ModuleDef for Std {
    fn declare(declarations: &Declarations<'_>) -> rquickjs::Result<()> {
        for name in EXPORTS {
            declarations.declare(name)?;
        }
        Ok(())
    }

    fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
        exports.export("hmacSha256Hex", Function::new(ctx.clone(), |key: String, data: String| hmac_sha256_hex(&key, &data))?)?;
        exports.export("sha256Hex", Function::new(ctx.clone(), |data: String| hex(&Sha256::digest(data.as_bytes())))?)?;
        exports.export("base64Encode", Function::new(ctx.clone(), |text: String| STANDARD.encode(text))?)?;
        let decode = |ctx: Ctx<'_>, text: String| thrown(&ctx, base64_decode(&text));
        exports.export("base64Decode", Function::new(ctx.clone(), decode)?)?;
        exports.export("urlEncode", Function::new(ctx.clone(), |text: String| url_encode(&text))?)?;
        let decode = |ctx: Ctx<'_>, text: String| thrown(&ctx, url_decode(&text));
        exports.export("urlDecode", Function::new(ctx.clone(), decode)?)?;
        Ok(())
    }
}

/// `result`, or a `TypeError` thrown in the hook.
fn thrown(ctx: &Ctx<'_>, result: Result<String, &'static str>) -> rquickjs::Result<String> {
    result.map_err(|message| Exception::throw_type(ctx, message))
}

/// HMAC-SHA256 of `data` keyed with `key`, as lowercase hex.
pub fn hmac_sha256_hex(key: &str, data: &str) -> String {
    Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_or_else(
        |_| String::new(),
        |mut mac| {
            mac.update(data.as_bytes());
            hex(&mac.finalize().into_bytes())
        },
    )
}

/// `bytes` as lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Standard, padded base64 to UTF-8 text.
fn base64_decode(text: &str) -> Result<String, &'static str> {
    let bytes = STANDARD.decode(text).map_err(|_| "base64Decode: the text is not standard base64")?;
    String::from_utf8(bytes).map_err(|_| "base64Decode: the bytes are not UTF-8 text")
}

/// Percent-encodes every byte of `text` but the RFC 3986 unreserved characters.
fn url_encode(text: &str) -> String {
    text.bytes().fold(String::with_capacity(text.len()), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
        out
    })
}

/// Decodes every `%XX` in `text`; `+` stays a plus.
fn url_decode(text: &str) -> Result<String, &'static str> {
    let mut bytes = text.bytes();
    let mut out = Vec::with_capacity(text.len());
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let (high, low) = (bytes.next().and_then(hex_digit), bytes.next().and_then(hex_digit));
        let (Some(high), Some(low)) = (high, low) else {
            return Err("urlDecode: a `%` must be followed by two hex digits");
        };
        out.push(high * 16 + low);
    }
    String::from_utf8(out).map_err(|_| "urlDecode: the bytes are not UTF-8 text")
}

fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).and_then(|digit| u8::try_from(digit).ok())
}

#[cfg(test)]
mod tests {
    use super::{base64_decode, hmac_sha256_hex, url_decode, url_encode};

    #[test]
    fn hmac_sha256_matches_rfc_4231_case_2() {
        assert_eq!(
            hmac_sha256_hex("Jefe", "what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn url_and_base64_round_trip_and_reject_what_they_cannot_read() {
        assert_eq!(url_encode("a b/é~"), "a%20b%2F%C3%A9~");
        assert_eq!(url_decode("a%20b%2F%C3%A9~+").as_deref(), Ok("a b/é~+"));
        assert!(url_decode("%G0").is_err() && url_decode("%2").is_err() && url_decode("%FF").is_err());
        assert_eq!(base64_decode("aGk=").as_deref(), Ok("hi"));
        assert!(base64_decode("@@").is_err());
    }
}
