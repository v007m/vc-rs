use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use vc_interfaces::{ReportError, Reporter};

use crate::ReporterContext;
use crate::ctrf::CtrfReporter;
use crate::json::JsonReporter;
use crate::junit::JunitReporter;
use crate::spec::SpecReporter;

/// Builds one reporter for a run.
pub type ReporterFactory = Arc<dyn Fn(&ReporterContext) -> Box<dyn Reporter> + Send + Sync>;

/// The reporters, by name (D43): the console formats `spec` and `json`, and the report files
/// `ctrf` and `junit`.
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
        registry.register("ctrf", Arc::new(|context| Box::new(CtrfReporter::new(context))));
        registry.register("junit", Arc::new(|context| Box::new(JunitReporter::new(context))));
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
        assert_eq!(names, ["ctrf", "json", "junit", "spec"]);
        assert_eq!(ReporterRegistry::new().names().count(), 0);
        assert_eq!(format!("{:?}", ReporterRegistry::builtin()), r#"["ctrf", "json", "junit", "spec"]"#);
    }

    #[tokio::test]
    async fn creates_a_reporter_or_names_the_known_ones() {
        let (capture, captured) = Capture::new();
        let context = testing::context(capture);
        let mut registry = ReporterRegistry::builtin();
        registry.register("tap", Arc::new(|_| Box::new(NoopReporter)));
        let mut junit = registry.create("junit", &context).unwrap();
        let artifacts = junit.finish(&RunSummary::new(RunStatus::Passed, Totals::default())).await.unwrap();
        assert_eq!(artifacts.len(), 1);
        assert_eq!(captured.take().1.len(), 1, "junit saved its file");
        let mut tap = registry.create("tap", &context).unwrap();
        assert_eq!(tap.finish(&RunSummary::new(RunStatus::Passed, Totals::default())).await.unwrap(), []);
        let unknown = registry.create("dots", &context).err().unwrap();
        let known = "ctrf, json, junit, spec, tap".to_owned();
        assert_eq!(unknown, ReportError::Unknown { name: "dots".into(), known });
    }
}
