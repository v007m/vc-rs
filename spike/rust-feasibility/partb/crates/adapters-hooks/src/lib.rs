//! TypeScript hooks executed in `QuickJS`. The runtime is `!Send`, so it lives on
//! one OS thread; the async [`HookRunner`] side only sends jobs over a channel.
mod marshal;
mod sandbox;

use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use ir::{FileName, HookRef, VarName};
use ports::{HookRunner, HttpRequest, PortError, PortErrorKind};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

pub use sandbox::Limits;

pub struct HookConfig {
    /// Hook module paths are resolved against this directory.
    pub root: PathBuf,
    pub modules: Vec<FileName>,
    pub limits: Limits,
    pub secrets: HashMap<String, String>,
}

struct Job {
    module: FileName,
    export: VarName,
    request: Value,
    reply: oneshot::Sender<Result<Value, String>>,
}

pub struct QjsHookRunner {
    jobs: mpsc::Sender<Job>,
}

fn hook_error(message: impl Into<String>) -> PortError {
    PortError::new(PortErrorKind::Hook, message)
}

impl QjsHookRunner {
    /// Start the hook thread and load (type-strip + evaluate) every module.
    ///
    /// # Errors
    /// The first module that fails to read, strip or evaluate.
    pub fn start(config: HookConfig) -> Result<Self, PortError> {
        let (jobs, rx) = mpsc::channel::<Job>(64);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || match sandbox::Sandbox::load_all(&config) {
            Ok(sandbox) => {
                let _ = ready_tx.send(Ok(()));
                serve(&sandbox, rx);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e));
            }
        });
        ready_rx.recv().map_err(|e| hook_error(e.to_string()))?.map_err(hook_error)?;
        Ok(Self { jobs })
    }
}

fn serve(sandbox: &sandbox::Sandbox, mut rx: mpsc::Receiver<Job>) {
    while let Some(job) = rx.blocking_recv() {
        let out = sandbox.call(&job.module, &job.export, &job.request);
        let _ = job.reply.send(out);
    }
}

#[async_trait]
impl HookRunner for QjsHookRunner {
    async fn before_request(&self, hook: &HookRef, request: HttpRequest) -> Result<HttpRequest, PortError> {
        let (reply, response) = oneshot::channel();
        let job = Job {
            module: hook.module.clone(),
            export: hook.export.clone(),
            request: marshal::request_to_json(&request),
            reply,
        };
        self.jobs.send(job).await.map_err(|_| hook_error("hook thread stopped"))?;
        let value = response.await.map_err(|_| hook_error("hook thread dropped the job"))?.map_err(hook_error)?;
        marshal::request_from_json(&value)
            .ok_or_else(|| hook_error(format!("hook {} returned an invalid request", hook.export)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use ir::{HeaderName, Method, UrlPath};

    fn runner() -> QjsHookRunner {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../hooks");
        let secrets = HashMap::from([("UPLOAD_KEY".to_owned(), "k".to_owned())]);
        let limits = Limits { timeout: Duration::from_millis(200), memory_bytes: 32 << 20 };
        QjsHookRunner::start(HookConfig { root, modules: vec![FileName::new("sign.ts")], limits, secrets }).unwrap()
    }

    #[tokio::test]
    async fn sign_hook_adds_hmac_header() {
        let request = HttpRequest { method: Method::Post, path: UrlPath::new("/p"), headers: vec![], body: None };
        let hook = HookRef { module: FileName::new("sign.ts"), export: VarName::new("addSignature") };
        let signed = runner().before_request(&hook, request).await.unwrap();
        let sig = signed.headers.iter().find(|(k, _)| k == &HeaderName::new("x-signature")).map(|(_, v)| v.clone());
        assert_eq!(sig.as_deref(), Some(sandbox::hmac_sha256_hex("k", "/p").as_str()));
    }
}
