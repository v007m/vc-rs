use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use vc_interfaces::{CompileCacheError, CompileCacheStore};

/// The folders under the project root that hold the cache, outermost first.
const FOLDERS: [&str; 2] = [".vibranium", "cache"];
const FILE: &str = "compile.bin";
/// Where the blob lives, relative to the project root: the location errors name.
const LOCATION: &str = ".vibranium/cache/compile.bin";

/// Keeps the compile cache blob in `.vibranium/cache/compile.bin` under the project root.
///
/// A save writes a fresh temporary file next to the blob and renames it over the blob, so a
/// reader sees the old blob or the new one, never half of one. Neither a load nor a save goes
/// through a link: if `.vibranium`, `.vibranium/cache` or the blob is a link, or not the kind
/// of entry expected, the call fails instead of touching what it leads to.
#[derive(Clone, Debug)]
pub struct FsCompileCache {
    root: PathBuf,
}

impl FsCompileCache {
    /// A store for the project at `root`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    /// The cache folder, checked one part at a time; `Ok(None)` when a part is missing and
    /// `create` is false.
    fn folder(&self, create: bool) -> io::Result<Option<PathBuf>> {
        let mut folder = self.root.clone();
        for part in FOLDERS {
            folder.push(part);
            match fs::symlink_metadata(&folder) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => return Err(io::Error::other(format!("{} is not a plain folder", folder.display()))),
                Err(error) if error.kind() == ErrorKind::NotFound && create => make_folder(&folder)?,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error),
            }
        }
        Ok(Some(folder))
    }

    fn read(&self) -> io::Result<Option<Vec<u8>>> {
        let Some(folder) = self.folder(false)? else { return Ok(None) };
        let blob = folder.join(FILE);
        match fs::symlink_metadata(&blob) {
            Ok(meta) if meta.is_file() => fs::read(&blob).map(Some),
            Ok(_) => Err(io::Error::other(format!("{LOCATION} is not a plain file"))),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let folder = self.folder(true)?.ok_or_else(|| io::Error::other("the cache folder vanished"))?;
        let temp = folder.join(temp_name());
        let written = OpenOptions::new().write(true).create_new(true).open(&temp).and_then(|file| fill(file, bytes));
        let moved = written.and_then(|()| fs::rename(&temp, folder.join(FILE)));
        if moved.is_err() {
            let _ignored = fs::remove_file(&temp);
        }
        moved
    }
}

impl CompileCacheStore for FsCompileCache {
    fn load(&self) -> Result<Option<Vec<u8>>, CompileCacheError> {
        self.read()
            .map_err(|error| CompileCacheError::Read { location: LOCATION.to_owned(), reason: error.to_string() })
    }

    fn save(&self, bytes: &[u8]) -> Result<(), CompileCacheError> {
        self.write(bytes)
            .map_err(|error| CompileCacheError::Write { location: LOCATION.to_owned(), reason: error.to_string() })
    }
}

fn fill(mut file: File, bytes: &[u8]) -> io::Result<()> {
    file.write_all(bytes)?;
    file.sync_all()
}

/// A name no other writer uses at the same time: this process, this instant.
fn temp_name() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
    format!("{FILE}.{}.{nanos}.tmp", std::process::id())
}

/// Creates `folder`. Another run that creates it first is fine, as long as what it made is a
/// plain folder.
fn make_folder(folder: &Path) -> io::Result<()> {
    match fs::create_dir(folder) {
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            let plain = fs::symlink_metadata(folder)?.is_dir();
            if plain { Ok(()) } else { Err(io::Error::other(format!("{} is not a plain folder", folder.display()))) }
        }
        made => made,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vc_interfaces::CompileCacheStore;
    use vc_testkit::compile_cache_store_suite;

    use super::{FsCompileCache, make_folder};

    #[test]
    fn meets_the_compile_cache_store_contract() {
        let mut dirs = Vec::new();
        compile_cache_store_suite(&mut || {
            let dir = tempfile::tempdir().unwrap();
            let store = FsCompileCache::new(dir.path());
            dirs.push(dir);
            Box::new(store)
        });
        assert!(!dirs.is_empty(), "the suite built stores");
    }

    #[test]
    fn keeps_the_blob_on_disk_for_the_next_run() {
        let dir = tempfile::tempdir().unwrap();
        FsCompileCache::new(dir.path()).save(b"blob").unwrap();
        assert_eq!(FsCompileCache::new(dir.path()).load().unwrap().as_deref(), Some(&b"blob"[..]));
        let names: Vec<_> =
            fs::read_dir(dir.path().join(".vibranium/cache")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["compile.bin"], "no temporary file is left behind");
    }

    #[test]
    fn a_folder_another_run_made_first_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(".vibranium");
        make_folder(&folder).unwrap();
        make_folder(&folder).unwrap();
        fs::write(dir.path().join("file"), b"").unwrap();
        assert!(make_folder(&dir.path().join("file")).is_err(), "a file in the way is refused");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_another_run_made_first_is_refused() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join(".vibranium")).unwrap();
        assert!(make_folder(&dir.path().join(".vibranium")).is_err());
    }

    #[test]
    fn refuses_a_cache_folder_that_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".vibranium"), b"").unwrap();
        let store = FsCompileCache::new(dir.path());
        assert!(store.load().is_err() && store.save(b"blob").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn never_writes_through_a_link() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join(".vibranium")).unwrap();
        let store = FsCompileCache::new(dir.path());
        assert!(store.save(b"blob").is_err() && store.load().is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0, "nothing written outside the root");
    }

    #[cfg(unix)]
    #[test]
    fn never_reads_a_linked_blob() {
        use std::os::unix::fs::symlink;
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), b"secret").unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".vibranium/cache")).unwrap();
        symlink(outside.path().join("secret"), dir.path().join(".vibranium/cache/compile.bin")).unwrap();
        assert!(FsCompileCache::new(dir.path()).load().is_err());
    }
}
