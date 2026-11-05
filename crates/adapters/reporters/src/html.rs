//! The HTML report (D169): the page in `web/report/`, embedded at compile time, with the run's
//! data beside it in `data/` (folder layout) or inside it, gzipped (single-file layout).
//!
//! Data goes into scripts as JSON with every `<` written `<`, so no value can close the
//! script it is in; the page renders every value as text.

use std::io::Write as _;

use base64::Engine as _;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::{Map, Value};
use vc_interfaces::{Artifact, ReportError};

use crate::{Destination, Format, HtmlLayout, RunSnapshot, html_data};

const PAGE: &str = include_str!("../../../../web/report/report.html");
const STYLE: &str = include_str!("../../../../web/report/report.css");
const SCRIPT: &str = include_str!("../../../../web/report/report.js");

/// Writes the HTML report: `index.html` and `data/`, or one `report.html`.
#[derive(Clone, Copy, Debug)]
pub struct HtmlFormat {
    layout: HtmlLayout,
}

impl HtmlFormat {
    /// The report laid out as `layout`.
    #[must_use]
    pub const fn new(layout: HtmlLayout) -> Self {
        Self { layout }
    }
}

impl Format for HtmlFormat {
    fn render(&self, snapshot: &RunSnapshot, destination: &dyn Destination) -> Result<Vec<Artifact>, ReportError> {
        let entries = html_data::entries(snapshot);
        let run = snapshot.run.id.to_string();
        let path = match self.layout {
            HtmlLayout::Single => destination.save("report.html", page(&run, &bundle(&entries)?).as_bytes())?,
            HtmlLayout::Folder => {
                for (key, value) in &entries {
                    destination.save(&format!("data/{key}.js"), loader(key, value)?.as_bytes())?;
                }
                destination.save("index.html", page(&run, "").as_bytes())?
            }
        };
        Ok(vec![Artifact::new("html", path)])
    }
}

/// The page with its style and script inlined and `data` before the start call.
fn page(run: &str, data: &str) -> String {
    let script = format!("<script>{SCRIPT}</script>");
    let data = format!("{data}<script>VIB.start();</script>");
    PAGE.replace("<!--VIB:RUN-->", run)
        .replace("<!--VIB:STYLE-->", &format!("<style>{STYLE}</style>"))
        .replace("<!--VIB:SCRIPT-->", &script)
        .replace("<!--VIB:DATA-->", &data)
}

/// `data/<key>.js`: one `VIB.load` call.
fn loader(key: &str, value: &Value) -> Result<String, ReportError> {
    Ok(format!("VIB.load({},{});\n", script_json(&Value::String(key.to_owned()))?, script_json(value)?))
}

/// Every key in one gzipped, base64 JSON object, as the `#vib-data` element.
fn bundle(entries: &[(String, Value)]) -> Result<String, ReportError> {
    let all: Map<String, Value> = entries.iter().cloned().collect();
    let json = serde_json::to_vec(&Value::Object(all)).map_err(|error| encode_error(&error.to_string()))?;
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&json).map_err(|error| encode_error(&error.to_string()))?;
    let packed = gzip.finish().map_err(|error| encode_error(&error.to_string()))?;
    let text = base64::engine::general_purpose::STANDARD.encode(packed);
    Ok(format!("<script type=\"application/gzip\" id=\"vib-data\">{text}</script>"))
}

/// JSON that is safe inside a `<script>`.
fn script_json(value: &Value) -> Result<String, ReportError> {
    let text = serde_json::to_string(value).map_err(|error| encode_error(&error.to_string()))?;
    Ok(text.replace('<', "\\u003c"))
}

fn encode_error(reason: &str) -> ReportError {
    ReportError::Encode { reporter: "html".to_owned(), reason: reason.to_owned() }
}

#[cfg(test)]
#[path = "html_tests.rs"]
mod tests;
