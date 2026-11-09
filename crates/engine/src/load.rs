//! `vc load <scenario>`: a load scenario from a `*.load.yaml` file, run against its profile.

use crate::error::not_built;
use crate::{Engine, EngineError};

/// What `vc load <scenario>` runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct LoadRequest {
    /// The scenario's full ID, `<fileId>#<id>`.
    pub scenario: String,
    /// `--profile`, in order.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order.
    pub sets: Vec<String>,
}

/// What a load run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LoadReport {
    /// The process exit code: 0 within thresholds, 3 a threshold breached (D127).
    pub exit: u8,
}

impl Engine {
    /// Runs the scenario's stages and checks its thresholds.
    ///
    /// # Errors
    ///
    /// [`EngineError::NotBuilt`] until load runs are built (M7).
    #[expect(clippy::unused_async, clippy::unused_async_trait_impl, reason = "drives the load stages once built (M7)")]
    pub async fn load(&self, request: &LoadRequest) -> Result<LoadReport, EngineError> {
        Err(not_built(&format!("vc load {}", request.scenario)))
    }
}

#[cfg(test)]
mod tests {
    use vc_testkit::FakeVcs;

    use super::LoadRequest;
    use crate::sample::engine;

    #[tokio::test]
    async fn load_says_it_is_not_built() {
        let request = LoadRequest { scenario: "users.peak#browses".to_owned(), ..LoadRequest::default() };
        let loaded = engine(FakeVcs::default()).load(&request).await;
        assert_eq!(
            loaded.err().map(|error| error.to_string()).as_deref(),
            Some("vc load users.peak#browses is not built yet")
        );
    }
}
