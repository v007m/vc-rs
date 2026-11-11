//! [`StdioPluginHost`]: a plugin run as a child process, spoken to in JSON-RPC 2.0 over its
//! stdin and stdout.

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::{OnceCell, broadcast, mpsc, oneshot};
use vc_interfaces::{INITIALIZE_DEADLINE, PluginError, PluginHost, PluginManifest, PluginPermissions, Secrets};

use crate::confine::{self, Confiner, Entry, Policy};
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
/// call; once it stops, every later call fails, and dropping the host shuts it down. A confined
/// host ([`StdioPluginHost::confined`]) runs it held to its manifest's permissions where the OS
/// can (D254). Where it cannot, the plugin runs unconfined and the host tells why, except on
/// macOS, where it is not started (V5119) unless [`StdioPluginHost::allow_unconfined`] (D260).
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
    confined: Option<(PathBuf, Arc<Confiner>)>,
    unconfined: bool,
    calls: OnceCell<Result<mpsc::UnboundedSender<Call>, PluginError>>,
}

impl StdioPluginHost {
    /// The plugin of `manifest`, installed in `dir`, allowed what `granted` holds.
    #[must_use]
    pub fn new(manifest: PluginManifest, dir: impl Into<PathBuf>, granted: PluginPermissions) -> Self {
        let dir = dir.into();
        let (secrets, config, calls) = (Secrets::new(), Value::Null, OnceCell::new());
        let (data, log, told, confined) = (None, None, None, None);
        Self { manifest, dir, granted, secrets, config, data, log, told, confined, unconfined: false, calls }
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
    /// its log could not be made, `plugin `<id>`: its stderr is dropped: <why>`, one when it
    /// starts unconfined, `plugin `<id>` runs unconfined: <why>`, and one for each file it may
    /// write but cannot make until it exists (on Linux, at the top of the project).
    #[must_use]
    pub fn told(self, told: broadcast::Sender<String>) -> Self {
        Self { told: Some(told), ..self }
    }

    /// The same host, holding the plugin to its manifest's permissions in the project at `root`
    /// where the OS can, as `confiner` does: Landlock on Linux through its launcher, `vc` itself
    /// (no launcher, no confinement), and Seatbelt on macOS.
    #[must_use]
    pub fn confined(self, root: impl Into<PathBuf>, confiner: Arc<Confiner>) -> Self {
        Self { confined: Some((root.into(), confiner)), ..self }
    }

    /// The same host, running the plugin unconfined, with a warning, where it cannot be confined
    /// when `allowed`, as on Linux and other OSes; else, on macOS, not starting it (D260). Only
    /// the user's own settings may allow it, never the project's.
    #[must_use]
    pub fn allow_unconfined(self, allowed: bool) -> Self {
        Self { unconfined: allowed, ..self }
    }

    fn plugin(&self) -> String {
        self.manifest.id.clone()
    }

    /// What runs the plugin's `entry`: confined to its policy when the host is confined and the
    /// OS can; otherwise as [`StdioPluginHost::unconfined`] says. Not started when a file it
    /// names is outside the project.
    async fn entry(&self) -> Result<Entry, PluginError> {
        let not_started = |reason: String| PluginError::Start { plugin: self.plugin(), reason };
        let (manifest, dir) = (self.manifest.clone(), self.dir.clone());
        let data = self.data.as_ref().map(|(home, under)| home.join(under));
        let within = self.confined.as_ref().map(|(root, _)| (root.clone(), data));
        let planned = tokio::task::spawn_blocking(move || plan(&manifest, &dir, within)).await;
        let (entry, planned) = planned.map_err(|error| not_started(error.to_string()))?.map_err(not_started)?;
        let (Some((policy, closed)), Some((_, confiner))) = (planned, &self.confined) else {
            return Ok(entry);
        };
        match confiner.ready().await {
            Ok(()) => {
                self.cannot_make(&policy.root, &closed);
                confiner.entry(&policy, entry).map_err(not_started)
            }
            Err(why) => self.unconfined(entry, why),
        }
    }

    /// Tells whoever asked ([`StdioPluginHost::told`]) of each file in the project at `root`
    /// the plugin may write but cannot make, as it does not exist yet ([`confine::uncreatable`]).
    fn cannot_make(&self, root: &Path, closed: &[PathBuf]) {
        for path in closed {
            let shown = path.strip_prefix(root).unwrap_or(path).display();
            self.tell(format!(
                "plugin `{}` cannot make `{shown}`, which it may write, until it exists: create the file \
                 first, or grant a path in a folder of the project",
                self.plugin()
            ));
        }
    }

    /// `entry` run unconfined, as the OS cannot confine it (`why`), telling whoever asked
    /// ([`StdioPluginHost::told`]); on macOS, unless allowed, not started (D260).
    fn unconfined(&self, entry: Entry, why: String) -> Result<Entry, PluginError> {
        if confine::FAIL_CLOSED && !self.unconfined {
            return Err(PluginError::Unconfined { plugin: self.plugin(), reason: why });
        }
        self.tell(format!("plugin `{}` runs unconfined: {why}", self.plugin()));
        Ok(entry)
    }

    /// The process running `entry`, with [`DATA_ENV`] when the host has a data folder; with a
    /// log folder, its stderr is piped when `logged` (the log could be made), else dropped.
    fn command(&self, entry: Entry, logged: bool) -> Command {
        let mut command = command(entry, &self.dir);
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
        self.tell(format!("plugin `{}`: its stderr is dropped: {why}", self.plugin()));
    }

    /// Sends `line` to whoever asked ([`StdioPluginHost::told`]).
    fn tell(&self, line: String) {
        if let Some(told) = &self.told {
            told.send(line).ok();
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
        let (data, log) = (self.data.clone(), self.log.clone());
        let prepared = tokio::task::spawn_blocking(move || prepare(data.as_ref(), log.as_ref())).await;
        let made = prepared.map_err(|error| not_started(format!("cannot make its folders: {error}")))?;
        let log = made.map_err(|error| not_started(format!("cannot make its data folder: {error}")))?;
        let log = log.and_then(|opened| opened.inspect_err(|why| self.unlogged(why)).ok());
        let entry = self.entry().await?;
        let mut child = self
            .command(entry, log.is_some())
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

/// Makes the plugin's data folder, when it has one, then opens its log, when it has a folder
/// for one: blocking file-system calls, so the host runs them off the runtime's workers.
///
/// # Errors
///
/// Why the data folder cannot be used; the log's own failure comes back inside, as the plugin
/// still starts without it.
fn prepare(
    data: Option<&(PathBuf, PathBuf)>,
    log: Option<&(PathBuf, PathBuf)>,
) -> io::Result<Option<io::Result<File>>> {
    if let Some((home, under)) = data {
        owned::folder(home, under)?;
    }
    Ok(log.map(|(home, under)| stderr_log::open(home, under)))
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

/// A plugin's policy and the files it may write but cannot make ([`confine::uncreatable`]).
type Confinement = (Policy, Vec<PathBuf>);

/// The plugin's `entry` in `dir` and, when it runs confined `within` the project at a root with
/// a data folder, its policy and the files it cannot make ([`confine::uncreatable`]); why not, when its
/// manifest names a file outside the project. Blocking file-system calls, so the host runs them
/// off the runtime's workers.
fn plan(
    manifest: &PluginManifest,
    dir: &Path,
    within: Option<(PathBuf, Option<PathBuf>)>,
) -> Result<(Entry, Option<Confinement>), String> {
    let planned = match within {
        Some((root, data)) => {
            let policy = Policy::new(&root, dir, &manifest.permissions)?.writing(data.as_deref());
            let closed = confine::uncreatable(&policy);
            Some((policy, closed))
        }
        None => None,
    };
    Ok((entry(manifest, dir), planned))
}

/// The plugin's `entry` in `dir`: a `command` naming a file in `dir` runs that file, any other is
/// found on `PATH`.
fn entry(manifest: &PluginManifest, dir: &Path) -> Entry {
    let local = dir.join(&manifest.command);
    let program = if local.is_file() { local } else { PathBuf::from(&manifest.command) };
    (program, manifest.args.iter().map(OsString::from).collect())
}

/// `entry` run in `dir` with only [`PASSED_ENV`].
fn command((program, args): Entry, dir: &Path) -> Command {
    let passed = PASSED_ENV.iter().filter_map(|name| std::env::var(name).ok().map(|value| (*name, value)));
    let mut command = Command::new(program);
    command.args(args).current_dir(dir).env_clear().envs(passed).kill_on_drop(true);
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
    command
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
