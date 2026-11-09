//! A recorded exchange, read the same from HAR 1.2 and 1.3: only the parts the importer uses.
//! No type here derives `Debug`, so a recorded value cannot reach a printout through one.

use har::Spec;
use serde::Deserialize;

/// One request the browser sent and what came back.
#[derive(Deserialize)]
pub struct Entry {
    pub request: Request,
    pub response: Response,
}

/// The request as recorded.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<Header>,
    pub post_data: Option<PostData>,
}

/// A request header.
#[derive(Deserialize)]
pub struct Header {
    pub name: String,
}

/// A request body and its type.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostData {
    #[serde(default)]
    pub mime_type: String,
    pub text: Option<String>,
}

/// The response, as far as filtering needs it.
#[derive(Deserialize)]
pub struct Response {
    pub content: Content,
}

/// The response body's type.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    pub mime_type: Option<String>,
}

/// The entries of a HAR 1.2 or 1.3 file, in recorded order.
///
/// # Errors
///
/// Why `bytes` is not such a file; the reason never quotes the file.
pub fn entries(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let har = har::from_slice(bytes).map_err(|error| format!("it is not a HAR 1.2 or 1.3 file: {error}"))?;
    let entries = match har.log {
        Spec::V1_2(log) => serde_json::to_value(log.entries),
        Spec::V1_3(log) => serde_json::to_value(log.entries),
    };
    entries.and_then(serde_json::from_value).map_err(|_| "its entries cannot be read".to_owned())
}
