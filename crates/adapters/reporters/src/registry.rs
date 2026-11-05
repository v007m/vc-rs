use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use vc_interfaces::{ReportError, Reporter};

use crate::ReporterContext;
use crate::json::JsonReporter;
use crate::spec::SpecReporter;

/// Builds one reporter for a run.
pub type ReporterFactory = Arc<dyn Fn(&ReporterContext) -> Box<dyn Reporter> + Send + Sync>;

/// The console reporters, by name (D43): `spec` and `json`. Report files are
/// [`Format`](crate::Format)s.
#[derive(Clone, Default)]
pub struct ReporterRegistry {
    factories: BTreeMap<String, ReporterFactory>,
}

impl ReporterRegistry {
    /// A registry with no reporters.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with every built-in reporter.
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register("spec", Arc::new(|context| Box::new(SpecReporter::new(context))));
        registry.register("json", Arc::new(|context| Box::new(JsonReporter::new(context))));
        registry
    }

    /// Adds `factory` under `name`, replacing any reporter already there.
    pub fn register(&mut self, name: impl Into<String>, factory: ReporterFactory) {
        self.factories.insert(name.into(), factory);
    }

    /// The registered names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.factories.keys().map(String::as_str)
    }

    /// The reporter registered as `name`, built from `context`.
    ///
    /// # Errors
    ///
    /// [`ReportError::Unknown`] when no reporter has that name.
    pub fn create(&self, name: &str, context: &ReporterContext) -> Result<Box<dyn Reporter>, ReportError> {
        let factory = self.factories.get(name).ok_or_else(|| ReportError::Unknown {
            name: name.to_owned(),
            known: self.names().collect::<Vec<_>>().join(", "),
        })?;
        Ok(factory(context))
    }
}

impl fmt::Debug for ReporterRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::{ReportError, RunSummary};
    use vc_model::{RunStatus, Totals};

    use super::ReporterRegistry;
    use crate::NoopReporter;
    use crate::testing::{self, Capture};

    #[test]
    fn has_the_builtin_reporters_by_name() {
        let registry = ReporterRegistry::builtin();
        let names: Vec<&str> = registry.names().collect();
        assert_eq!(names, ["json", "spec"]);
        assert_eq!(ReporterRegistry::new().names().count(), 0);
        assert_eq!(format!("{:?}", ReporterRegistry::builtin()), r#"["json", "spec"]"#);
    }

    #[tokio::test]
    async fn creates_a_reporter_or_names_the_known_ones() {
        let (capture, captured) = Capture::new();
        let context = testing::context(capture);
        let mut registry = ReporterRegistry::builtin();
        registry.register("tap", Arc::new(|_| Box::new(NoopReporter)));
        let mut json = registry.create("json", &context).unwrap();
        let artifacts = json.finish(&RunSummary::new(RunStatus::Passed, Totals::default())).await.unwrap();
        assert_eq!(artifacts, []);
        assert!(captured.take().0.contains("\"status\":\"passed\""), "json printed the summary");
        let mut tap = registry.create("tap", &context).unwrap();
        assert_eq!(tap.finish(&RunSummary::new(RunStatus::Passed, Totals::default())).await.unwrap(), []);
        let unknown = registry.create("dots", &context).err().unwrap();
        let known = "json, spec, tap".to_owned();
        assert_eq!(unknown, ReportError::Unknown { name: "dots".into(), known });
    }
}
