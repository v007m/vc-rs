//! The sign stage of the pipeline (D443): it sits inside the per-host limit, after the hooks,
//! so the signer sees every header the request leaves with and dates it as it leaves.

use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{AuthError, Clock, TransportError};
use vc_model::RenderedRequest;

use super::stage::AuthStage;
use crate::RequestStage;

/// Signs each attempt with the [`Signer`](vc_interfaces::Signer) of the call's credentials,
/// which the auth stage has already fetched, at the time the clock tells.
pub struct SignStage {
    auth: Arc<AuthStage>,
    clock: Option<Arc<dyn Clock>>,
}

impl SignStage {
    pub fn new(auth: Arc<AuthStage>, clock: Option<Arc<dyn Clock>>) -> Self {
        Self { auth, clock }
    }
}

#[async_trait]
impl RequestStage for SignStage {
    async fn apply(&self, request: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        let Some(clock) = &self.clock else {
            return Err(TransportError::Credentials(AuthError::Unknown { kind: self.auth.kind().to_owned() }));
        };
        let credentials = self.auth.credentials().await.map_err(TransportError::Credentials)?;
        let Some(signer) = credentials.signer else {
            let (auth, missing) = (self.auth.name().to_owned(), "signer".to_owned());
            return Err(TransportError::Credentials(AuthError::Malformed { auth, missing }));
        };
        signer.sign(request, clock.now_ms()).map_err(TransportError::Credentials)
    }
}

#[cfg(test)]
#[path = "sign_tests.rs"]
mod tests;
