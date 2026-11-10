//! [`StdioPluginHost`]: a plugin run as a child process, spoken to in JSON-RPC 2.0 over its
//! stdin and stdout.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::{OnceCell, broadcast, mpsc, oneshot};
use vc_interfaces::{INITIALIZE_DEADLINE, PluginError, PluginHost, PluginManifest, PluginPermissions, Secrets};

use crate::owned;
use crate::session::{Call, Session};
use crate::stderr_log;

/// The protocol version the host speaks, sent in `initialize`.
pub const PROTOCOL_VERSION: u32 = 1;

/// The only environment variables a plugin's process gets: enough to find programs and a
/// temporary folder, never the run's secrets.
const PASSED_ENV: [&str; 9] = ["PATH", "HOME", "USER", "LANG", "TMPDIR", "TEMP", "TMP", "SYSTEMROOT", "USERPROFILE"];

/// The environment variable naming the folder a plugin keeps its own files in: its install
/// folder is checked against `vibranium.lock` and must not change.
pub const DATA_ENV: &str = "VIB_PLUGIN_DATA";

/// A [`PluginHost`] that starts the plugin's `entry` in its folder on the first call.
///
/// Its stdin and stdout are piped; its stderr is passed through, or kept in a log in a folder
/// the host owns when one is given. An `entry.command` naming a file in the folder runs that file; any other
/// is found on `PATH`.
///
/// It refuses to start a plugin whose manifest asks for more than `vibranium.lock` granted, and
/// answers the plugin's `secrets/get` only for the secrets both the manifest declares and the lock
/// grants (−32001 otherwise). The process gets no environment beyond [`PASSED_ENV`] and, when the
/// host has one, [`DATA_ENV`] naming its data folder, made as it starts. One process serves every
/// call; once it stops, every later call fails, and dropping the host shuts it down.
#[derive(Debug)]
pub struct StdioPluginHost {
    manifest: PluginManifest,
    dir: PathBuf,
    granted: PluginPermissions,
    secrets: Secrets,
    config: Value,
    data: Option<(PathBuf, PathBuf)>,
    log: Option<(PathBuf, PathBuf)>,
    told: Option<broadcast::Sender<String>>,
    calls: OnceCell<Result<mpsc::UnboundedSender<Call>, PluginError>>,
}

impl StdioPluginHost {
    /// The plugin of `manifest`, installed in `dir`, allowed what `granted` holds.
    #[must_use]
    pub fn new(manifest: PluginManifest, dir: impl Into<PathBuf>, granted: PluginPermissions) -> Self {
        let dir = dir.into();
        let (secrets, config, calls) = (Secrets::new(), Value::Null, OnceCell::new());
        Self { manifest, dir, granted, secrets, config, data: None, log: None, told: None, calls }
    }

    /// The same host, able to give the plugin those of `run`'s secrets that the manifest declares
    /// and the lock grants.
    #[must_use]
    pub fn secrets(self, run: &Secrets) -> Self {
        let allowed = |name: &str| {
            let listed = |names: &[String]| names.iter().any(|listed| listed == name);
            listed(&self.manifest.permissions.secrets) && listed(&self.granted.secrets)
        };
        let mut secrets = Secrets::new();
        for (name, secret) in run.iter().filter(|(name, _)| allowed(name)) {
            secrets.insert(name, secret.clone());
        }
        Self { secrets, ..self }
    }

    /// The same host, handing `config` (the project's `plugins[].config`) to `initialize`.
    #[must_use]
    pub fn config(self, config: Value) -> Self {
        Self { config, ..self }
    }

    /// The same host, giving the plugin the folder `under` in `home` for its own files
    /// ([`DATA_ENV`]). Each folder on the way from `home` is made when missing as the plugin
    /// starts; when one is a link or not a folder, the plugin is not started.
    #[must_use]
    pub fn data(self, home: impl Into<PathBuf>, under: impl Into<PathBuf>) -> Self {
        Self { data: Some((home.into(), under.into())), ..self }
    }

    /// The same host, keeping the plugin's stderr in [`crate::STDERR_LOG`] in the folder `under`
    /// in `home` rather than passing it through, up to [`crate::LOG_LIMIT`] a start, the log
    /// before kept as `stderr.log.1`. The folder must be the host's own, never the plugin's data
    /// folder; the plugin is never told of it. Each folder on the way from `home` is made when
    /// missing; when one is a link or the log cannot be made, the stderr is dropped, and
    /// [`StdioPluginHost::told`] says why.
    #[must_use]
    pub fn stderr_log(self, home: impl Into<PathBuf>, under: impl Into<PathBuf>) -> Self {
        Self { log: Some((home.into(), under.into())), ..self }
    }

    /// The same host, sending `told` one line when the plugin starts with its stderr dropped as
    /// its log could not be made: `plugin `<id>`: its stderr is dropped: <why>`.
    #[must_use]
    pub fn told(self, told: broadcast::Sender<String>) -> Self {
        Self { told: Some(told), ..self }
    }

    fn plugin(&self) -> String {
        self.manifest.id.clone()
    }

    /// The process of the plugin's `entry`, with [`DATA_ENV`] when the host has a data folder;
    /// with a log folder, its stderr is piped when `logged` (the log could be made), else dropped.
    fn command(&self, logged: bool) -> Command {
        let mut command = command(&self.manifest, &self.dir);
        if let Some((home, under)) = &self.data {
            command.env(DATA_ENV, home.join(under));
        }
        if self.log.is_some() {
            command.stderr(if logged { Stdio::piped() } else { Stdio::null() });
        }
        command
    }

    /// Tells whoever asked ([`StdioPluginHost::told`]) why the plugin's stderr is dropped.
    fn unlogged(&self, why: &std::io::Error) {
        if let Some(told) = &self.told {
            told.send(format!("plugin `{}`: its stderr is dropped: {why}", self.plugin())).ok();
        }
    }

    /// The channel to the running plugin: started, and initialized, on the first call.
    async fn running(&self) -> Result<mpsc::UnboundedSender<Call>, PluginError> {
        self.calls.get_or_init(|| self.start()).await.clone()
    }

    async fn start(&self) -> Result<mpsc::UnboundedSender<Call>, PluginError> {
        let beyond = self.manifest.permissions.beyond(&self.granted);
        if !beyond.is_empty() {
            let what = format!("{}, which vibranium.lock does not grant", beyond.join(", "));
            return Err(PluginError::Permission { plugin: self.plugin(), what });
        }
        let not_started = |reason: String| PluginError::Start { plugin: self.plugin(), reason };
        if let Some((home, under)) = &self.data {
            let made = owned::folder(home, under);
            made.map_err(|error| not_started(format!("cannot make its data folder: {error}")))?;
        }
        let log = self.log.as_ref().map(|(home, under)| stderr_log::open(home, under));
        let log = log.and_then(|opened| opened.inspect_err(|why| self.unlogged(why)).ok());
        let mut child = self
            .command(log.is_some())
            .spawn()
            .map_err(|error| not_started(format!("cannot run `{}`: {error}", self.manifest.command)))?;
        if let (Some(stderr), Some(log)) = (child.stderr.take(), log) {
            tokio::spawn(stderr_log::keep(stderr, log));
        }
        let stdin = child.stdin.take().ok_or_else(|| not_started("it has no stdin".to_owned()))?;
        let (calls, receiver) = mpsc::unbounded_channel();
        tokio::spawn(Session::new(self.plugin(), stdin, self.secrets.clone()).run(receiver, child));
        let params = json!({"protocolVersion": PROTOCOL_VERSION, "plugin": self.plugin(), "config": self.config});
        let initialized = ask(&calls, ("initialize", params), INITIALIZE_DEADLINE).await;
        initialized.map_err(|error| not_started(format!("`initialize` failed: {error}")))?;
        Ok(calls)
    }

    async fn ask(&self, (method, params): (&str, Value), deadline: Duration) -> Result<Value, PluginError> {
        let calls = self.running().await?;
        ask(&calls, (method, params), deadline).await.map_err(|failure| failure.of(self.plugin(), method, deadline))
    }
}

#[async_trait]
impl PluginHost for StdioPluginHost {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn call(&self, method: &str, params: Value, deadline: Duration) -> Result<Value, PluginError> {
        self.ask((method, params), deadline).await
    }
}

/// Why a call got no result.
#[derive(Debug)]
enum Failure {
    /// The plugin's own error, or the host's for it.
    Plugin(PluginError),
    /// Past the deadline.
    Late,
    /// The plugin stopped before it answered.
    Gone,
}

impl Failure {
    fn of(self, plugin: String, method: &str, after: Duration) -> PluginError {
        match self {
            Self::Plugin(error) => error,
            Self::Late => PluginError::Deadline { plugin, method: method.to_owned(), after },
            Self::Gone => PluginError::Crashed { plugin, reason: "it stopped earlier in this run".to_owned() },
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Plugin(error) => error.fmt(f),
            Self::Late => f.write_str("no answer in time"),
            Self::Gone => f.write_str("it stopped"),
        }
    }
}

/// Sends `method` to the session and waits up to `deadline` for its answer.
async fn ask(
    calls: &mpsc::UnboundedSender<Call>,
    (method, params): (&str, Value),
    deadline: Duration,
) -> Result<Value, Failure> {
    let (reply, answer) = oneshot::channel();
    calls.send(Call { method: method.to_owned(), params, reply, deadline }).map_err(|_| Failure::Gone)?;
    match tokio::time::timeout(deadline, answer).await {
        Ok(Ok(result)) => result.map_err(Failure::Plugin),
        Ok(Err(_)) => Err(Failure::Gone),
        Err(_) => Err(Failure::Late),
    }
}

/// The plugin's `entry`, run in `dir`: a `command` naming a file in `dir` runs that file, any
/// other is found on `PATH`.
fn command(manifest: &PluginManifest, dir: &Path) -> Command {
    let local = dir.join(&manifest.command);
    let program = if local.is_file() { local } else { PathBuf::from(&manifest.command) };
    let passed = PASSED_ENV.iter().filter_map(|name| std::env::var(name).ok().map(|value| (*name, value)));
    let mut command = Command::new(program);
    command.args(&manifest.args).current_dir(dir).env_clear().envs(passed).kill_on_drop(true);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    command
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
