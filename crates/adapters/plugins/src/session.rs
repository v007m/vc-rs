//! One running plugin process, owned by a single task: it writes the host's calls to the
//! plugin's stdin, matches the answers on its stdout to them, and answers the plugin's own
//! requests (`secrets/get`) from the secrets it was granted.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::BufReader;
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use vc_interfaces::{PERMISSION_DENIED, PluginError, Secrets};

use crate::framing;
use crate::rpc::{self, Incoming, METHOD_NOT_FOUND};

/// How long a plugin has to answer `shutdown`, and then to exit, before it is killed.
const GRACE: Duration = Duration::from_secs(1);

/// What a call is answered with.
pub type Reply = oneshot::Sender<Result<Value, PluginError>>;

/// What the plugin sent: a message, or why its stdout is not the protocol.
pub type Received = Result<Value, String>;

/// One call of the host's.
#[derive(Debug)]
pub struct Call {
    pub method: String,
    pub params: Value,
    pub reply: Reply,
}

/// What the session does next.
enum Next {
    Call(Option<Call>),
    Message(Option<Received>),
}

/// The task's state: the plugin's stdin, its granted secrets and the calls it has not answered.
pub struct Session {
    plugin: String,
    stdin: ChildStdin,
    secrets: Secrets,
    pending: HashMap<u64, (String, Reply)>,
    last: u64,
}

impl Session {
    /// A session writing to `stdin` of plugin `plugin`, which may read `secrets`.
    pub fn new(plugin: String, stdin: ChildStdin, secrets: Secrets) -> Self {
        Self { plugin, stdin, secrets, pending: HashMap::new(), last: 0 }
    }

    /// Serves `calls` until the host drops them, then shuts the plugin down; or until the
    /// plugin stops or breaks the protocol, which fails every call it has not answered.
    pub async fn run(mut self, mut calls: mpsc::UnboundedReceiver<Call>, mut child: Child) {
        let Some(stdout) = child.stdout.take() else { return };
        let mut incoming = messages(stdout);
        loop {
            let next = tokio::select! {
                call = calls.recv() => Next::Call(call),
                message = incoming.recv() => Next::Message(message),
            };
            match next {
                Next::Call(Some(call)) => self.send(call).await,
                Next::Call(None) => return self.shutdown(&mut incoming, child).await,
                Next::Message(Some(Ok(message))) => self.receive(message).await,
                Next::Message(Some(Err(reason))) => {
                    return self.fail(&PluginError::Protocol { plugin: self.plugin.clone(), reason });
                }
                Next::Message(None) => {
                    let reason = exited(&mut child).await;
                    return self.fail(&PluginError::Crashed { plugin: self.plugin.clone(), reason });
                }
            }
        }
    }

    /// Writes `call` as the next request; a call whose caller gave up is forgotten first.
    async fn send(&mut self, call: Call) {
        self.pending.retain(|_, (_, reply)| !reply.is_closed());
        self.last += 1;
        let request = rpc::request(self.last, &call.method, &call.params);
        match framing::write(&mut self.stdin, &request).await {
            Ok(()) => drop(self.pending.insert(self.last, (call.method, call.reply))),
            Err(error) => {
                let reason = format!("cannot write to it: {error}");
                drop(call.reply.send(Err(PluginError::Crashed { plugin: self.plugin.clone(), reason })));
            }
        }
    }

    /// Hands an answer to its caller, or answers a request of the plugin's.
    async fn receive(&mut self, message: Value) {
        match rpc::classify(message) {
            Ok(Incoming::Answer { id, outcome }) => self.answer(id, outcome),
            Ok(Incoming::Request { id, method, params }) => {
                let answer = self.serve(&id, &method, &params);
                if let Err(error) = framing::write(&mut self.stdin, &answer).await {
                    let reason = format!("cannot write to it: {error}");
                    self.fail(&PluginError::Crashed { plugin: self.plugin.clone(), reason });
                }
            }
            Ok(Incoming::Notice) => {}
            Err(reason) => self.fail(&PluginError::Protocol { plugin: self.plugin.clone(), reason }),
        }
    }

    fn answer(&mut self, id: u64, outcome: Result<Value, (i64, String)>) {
        let Some((method, reply)) = self.pending.remove(&id) else { return };
        let plugin = self.plugin.clone();
        let result = outcome.map_err(|(code, message)| {
            let message = redact(&message, &self.secrets);
            PluginError::Remote { plugin, method, code, message }
        });
        drop(reply.send(result));
    }

    /// The answer to the plugin's request `method`: a granted secret, or a refusal.
    fn serve(&self, id: &Value, method: &str, params: &Value) -> Value {
        if method != "secrets/get" {
            return rpc::error(id, METHOD_NOT_FOUND, &format!("the host has no method `{method}`"));
        }
        let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
        let Some(secret) = self.secrets.get(name) else {
            let refusal = format!("secret `{name}` is not granted to plugin `{}`", self.plugin);
            return rpc::error(id, PERMISSION_DENIED, &refusal);
        };
        rpc::result(id, &json!({"value": secret.expose()}))
    }

    /// Fails every call not yet answered with `error`.
    fn fail(&mut self, error: &PluginError) {
        for (_, (_, reply)) in self.pending.drain() {
            drop(reply.send(Err(error.clone())));
        }
    }

    /// Asks the plugin to shut down, closes its stdin and waits for it to exit; a plugin that
    /// takes longer than [`GRACE`] for either is killed when `child` drops.
    async fn shutdown(mut self, incoming: &mut mpsc::Receiver<Received>, mut child: Child) {
        self.last += 1;
        let asked = framing::write(&mut self.stdin, &rpc::request(self.last, "shutdown", &Value::Null)).await;
        if asked.is_ok() {
            let _ = tokio::time::timeout(GRACE, answered(incoming, self.last)).await;
        }
        drop(self.stdin);
        let _ = tokio::time::timeout(GRACE, child.wait()).await;
    }
}

/// Waits for the answer to request `id`, or the end of the plugin's stdout.
async fn answered(incoming: &mut mpsc::Receiver<Received>, id: u64) {
    while let Some(Ok(message)) = incoming.recv().await {
        if matches!(rpc::classify(message), Ok(Incoming::Answer { id: answered, .. }) if answered == id) {
            return;
        }
    }
}

/// The messages on `stdout`, read by a task of their own so a half-read message is never lost
/// when the session is busy; the last one is the reason the rest is not the protocol.
fn messages(stdout: ChildStdout) -> mpsc::Receiver<Received> {
    let (sender, receiver) = mpsc::channel(64);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        while let Some(read) = framing::read(&mut reader).await.transpose() {
            let broken = read.is_err();
            if sender.send(read).await.is_err() || broken {
                break;
            }
        }
    });
    receiver
}

/// `message` with the value of every secret in `secrets` masked.
fn redact(message: &str, secrets: &Secrets) -> String {
    let values = secrets.iter().map(|(_, secret)| secret.expose()).filter(|value| !value.is_empty());
    values.fold(message.to_owned(), |text, value| text.replace(value, "***"))
}

/// How the plugin ended: its exit status, once it has one within [`GRACE`].
async fn exited(child: &mut Child) -> String {
    match tokio::time::timeout(GRACE, child.wait()).await {
        Ok(Ok(status)) => format!("it exited with {status}"),
        Ok(Err(error)) => format!("its output ended: {error}"),
        Err(_) => "it closed its output".to_owned(),
    }
}
