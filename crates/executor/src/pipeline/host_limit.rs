use std::collections::HashMap;
use std::sync::Arc;

use http::Uri;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tower::{Service, ServiceExt};
use vc_interfaces::TransportError;

use super::{Call, Hop};

/// A request for a host's semaphore, with where to send it.
type Ask = (String, oneshot::Sender<Arc<Semaphore>>);

/// The per-host semaphores of one run (`defaults.perHost`).
///
/// One actor task owns the map from host to semaphore and hands each caller the host's
/// `Arc<Semaphore>`, created on first use; callers acquire permits themselves, outside the actor,
/// so the actor only does a map lookup per request. Clones talk to the same actor. The map is
/// bounded by the distinct hosts a run talks to, and is dropped with the run's last handle.
#[derive(Clone, Debug)]
pub struct HostLimits {
    asks: mpsc::UnboundedSender<Ask>,
}

impl HostLimits {
    /// Starts the actor on the current tokio runtime; each host gets `per_host` permits (at
    /// least one).
    #[must_use]
    pub fn spawn(per_host: usize) -> Self {
        let (asks, mut queue) = mpsc::unbounded_channel::<Ask>();
        let permits = per_host.max(1);
        tokio::spawn(async move {
            let mut hosts: HashMap<String, Arc<Semaphore>> = HashMap::new();
            while let Some((host, reply)) = queue.recv().await {
                let semaphore = hosts.entry(host).or_insert_with(|| Arc::new(Semaphore::new(permits)));
                let _ = reply.send(Arc::clone(semaphore));
            }
        });
        Self { asks }
    }

    /// The semaphore for `host`; `None` once the actor has stopped.
    async fn semaphore(&self, host: String) -> Option<Arc<Semaphore>> {
        let (reply, answer) = oneshot::channel();
        self.asks.send((host, reply)).ok()?;
        answer.await.ok()
    }
}

/// Holds one of the target host's permits for the whole attempt.
#[derive(Clone, Debug)]
pub struct HostLimit<S> {
    inner: S,
    limits: HostLimits,
}

impl<S> HostLimit<S> {
    /// Limits attempts sent through `inner` by `limits`.
    pub const fn new(inner: S, limits: HostLimits) -> Self {
        Self { inner, limits }
    }
}

impl<S: Hop> Service<Call> for HostLimit<S> {
    ready_service!();

    fn call(&mut self, call: Call) -> Self::Future {
        let (inner, limits) = (self.inner.clone(), self.limits.clone());
        Box::pin(async move {
            let stopped = || TransportError::Protocol { reason: "the per-host limiter has stopped".to_owned() };
            let semaphore = limits.semaphore(host_key(&call.request.uri)).await.ok_or_else(stopped)?;
            let _permit = semaphore.acquire_owned().await.map_err(|_| stopped())?;
            inner.oneshot(call).await
        })
    }
}

/// `host:port` in lower case, with the scheme's default port when none is given.
fn host_key(uri: &Uri) -> String {
    let host = uri.host().unwrap_or_default().to_ascii_lowercase();
    let default = if uri.scheme_str() == Some("https") { 443 } else { 80 };
    let port = uri.port_u16().unwrap_or(default);
    format!("{host}:{port}")
}

#[cfg(test)]
mod tests {
    use http::Uri;

    use super::host_key;

    #[test]
    fn keys_by_lower_case_host_and_port_with_scheme_defaults() {
        assert_eq!(host_key(&Uri::from_static("http://API.test/x")), "api.test:80");
        assert_eq!(host_key(&Uri::from_static("https://api.test/x")), "api.test:443");
        assert_eq!(host_key(&Uri::from_static("https://api.test:8443/")), "api.test:8443");
    }
}
