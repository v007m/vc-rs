use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider};
use vc_model::SourcePath;

/// A project held in memory: each file a relative path and its bytes.
///
/// Paths are taken as written, so a fixture can hold names the file system would refuse.
#[derive(Clone, Debug, Default)]
pub struct MemorySources {
    files: Vec<(String, Vec<u8>)>,
}

impl MemorySources {
    /// A project with these files.
    #[must_use]
    pub fn new(files: &[(&str, &[u8])]) -> Self {
        Self { files: files.iter().map(|&(path, bytes)| (path.to_owned(), bytes.to_vec())).collect() }
    }
}

impl SourceProvider for MemorySources {
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError> {
        let mut discovery = Discovery::default();
        for (relative, bytes) in &self.files {
            match filter
                .select(relative)
                .map(|path| path.and_then(|path| vc_interfaces::source_file(path, bytes.clone())))
            {
                Some(Ok(file)) => discovery.files.push(file),
                Some(Err(problem)) => discovery.problems.push(problem),
                None => discovery.assets.extend(filter.asset(relative)),
            }
        }
        discovery.sort();
        Ok(discovery)
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        let mut discovery = Discovery::default();
        let mut unique = paths.to_vec();
        unique.sort();
        unique.dedup();
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

impl MemorySources {
    /// The file at `path`, or why it cannot be read.
    fn read_one(&self, path: &SourcePath) -> Result<vc_model::SourceFile, SourceError> {
        vc_interfaces::source_file(path.clone(), self.read_bytes(path)?)
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{SourceError, SourceFilter, SourceProvider};

    use super::MemorySources;
    use crate::source_provider_suite;

    #[test]
    fn passes_the_source_provider_suite() {
        source_provider_suite(&mut |files| Box::new(MemorySources::new(files)));
    }

    #[test]
    fn reports_names_a_project_cannot_use() {
        let sources = MemorySources::new(&[("k:v.api.yaml", b""), ("ok.api.yaml", b"")]);
        let discovery = sources.discover(&SourceFilter::default()).unwrap();
        assert_eq!(discovery.files.len(), 1);
        assert_eq!(discovery.problems, [SourceError::InvalidPath { path: "k\u{fffd}v.api.yaml".to_owned() }]);
    }
}
