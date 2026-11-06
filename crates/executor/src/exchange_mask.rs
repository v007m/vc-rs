//! Masking an exchange, which is what history and reports keep of a call: the masked headers
//! whole, and every secret value, hidden field and rule match in the URL, the other headers
//! and both bodies.

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Uri};
use serde_json::Value;
use vc_model::Exchange;

use crate::MASKED;
use crate::mask::Masking;

/// [`MASKED`] percent-encoded, as it stands in a URL.
const MASKED_IN_URL: &str = "%E2%80%A2%E2%80%A2%E2%80%A2%E2%80%A2";

/// `exchange` with its URL, headers and bodies masked; its timing and connection are kept.
pub async fn exchange(masking: &Masking<'_>, mut exchange: Exchange) -> Exchange {
    exchange.request.uri = uri(masking, &exchange.request.uri).await;
    exchange.request.headers = headers(masking, &exchange.request.headers).await;
    exchange.request.body = body(masking, &exchange.request.body).await;
    exchange.response.headers = headers(masking, &exchange.response.headers).await;
    exchange.response.body = body(masking, &exchange.response.body).await;
    exchange
}

/// `uri` with what it must not show masked; one that no longer parses keeps only its scheme
/// and host.
async fn uri(masking: &Masking<'_>, uri: &Uri) -> Uri {
    let shown = uri.to_string();
    let masked = masking.text(&shown).await;
    if masked == shown {
        return uri.clone();
    }
    masked.replace(MASKED, MASKED_IN_URL).parse().unwrap_or_else(|_| masked_whole(uri))
}

fn masked_whole(uri: &Uri) -> Uri {
    let path = format!("/{MASKED_IN_URL}");
    let builder = Uri::builder().path_and_query(path.as_str());
    let builder = match (uri.scheme(), uri.authority()) {
        (Some(scheme), Some(authority)) => builder.scheme(scheme.clone()).authority(authority.clone()),
        _ => builder,
    };
    builder.build().unwrap_or_default()
}

/// `headers` with a masked header's every value replaced, and the rest masked as text; a
/// value that is not text is replaced too.
async fn headers(masking: &Masking<'_>, headers: &HeaderMap) -> HeaderMap {
    let mut masked = HeaderMap::with_capacity(headers.len());
    for (name, value) in headers {
        let shown = (!masking.masks.hides_header(name.as_str())).then_some(value);
        let text = match shown.and_then(|value| value.to_str().ok()) {
            Some(text) => Some(masking.text(text).await),
            None => None,
        };
        let mut value = text.and_then(|text| HeaderValue::try_from(text).ok()).unwrap_or_else(masked_value);
        value.set_sensitive(true);
        masked.append(name.clone(), value);
    }
    masked
}

fn masked_value() -> HeaderValue {
    HeaderValue::from_str(MASKED).unwrap_or_else(|_| HeaderValue::from_static("****"))
}

/// `body` masked: JSON field by field and then as text, other text as text, and bytes that
/// are not text replaced whole when a secret value is among them.
async fn body(masking: &Masking<'_>, body: &Bytes) -> Bytes {
    let Ok(text) = std::str::from_utf8(body) else {
        return if masking.masks.finds_value(body) { Bytes::from_static(MASKED.as_bytes()) } else { body.clone() };
    };
    let masked = match serde_json::from_str::<Value>(text) {
        Ok(json) => json_text(masking, &json, text).await,
        Err(_) => masking.text(text).await,
    };
    if masked == text { body.clone() } else { Bytes::from(masked) }
}

/// The JSON `json` (written as `text`) with its hidden fields and rule matches masked, then
/// with any secret value left in a key or a number masked as text.
async fn json_text(masking: &Masking<'_>, json: &Value, text: &str) -> String {
    let masked = masking.value(json.clone()).await;
    let written = if masked == *json { text.to_owned() } else { masked.to_string() };
    masking.masks.without_values(&written)
}

#[cfg(test)]
#[path = "exchange_mask_tests.rs"]
mod tests;
