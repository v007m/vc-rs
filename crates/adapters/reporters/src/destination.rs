use std::fmt::Debug;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use vc_interfaces::ReportError;

/// Where reports go: console text, and report files in the run's report folder.
pub trait Destination: Send + Sync + Debug {
    /// Writes `text` to the console as it is; once its reader has gone (a closed pipe, as in
    /// `vc run | head -1`), the text is dropped and the run goes on.
    ///
    /// # Errors
    ///
    /// [`ReportError::Write`] when the console cannot be written for another reason.
    fn print(&self, text: &str) -> Result<(), ReportError>;

    /// Saves a whole report file called `name`, which may name a subfolder (`data/index.js`),
    /// and returns where it is.
    ///
    /// # Errors
    ///
    /// [`ReportError::Write`] when the folder or file cannot be written.
    fn save(&self, name: &str, contents: &[u8]) -> Result<PathBuf, ReportError>;
}

/// Console text to stdout, report files under a folder (created when missing).
#[derive(Clone, Debug)]
pub struct FsDestination {
    dir: PathBuf,
    /// Whether console text is printed; an MCP run's stdout carries the protocol instead.
    console: bool,
}

impl FsDestination {
    /// Report files go under `dir`, such as `.vibranium/reports/483`.
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into(), console: true }
    }

    /// The same destination with console text dropped, when `quiet`.
    #[must_use]
    pub fn quiet(self, quiet: bool) -> Self {
        Self { console: self.console && !quiet, ..self }
    }
}

impl Destination for FsDestination {
    fn print(&self, text: &str) -> Result<(), ReportError> {
        if !self.console {
            return Ok(());
        }
        let mut out = std::io::stdout().lock();
        match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
            Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => Err(write_error("console", &error)),
            _ => Ok(()),
        }
    }

    fn save(&self, name: &str, contents: &[u8]) -> Result<PathBuf, ReportError> {
        let path = self.dir.join(name);
        let dir = path.parent().unwrap_or(&self.dir);
        fs::create_dir_all(dir).map_err(|e| write_error(&shown(dir), &e))?;
        fs::write(&path, contents).map_err(|e| write_error(&shown(&path), &e))?;
        Ok(path)
    }
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

fn write_error(location: &str, error: &std::io::Error) -> ReportError {
    ReportError::Write { location: location.to_owned(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use vc_interfaces::ReportError;

    use super::{Destination, FsDestination};

    #[test]
    fn saves_files_under_a_new_folder() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("reports/7");
        let saved = FsDestination::new(&dir).save("ctrf.json", b"{}").unwrap();
        assert_eq!(saved, dir.join("ctrf.json"));
        assert_eq!(fs::read(&saved).unwrap(), b"{}");
        FsDestination::new(&dir).print("").unwrap();
        let nested = FsDestination::new(&dir).save("data/tests/0.js", b"x").unwrap();
        assert_eq!(fs::read(nested).unwrap(), b"x");
    }

    #[test]
    fn a_quiet_destination_prints_nothing_but_still_saves() {
        let root = tempfile::tempdir().unwrap();
        let quiet = FsDestination::new(root.path()).quiet(true);
        assert!(!quiet.console);
        quiet.print("not shown").unwrap();
        assert!(quiet.save("a.json", b"{}").unwrap().exists());
        assert!(FsDestination::new(root.path()).quiet(false).console);
    }

    #[test]
    fn says_which_path_cannot_be_written() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("taken");
        fs::write(&file, "x").unwrap();
        let err = FsDestination::new(&file).save("ctrf.json", b"{}").unwrap_err();
        assert!(matches!(&err, ReportError::Write { location, .. } if *location == file.display().to_string()));
        fs::create_dir(root.path().join("dir.json")).unwrap();
        let err = FsDestination::new(root.path()).save("dir.json", b"{}").unwrap_err();
        let expected = root.path().join("dir.json").display().to_string();
        assert!(matches!(&err, ReportError::Write { location, .. } if *location == expected));
    }
}
