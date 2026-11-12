//! A project held in memory, as a [`SourceProvider`]: what the docs playground runs and what
//! tests hand the engine. And the secret schema, found the same way by every composition root.

use vc_model::{DEFAULT_SECRETS_SCHEMA, SourceFile, SourcePath};

use crate::{Discovery, SourceError, SourceFilter, SourceProvider};

/// A project held in memory: each file a project path and its bytes.
///
/// Paths are taken as written, so a fixture can hold names the file system would refuse.
/// Nothing is outside it, so it keeps the default [`SourceProvider::read_outside`]: a data file
/// under `datasets.allowRoots` is a file that cannot be read (V1012).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemorySources {
    files: Vec<(String, Vec<u8>)>,
}

impl MemorySources {
    /// A project with these files.
    #[must_use]
    pub fn new(files: &[(&str, &[u8])]) -> Self {
        Self { files: files.iter().map(|&(path, bytes)| (path.to_owned(), bytes.to_vec())).collect() }
    }

    /// A project of `files`, each a project path and its contents.
    #[must_use]
    pub fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self { files: files.into_iter().collect() }
    }

    fn read_one(&self, path: &SourcePath) -> Result<SourceFile, SourceError> {
        crate::source_file(path.clone(), self.read_bytes(path)?)
    }
}

impl SourceProvider for MemorySources {
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError> {
        let mut discovery = Discovery::default();
        for (relative, bytes) in &self.files {
            match filter.select(relative).map(|path| path.and_then(|path| crate::source_file(path, bytes.clone()))) {
                Some(Ok(file)) => discovery.files.push(file),
                Some(Err(problem)) => discovery.problems.push(problem),
                None => discovery.assets.extend(filter.asset(relative)),
            }
        }
        discovery.sort();
        Ok(discovery)
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        let mut unique = paths.to_vec();
        unique.sort();
        unique.dedup();
        let mut discovery = Discovery::default();
        for path in &unique {
            match self.read_one(path) {
                Ok(file) => discovery.files.push(file),
                Err(problem) => discovery.problems.push(problem),
            }
        }
        discovery.sort();
        Ok(discovery)
    }

    fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>, SourceError> {
        let held = self.files.iter().find(|(relative, _)| relative == path.as_str());
        held.map(|(_, bytes)| bytes.clone()).ok_or_else(|| SourceError::Missing { path: path.to_string() })
    }
}

/// `.env.schema` at the project root, read when `found` lists it as an asset (D165).
///
/// A schema that cannot be read joins `found`'s problems, so it is a V1012 error rather than no
/// schema.
pub fn secret_schema(sources: &(impl SourceProvider + ?Sized), found: &mut Discovery) -> Option<SourceFile> {
    let path = SourcePath::new(DEFAULT_SECRETS_SCHEMA).ok()?;
    if !found.assets.contains(&path) {
        return None;
    }
    match sources.read(&[path]) {
        Ok(mut read) => {
            found.problems.append(&mut read.problems);
            read.files.pop()
        }
        Err(error) => {
            found.problems.push(error);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use vc_model::SourcePath;

    use super::{MemorySources, secret_schema};
    use crate::{Discovery, SourceError, SourceFilter, SourceProvider};

    #[test]
    fn files_given_as_owned_pairs_are_the_same_project() {
        let owned = MemorySources::from_files([("a.json".to_owned(), b"{}".to_vec())]);
        assert_eq!(owned, MemorySources::new(&[("a.json", b"{}")]));
    }

    #[test]
    fn the_secret_schema_is_read_only_when_discovery_listed_it() {
        let sources = MemorySources::new(&[("x.test.yaml", b"tests: []\n"), (".env.schema", b"TOKEN=\n")]);
        let mut found = sources.discover(&SourceFilter::default()).unwrap();
        assert_eq!(secret_schema(&sources, &mut found).map(|file| file.text), Some("TOKEN=\n".to_owned()));
        let mut unlisted = Discovery::default();
        assert_eq!(secret_schema(&sources, &mut unlisted), None);
    }

    #[test]
    fn a_listed_schema_that_cannot_be_read_is_a_problem() {
        let sources = MemorySources::default();
        let mut found = Discovery { assets: vec![SourcePath::new(".env.schema").unwrap()], ..Discovery::default() };
        assert_eq!(secret_schema(&sources, &mut found), None);
        assert_eq!(found.problems, [SourceError::Missing { path: ".env.schema".to_owned() }]);
    }
}
