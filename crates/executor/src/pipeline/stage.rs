use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use tower::{Service, ServiceExt};
use vc_interfaces::TransportError;
use vc_model::RenderedRequest;

use super::{Call, Hop};

/// Changes a request before it is sent: auth adds credentials, hooks run `beforeRequest`.
#[async_trait]
pub trait RequestStage: Send + Sync {
    /// The request to send instead of `request`.
    ///
    /// # Errors
    ///
    /// When the request cannot be prepared; it is then not sent.
    async fn apply(&self, request: RenderedRequest) -> Result<RenderedRequest, TransportError>;
}

/// The null stage: passes the request on as it is.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Unchanged;

#[async_trait]
impl RequestStage for Unchanged {
    async fn apply(&self, request: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        Ok(request)
    }
}

/// Runs a [`RequestStage`] on each attempt before passing it on.
#[derive(Clone)]
pub struct Stage<S> {
    inner: S,
    stage: Arc<dyn RequestStage>,
}

impl<S> Stage<S> {
    /// Applies `stage` to attempts sent through `inner`.
    pub fn new(inner: S, stage: Arc<dyn RequestStage>) -> Self {
        Self { inner, stage }
    }
}

impl<S: fmt::Debug> fmt::Debug for Stage<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stage").field("inner", &self.inner).finish_non_exhaustive()
    }
}

impl<S: Hop> Service<Call> for Stage<S> {
    ready_service!();

    fn call(&mut self, mut call: Call) -> Self::Future {
        let (inner, stage) = (self.inner.clone(), Arc::clone(&self.stage));
        Box::pin(async move {
            call.request = stage.apply(call.request).await?;
            inner.oneshot(call).await
        })
    }
}
