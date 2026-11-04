use async_trait::async_trait;
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::Event;

/// A reporter that ignores everything, for runs nobody watches (the null object).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopReporter;

#[async_trait]
impl Reporter for NoopReporter {
    async fn on_event(&mut self, _: &Event) -> Result<(), ReportError> {
        Ok(())
    }

    async fn finish(&mut self, _: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use vc_testkit::reporter_suite;

    use super::NoopReporter;

    #[tokio::test]
    async fn passes_the_reporter_suite() {
        reporter_suite(&mut || Box::new(NoopReporter)).await;
    }
}
