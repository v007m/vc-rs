//! The web UI's git pane through the `git` command line: the status, a file's diff, and a
//! commit, push or pull (D546–D550).
//!
//! Each runs as [`run`] does. Push and pull use the user's own credentials, from their helper or
//! agent, and never ask for them on a terminal (`GIT_TERMINAL_PROMPT=0`, no stdin); one that
//! still waits, as SSH does for a key's passphrase, is stopped after two minutes (D554). Every
//! message has the credentials of any URL in it hidden ([`without_credentials`]).

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};
use std::{io, thread};

use vc_interfaces::{VcsChange, VcsCommit, VcsError, VcsFile, VcsStatus, without_credentials};
use vc_model::SourcePath;

const STATUS: &str = "read the status";
const DIFF: &str = "diff";
const COMMIT: &str = "commit";

/// How long a push or pull may run before it is stopped (D554).
const LIMIT: Duration = Duration::from_secs(120);

/// How often a push or pull is checked on while it runs.
const POLL: Duration = Duration::from_millis(20);

/// How long a stream of a push or pull that has exited is still read, so a process it left
/// behind holding the stream never holds the pane up.
const DRAIN: Duration = Duration::from_millis(200);

/// A file's line counts: added, removed, and whether it is binary.
type Counts = (u32, u32, bool);

/// `git args` in `root`, every argument on its own and never through a shell, with no hook from
/// `core.fsmonitor`, paths printed as they are, pathspecs taken literally, and no prompt.
pub fn run(root: &Path, args: &[&str]) -> Result<Output, VcsError> {
    git(root, args).output().map_err(|error| unavailable(&error))
}

/// The `git args` command [`run`] runs.
fn git(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).args(["-c", "core.fsmonitor=false", "-c", "core.quotePath=false"]);
    command.arg("--literal-pathspecs").env("GIT_TERMINAL_PROMPT", "0").args(args);
    command
}

/// git could not be started.
fn unavailable(error: &io::Error) -> VcsError {
    VcsError::Unavailable { reason: without_credentials(&error.to_string()) }
}

/// The branch, its upstream, how far apart they are, and the changed files (D546).
pub fn status(root: &Path) -> Result<VcsStatus, VcsError> {
    ran(root, (STATUS, &["rev-parse", "--git-dir"]))?;
    let branch = optional(root, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let upstream = optional(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"])?;
    let (behind, ahead) = if upstream.is_empty() { (0, 0) } else { apart(root)? };
    Ok(VcsStatus { branch, upstream, ahead, behind, files: files(root)? })
}

/// The unified diff of `path` against the last commit, or of the whole file when it is
/// untracked (D547). External diff tools and text conversions are not run.
pub fn diff(root: &Path, path: &SourcePath) -> Result<String, VcsError> {
    let path = path.as_str();
    let untracked = listed(root, (DIFF, &["ls-files", "--others", "--exclude-standard", "-z", "--", path]))?;
    if untracked.is_empty() {
        let base = base(root)?;
        let args = ["diff", "--no-ext-diff", "--no-textconv", "--no-color", "--no-renames", &base, "--", path];
        return said(root, (DIFF, &args));
    }
    let output = run(root, &["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--no-color", "--", "/dev/null", path])?;
    match output.status.code() {
        Some(0 | 1) => Ok(without_credentials(&String::from_utf8_lossy(&output.stdout))),
        _ => Err(failed(DIFF, &message(&output))),
    }
}

/// Stages `commit.paths` as they are now and commits them, and only them; the short ID (D548).
pub fn commit(root: &Path, commit: &VcsCommit) -> Result<String, VcsError> {
    if commit.message.trim().is_empty() || commit.paths.is_empty() {
        return Err(failed(COMMIT, "name at least one file and give a message"));
    }
    let paths: Vec<&str> = commit.paths.iter().map(SourcePath::as_str).collect();
    ran(root, (COMMIT, &[&["add", "-A", "--"][..], &paths].concat()))?;
    let only = ["commit", "--quiet", "--only", "-m", &commit.message, "--"];
    ran(root, (COMMIT, &[&only[..], &paths].concat()))?;
    Ok(said(root, (COMMIT, &["rev-parse", "--short", "HEAD"]))?.trim().to_owned())
}

/// Pushes the branch to its upstream; what git said (D549).
pub fn push(root: &Path) -> Result<String, VcsError> {
    timed(root, ("push", &["push"]), LIMIT)
}

/// Fast-forwards the branch to its upstream, never merging or rebasing; what git said (D549).
pub fn pull(root: &Path) -> Result<String, VcsError> {
    timed(root, ("pull", &["pull", "--no-rebase", "--ff-only"]), LIMIT)
}

/// What `git args` said, run as [`run`] runs it with no stdin; once it has run for `limit`, it
/// and every process it started (on unix, its process group, such as `ssh`) are killed and it
/// fails as timed out (D554).
pub fn timed(root: &Path, (action, args): (&str, &[&str]), limit: Duration) -> Result<String, VcsError> {
    let mut command = git(root, args);
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn().map_err(|error| unavailable(&error))?;
    let (stdout, stderr) = (drained(child.stdout.take()), drained(child.stderr.take()));
    let Some(status) = waited(&mut child, limit).map_err(|error| unavailable(&error))? else {
        return Err(stopped(&mut child, (action, limit)));
    };
    let output = Output { status, stdout: read(&stdout), stderr: read(&stderr) };
    if status.success() { Ok(message(&output)) } else { Err(failed(action, &message(&output))) }
}

/// Kills `child` and what it started, which ran past `limit`, and says why it was stopped.
fn stopped(child: &mut Child, (action, limit): (&str, Duration)) -> VcsError {
    crate::command::kill_group(child.id());
    child.kill().ok();
    child.wait().ok();
    let reason = format!(
        "git {action} timed out after {} s; it may be waiting for an SSH passphrase. Run it in a terminal or add the key to ssh-agent",
        limit.as_secs()
    );
    failed(action, &reason)
}

/// The exit status of `child` once it has exited, or `None` once it has run for `limit`.
fn waited(child: &mut Child, limit: Duration) -> io::Result<Option<ExitStatus>> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if started.elapsed() >= limit {
            return Ok(None);
        }
        thread::sleep(POLL);
    }
}

/// Everything `stream` gives until it ends, read on a thread of its own so a full pipe never
/// stops git.
fn drained(stream: Option<impl io::Read + Send + 'static>) -> Receiver<Vec<u8>> {
    let (send, read) = mpsc::channel();
    if let Some(mut stream) = stream {
        drop(thread::spawn(move || {
            let mut bytes = Vec::new();
            drop(stream.read_to_end(&mut bytes));
            drop(send.send(bytes));
        }));
    }
    read
}

/// What a stream gave, waiting at most [`DRAIN`] for it to end.
fn read(stream: &Receiver<Vec<u8>>) -> Vec<u8> {
    stream.recv_timeout(DRAIN).unwrap_or_default()
}

/// Commits behind and ahead of the upstream.
fn apart(root: &Path) -> Result<(u32, u32), VcsError> {
    let counts = said(root, (STATUS, &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"]))?;
    let mut parts = counts.split_whitespace().map(|count| count.parse::<u32>().unwrap_or_default());
    Ok((parts.next().unwrap_or_default(), parts.next().unwrap_or_default()))
}

/// The changed files under `root`, by path: tracked changes since the last commit, conflicts,
/// and untracked files that are not ignored.
fn files(root: &Path) -> Result<Vec<VcsFile>, VcsError> {
    let base = base(root)?;
    let mut files = BTreeMap::new();
    let changes = listed(root, (STATUS, &["diff", "--name-status", "-z", "--no-renames", "--relative", &base, "--"]))?;
    for pair in changes.chunks_exact(2) {
        if let [letter, path] = pair {
            add(&mut files, (path, change(letter)), (0, 0, false));
        }
    }
    for path in listed(root, (STATUS, &["diff", "--name-only", "--diff-filter=U", "-z", "--relative", "--"]))? {
        add(&mut files, (&path, VcsChange::Conflicted), (0, 0, false));
    }
    for (path, counts) in counts(root, &base)? {
        if let Some(file) = files.get_mut(&path) {
            (file.added, file.removed, file.binary) = counts;
        }
    }
    for path in listed(root, (STATUS, &["ls-files", "--others", "--exclude-standard", "-z", "--"]))? {
        add(&mut files, (&path, VcsChange::Untracked), lines(&root.join(&path)));
    }
    Ok(files.into_values().collect())
}

/// Adds the file at `path` to `files` when the project can name it.
fn add(files: &mut BTreeMap<SourcePath, VcsFile>, (path, change): (&str, VcsChange), counts: Counts) {
    if let Ok(path) = SourcePath::new(path) {
        files.insert(path.clone(), file(path, (change, counts)));
    }
}

fn file(path: SourcePath, (change, (added, removed, binary)): (VcsChange, Counts)) -> VcsFile {
    VcsFile { path, change, added, removed, binary }
}

/// The change git's `--name-status` letter stands for; a type change counts as modified.
fn change(letter: &str) -> VcsChange {
    match letter {
        "A" => VcsChange::Added,
        "D" => VcsChange::Deleted,
        "U" => VcsChange::Conflicted,
        _ => VcsChange::Modified,
    }
}

/// Each tracked file's line counts since `base`, from `--numstat` (`-` counts mean binary).
fn counts(root: &Path, base: &str) -> Result<Vec<(SourcePath, Counts)>, VcsError> {
    let lines = listed(root, (STATUS, &["diff", "--numstat", "-z", "--no-renames", "--relative", base, "--"]))?;
    Ok(lines
        .iter()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (added, removed, path) = (parts.next()?, parts.next()?, parts.next()?);
            let binary = added == "-" && removed == "-";
            let count = |text: &str| text.parse::<u32>().unwrap_or_default();
            Some((SourcePath::new(path).ok()?, (count(added), count(removed), binary)))
        })
        .collect())
}

/// An untracked file's line count, and whether it is binary (a NUL in its first 8000 bytes, as
/// git decides). A link is one line, its target, and is never followed.
fn lines(path: &Path) -> Counts {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
    let bytes = if is_link { b"link\n".to_vec() } else { std::fs::read(path).unwrap_or_default() };
    if bytes.iter().take(8000).any(|byte| *byte == 0) {
        return (0, 0, true);
    }
    let newlines = bytes.iter().filter(|byte| **byte == b'\n').count();
    let unended = usize::from(bytes.last().is_some_and(|byte| *byte != b'\n'));
    (u32::try_from(newlines + unended).unwrap_or(u32::MAX), 0, false)
}

/// `HEAD`, or the empty tree before the first commit.
fn base(root: &Path) -> Result<String, VcsError> {
    if run(root, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?.status.success() {
        return Ok("HEAD".to_owned());
    }
    Ok(said(root, (STATUS, &["hash-object", "-t", "tree", "--stdin"]))?.trim().to_owned())
}

/// What `git args` printed, trimmed, or nothing when it failed, as when there is no upstream.
fn optional(root: &Path, args: &[&str]) -> Result<String, VcsError> {
    let output = run(root, args)?;
    let text = if output.status.success() { String::from_utf8_lossy(&output.stdout).trim().to_owned() } else { String::new() };
    Ok(without_credentials(&text))
}

/// The `\0`-separated entries `git args` printed.
fn listed(root: &Path, (action, args): (&str, &[&str])) -> Result<Vec<String>, VcsError> {
    let output = ran(root, (action, args))?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.split('\0').filter(|entry| !entry.is_empty()).map(str::to_owned).collect())
}

/// What `git args` printed.
fn said(root: &Path, (action, args): (&str, &[&str])) -> Result<String, VcsError> {
    ran(root, (action, args)).map(|output| without_credentials(&String::from_utf8_lossy(&output.stdout)))
}

/// What `git args` gave when it worked, or a [`VcsError::Action`] naming `action` with git's
/// message when it did not.
fn ran(root: &Path, (action, args): (&str, &[&str])) -> Result<Output, VcsError> {
    let output = run(root, args)?;
    if output.status.success() { Ok(output) } else { Err(failed(action, &message(&output))) }
}

/// Everything git said on both streams, trimmed, without credentials.
fn message(output: &Output) -> String {
    let streams = [&output.stdout, &output.stderr].map(|bytes| String::from_utf8_lossy(bytes).trim().to_owned());
    without_credentials(&streams.into_iter().filter(|text| !text.is_empty()).collect::<Vec<_>>().join("\n"))
}

fn failed(action: &str, reason: &str) -> VcsError {
    VcsError::Action { action: action.to_owned(), reason: reason.to_owned() }
}
