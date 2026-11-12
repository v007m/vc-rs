//! This user's private folder for the demo's pidfiles and joins (`vc demo`): the pidfile `stop`
//! reads, and one file per `start` that joined a demo another one launched.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};

/// How many joins one caller can hold on a demo at once.
const JOINS: u32 = 64;

/// How long a join counts after its caller is gone: the caller may be a shell that ran `start`
/// and ended, while the run around it still uses the demo.
const JOINED: Duration = Duration::from_hours(1);

/// The pidfile of the demo on `port`, in this user's [`folder`] so `stop` finds it from any
/// folder; `None` when that folder is not this user's own.
pub fn pidfile(port: u16) -> Option<PathBuf> {
    folder().map(|folder| folder.join(format!("{port}.pid")))
}

/// How long a `start` waits for its turn while another `start` on the port launches its demo:
/// that one gives it up within its wait for the demo to answer.
const TURN: Duration = Duration::from_secs(15);

const POLL: Duration = Duration::from_millis(50);

/// Waits for this `start`'s turn on `port`, so two `start`s at once take turns: the second finds
/// the first's demo answering and joins it, instead of seeing the port taken before the demo
/// answers. The turn is an OS lock on `<port>.lock`, held until the file is dropped and freed when
/// `vc` exits, so a crash leaves none behind; `None` (no turn, as before) when there is nowhere
/// private for it, or another `start` holds it past [`TURN`].
pub fn turn(port: u16) -> Option<fs::File> {
    let path = folder()?.join(format!("{port}.lock"));
    let file = fs::OpenOptions::new().create(true).truncate(false).write(true).open(path).ok()?;
    let deadline = Instant::now() + TURN;
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => thread::sleep(POLL),
            Err(_) => return None,
        }
    }
}

/// The folder with one file per `start` that joined the demo on `port`, named `<pid>.<n>` for
/// the process that called it: each still uses the demo while that process lives, and for
/// [`JOINED`] after it joined.
pub fn users(port: u16) -> Option<PathBuf> {
    folder().map(|folder| folder.join(format!("{port}.users")))
}

/// This user's folder for the demo's pidfiles and joins, made private to the user. The temp
/// folder may be shared, so it is named for the user and refused unless it is the user's own
/// folder, not a link.
fn folder() -> Option<PathBuf> {
    let folder = env::temp_dir().join(folder_name());
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(&folder).ok()?;
    own_folder(&folder).then_some(folder)
}

#[cfg(unix)]
pub fn folder_name() -> String {
    format!("vibranium-demo-{}", rustix::process::getuid().as_raw())
}

#[cfg(not(unix))]
pub fn folder_name() -> String {
    "vibranium-demo".into()
}

/// Notes that this `start` uses a demo another one launched; when it cannot, the next `stop`
/// ends the demo as before.
pub fn join(port: u16) {
    let Some(folder) = users(port) else { return };
    if !fs::create_dir_all(&folder).is_ok_and(|()| own_folder(&folder)) {
        return;
    }
    let caller = caller();
    for n in 0..JOINS {
        if fs::File::create_new(folder.join(format!("{caller}.{n}"))).is_ok() {
            break;
        }
    }
}

/// Takes one joined `start` that still [`counts`] off the demo on `port`, and drops those that
/// no longer do (the caller crashed before its `stop`); whether one was taken, so the demo keeps
/// running.
pub fn left_to_another(port: u16) -> bool {
    let Some(folder) = users(port).filter(|folder| own_folder(folder)) else { return false };
    let Ok(entries) = fs::read_dir(&folder) else { return false };
    let mut taken = false;
    for entry in entries.flatten() {
        let Some(pid) = joined_by(&entry.file_name().to_string_lossy()) else { continue };
        let live = counts(&entry, pid);
        if !taken || !live {
            let removed = fs::remove_file(entry.path()).is_ok();
            taken |= removed && live;
        }
    }
    taken
}

/// Whether a join still keeps the demo up: its caller runs, or it joined less than [`JOINED`]
/// ago.
fn counts(entry: &fs::DirEntry, pid: u32) -> bool {
    let age = entry.metadata().and_then(|metadata| metadata.modified()).ok().and_then(|made| made.elapsed().ok());
    age.is_some_and(|age| age < JOINED) || alive(pid)
}

/// The pid a joined file `<pid>.<n>` names.
fn joined_by(name: &str) -> Option<u32> {
    name.split_once('.').and_then(|(pid, _)| pid.parse().ok())
}

/// The process that called this `start`, which outlives it: the run whose `command:` it is (a
/// one-command shell line runs it in the shell's place), or the user's shell. A longer shell
/// line ends before the run does, so a join also counts for [`JOINED`].
#[cfg(unix)]
fn caller() -> u32 {
    u32::try_from(rustix::process::Pid::as_raw(rustix::process::getppid())).unwrap_or_else(|_| process::id())
}

#[cfg(not(unix))]
fn caller() -> u32 {
    process::id()
}

/// Whether the process `pid` still runs; one of another user's counts.
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid).ok().and_then(rustix::process::Pid::from_raw) else { return false };
    matches!(rustix::process::test_kill_process(pid), Ok(()) | Err(rustix::io::Errno::PERM))
}

/// Elsewhere a join counts until its `stop`, or for [`JOINED`].
#[cfg(not(unix))]
const fn alive(_pid: u32) -> bool {
    false
}

/// Whether `path` is a folder this user owns, not a link: the temp folder may be shared, and
/// another user's link there is never followed.
fn own_folder(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else { return false };
    #[cfg(unix)]
    let mine = std::os::unix::fs::MetadataExt::uid(&metadata) == rustix::process::getuid().as_raw();
    #[cfg(not(unix))]
    let mine = true;
    metadata.is_dir() && mine
}
