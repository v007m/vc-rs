//! A built web UI of two files, for the server tests. Its CSP meta allows more than the header
//! the server builds from it may.

use bytes::Bytes;

use crate::Files;

/// Its `index.html`, whose meta lists `'unsafe-inline'` beside the build's one script hash.
pub const INDEX_HTML: &str = "<!doctype html><html><head><meta http-equiv=\"content-security-policy\" \
     content=\"script-src 'self' 'unsafe-inline' 'sha256-CbculNcflElcZFscHVVSn37ckaTi9ym23lQrblhKGW4='\"></head></html>";

/// A built web UI of two files.
pub struct TestFiles;

impl Files for TestFiles {
    fn get(&self, path: &str) -> Option<Bytes> {
        match path {
            "index.html" => Some(Bytes::from_static(INDEX_HTML.as_bytes())),
            "_app/immutable/app.js" => Some(Bytes::from_static(b"export {};")),
            _ => None,
        }
    }
}
