//! Report files (`--report`): each format renders a finished run's [`RunSnapshot`], so `vc report`
//! can render it again later from the history.

use std::collections::BTreeMap;
use std::fmt::{self, Debug};
use std::sync::Arc;

use vc_interfaces::{Artifact, ReportError};

use crate::ctrf::CtrfFormat;
use crate::html::HtmlFormat;
use crate::junit::JunitFormat;
use crate::{Destination, RunSnapshot};

/// One report file format: `ctrf`, `junit`, `html`.
pub trait Format: Send + Sync + Debug {
    /// Writes the report of `snapshot` to `destination` and returns the files written.
    ///
    /// # Errors
    ///
    /// [`ReportError::Write`] when a file cannot be written, [`ReportError::Encode`] when the
    /// report cannot be encoded.
    fn render(&self, snapshot: &RunSnapshot, destination: &dyn Destination) -> Result<Vec<Artifact>, ReportError>;
}

/// How the `html` report is laid out (`reports.html.layout`, `--single-file`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HtmlLayout {
    /// `index.html` plus a `data/` folder loaded on demand: fast for large runs.
    #[default]
    Folder,
    /// One self-contained `report.html`, for attaching to a ticket or an email.
    Single,
}

/// The report formats by name.
#[derive(Clone, Default)]
pub struct FormatRegistry {
    formats: BTreeMap<String, Arc<dyn Format>>,
}

impl FormatRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `ctrf`, `junit` and `html` laid out as `layout`.
    #[must_use]
    pub fn builtin(layout: HtmlLayout) -> Self {
        let mut registry = Self::new();
        registry.register("ctrf", Arc::new(CtrfFormat));
        registry.register("junit", Arc::new(JunitFormat));
        registry.register("html", Arc::new(HtmlFormat::new(layout)));
        registry
    }

    /// Adds `format` as `name`, replacing a format of that name.
    pub fn register(&mut self, name: impl Into<String>, format: Arc<dyn Format>) {
        self.formats.insert(name.into(), format);
    }

    /// The registered names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.formats.keys().map(String::as_str)
    }

    /// The format called `name`.
    ///
    /// # Errors
    ///
    /// [`ReportError::Unknown`] naming the known formats.
    pub fn get(&self, name: &str) -> Result<Arc<dyn Format>, ReportError> {
        self.formats.get(name).cloned().ok_or_else(|| ReportError::Unknown {
            name: name.to_owned(),
            known: self.names().collect::<Vec<_>>().join(", "),
        })
    }
}

impl Debug for FormatRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::ReportError;

    use super::{FormatRegistry, HtmlLayout};

    #[test]
    fn has_the_builtin_formats_by_name() {
        let registry = FormatRegistry::builtin(HtmlLayout::default());
        assert_eq!(registry.names().collect::<Vec<_>>(), ["ctrf", "html", "junit"]);
        assert_eq!(format!("{registry:?}"), r#"["ctrf", "html", "junit"]"#);
        assert!(registry.get("junit").is_ok());
        let unknown = registry.get("tap").unwrap_err();
        assert_eq!(unknown, ReportError::Unknown { name: "tap".into(), known: "ctrf, html, junit".into() });
        assert_eq!(FormatRegistry::new().names().count(), 0);
    }
}
