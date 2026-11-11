//! One running plugin process, owned by a single task: it writes the host's calls to the
//! plugin's stdin, matches the answers on its stdout to them, and answers the plugin's own
//! requests (`secrets/get`) from the secrets it was granted.

use std::collections::HashMap;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncRead, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{mpsc, oneshot};
use vc_interfaces::{CALL_DEADLINE, PERMISSION_DENIED, PluginError, Secrets};

use crate::framing;
use crate::rpc::{self, Incoming, METHOD_NOT_FOUND};

/// How long a plugin has to answer `shutdown`, and then to exit, before it is killed.
const GRACE: Duration = Duration::from_secs(1);

/// The call that ends the session: `shutdown` sent to the plugin, then its stdin closed. It is
/// answered once the plugin has exited or been killed.
pub const SHUTDOWN: &str = "shutdown";

/// How long ending a plugin takes at most: [`GRACE`] to write `shutdown`, to have it answered
/// and to exit, and a little more for the kill.
pub const ENDING: Duration = Duration::from_millis(3_500);

/// How many of the plugin's messages may wait unread before the session gives up on it.
const ROOM: usize = 1024;

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
    /// How long writing the call to the plugin may take: the call's own deadline.
    pub deadline: Duration,
}

/// What the session does next.
enum Next {
    Call(Option<Call>),
    /// A [`SHUTDOWN`] call from the host.
    Shutdown(Call),
    Message(Option<Received>),
}

impl Next {
    /// `call`, or [`Next::Shutdown`] when it asks the session to shut down.
    fn call(call: Option<Call>) -> Self {
        match call {
            Some(call) if call.method == SHUTDOWN => Self::Shutdown(call),
            call => Self::Call(call),
        }
    }
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

    /// Serves `calls` until the host sends [`SHUTDOWN`] or drops them, then shuts the plugin
    /// down, answering [`SHUTDOWN`] once it has; or until the plugin stops, breaks the protocol,
    /// cannot be written to in time or leaves more than [`ROOM`] messages unread, which fails every call it has not answered and ends the session
    /// (killing the plugin), so every later call fails at once.
    pub async fn run(mut self, mut calls: mpsc::UnboundedReceiver<Call>, mut child: Child) {
        let Some(stdout) = child.stdout.take() else { return };
        let mut incoming = messages(stdout, ROOM);
        loop {
            let next = tokio::select! {
                call = calls.recv() => Next::call(call),
                message = incoming.recv() => Next::Message(message),
            };
            let step = match next {
                Next::Shutdown(call) => {
                    self.shutdown(&mut incoming, child).await;
                    drop(call.reply.send(Ok(Value::Null)));
                    return;
                }
                Next::Call(Some(call)) => self.send(call).await,
                Next::Call(None) => return self.shutdown(&mut incoming, child).await,
                Next::Message(Some(Ok(message))) => self.receive(message).await,
                Next::Message(Some(Err(reason))) => Err(PluginError::Protocol { plugin: self.plugin.clone(), reason }),
                Next::Message(None) => Err(self.crashed(exited(&mut child).await)),
            };
            if let Err(error) = step {
                return self.fail(&error);
            }
        }
    }

    /// Writes `call` as the next request; a call whose caller gave up is forgotten first. A
    /// plugin that cannot be written to within the call's deadline fails `call` and ends the
    /// session.
    async fn send(&mut self, call: Call) -> Result<(), PluginError> {
        self.pending.retain(|_, (_, reply)| !reply.is_closed());
        self.last += 1;
        let request = rpc::request(self.last, &call.method, &call.params);
        if let Err(error) = self.write(&request, call.deadline).await {
            drop(call.reply.send(Err(error.clone())));
            return Err(error);
        }
        drop(self.pending.insert(self.last, (call.method, call.reply)));
        Ok(())
    }

    /// Writes `message` to the plugin's stdin, which it must read within `within`.
    async fn write(&mut self, message: &Value, within: Duration) -> Result<(), PluginError> {
        match tokio::time::timeout(within, framing::write(&mut self.stdin, message)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(self.crashed(format!("cannot write to it: {error}"))),
            Err(_) => Err(self.crashed(format!("it did not read its input within {within:?}"))),
        }
    }

    /// Hands an answer to its caller, or answers a request of the plugin's. A message that is
    /// not JSON-RPC, or an answer that cannot be written, ends the session.
    async fn receive(&mut self, message: Value) -> Result<(), PluginError> {
        match rpc::classify(message) {
            Ok(Incoming::Answer { id, outcome }) => self.answer(id, outcome),
            Ok(Incoming::Request { id, method, params }) => {
                let answer = self.serve(&id, &method, &params);
                self.write(&answer, CALL_DEADLINE).await?;
            }
            Ok(Incoming::Notice) => {}
            Err(reason) => return Err(PluginError::Protocol { plugin: self.plugin.clone(), reason }),
        }
        Ok(())
    }

    fn crashed(&self, reason: String) -> PluginError {
        PluginError::Crashed { plugin: self.plugin.clone(), reason }
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
        let asked = self.write(&rpc::request(self.last, "shutdown", &Value::Null), GRACE).await;
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
///
/// The reader never waits on the queue, so the plugin's stdout is drained even while the session
/// waits on a write to its stdin (otherwise a plugin blocked on a full stdout stops reading its
/// stdin, and the two wait on each other). At most `room` messages wait: the last place is kept
/// for the reason a plugin that writes faster than it is read is given up on.
fn messages(stdout: impl AsyncRead + Unpin + Send + 'static, room: usize) -> mpsc::Receiver<Received> {
    let (sender, receiver) = mpsc::channel(room);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        while let Some(read) = framing::read(&mut reader).await.transpose() {
            let broken = read.is_err();
            let full = sender.capacity() <= 1;
            let read = if full { Err(flooded(room)) } else { read };
            if sender.try_send(read).is_err() || broken || full {
                break;
            }
        }
    });
    receiver
}

/// Why a plugin that leaves `room` messages unread is given up on.
fn flooded(room: usize) -> String {
    format!("it wrote more than {} messages the host had not read yet", room - 1)
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

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
