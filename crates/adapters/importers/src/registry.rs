use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use vc_interfaces::{ImportError, Importer, SpecFetcher};

use crate::{CurlImporter, HarImporter, OpenApiImporter};

/// Builds one importer over the fetcher for the documents a file refers to.
pub type ImporterFactory = Arc<dyn Fn(Arc<dyn SpecFetcher>) -> Box<dyn Importer> + Send + Sync>;

/// The importers, by kind (`vc import <kind>`).
#[derive(Clone, Default)]
pub struct ImporterRegistry {
    factories: BTreeMap<String, ImporterFactory>,
}

impl ImporterRegistry {
    /// A registry with no importers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with every built-in importer: `curl`, `har` and `openapi`.
    #[must_use]
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register("curl", Arc::new(|_| Box::new(CurlImporter)));
        registry.register("har", Arc::new(|_| Box::new(HarImporter)));
        registry.register("openapi", Arc::new(|fetcher| Box::new(OpenApiImporter::new(fetcher))));
        registry
    }

    /// Adds `factory` under `kind`, replacing any importer already there.
    pub fn register(&mut self, kind: impl Into<String>, factory: ImporterFactory) {
        self.factories.insert(kind.into(), factory);
    }

    /// The registered kinds, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.factories.keys().map(String::as_str)
    }

    /// The importer registered as `kind`, fetching through `fetcher`.
    ///
    /// # Errors
    ///
    /// [`ImportError::UnknownKind`] when no importer has that kind.
    pub fn create(&self, kind: &str, fetcher: Arc<dyn SpecFetcher>) -> Result<Box<dyn Importer>, ImportError> {
        let unknown =
            || ImportError::UnknownKind { kind: kind.to_owned(), known: self.names().map(str::to_owned).collect() };
        self.factories.get(kind).map(|factory| factory(fetcher)).ok_or_else(unknown)
    }
}

impl fmt::Debug for ImporterRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImporterRegistry").field("kinds", &self.names().collect::<Vec<_>>()).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use vc_interfaces::{FetchError, ImportError, ImportSource, SpecFetcher};
    use vc_testkit::{FakeImporter, importer_suite};

    use super::ImporterRegistry;

    struct NoFetch;

    #[async_trait]
    impl SpecFetcher for NoFetch {
        async fn fetch(&self, location: &str) -> Result<Vec<u8>, FetchError> {
            Err(FetchError::NotAllowed { location: location.to_owned() })
        }

        fn decode(&self, location: &str, _bytes: &[u8]) -> Result<serde_json::Value, FetchError> {
            Err(FetchError::Undecodable { location: location.to_owned(), reason: "never".to_owned() })
        }
    }

    fn registry() -> ImporterRegistry {
        let mut registry = ImporterRegistry::new();
        registry.register("fake", Arc::new(|_| Box::new(FakeImporter::default())));
        registry
    }

    #[tokio::test]
    async fn builds_the_importer_registered_for_a_kind() {
        let importer = registry().create("fake", Arc::new(NoFetch));
        assert!(importer.is_ok());
        if let Ok(importer) = importer {
            let sample = ImportSource { location: "a.txt".to_owned(), bytes: FakeImporter::SAMPLE.as_bytes().to_vec() };
            importer_suite(importer.as_ref(), &sample).await;
        }
    }

    #[test]
    fn an_unknown_kind_lists_the_known_ones() {
        let error = registry().create("postman", Arc::new(NoFetch)).err();
        let expected = ImportError::UnknownKind { kind: "postman".to_owned(), known: vec!["fake".to_owned()] };
        assert_eq!(error, Some(expected));
        assert_eq!(format!("{:?}", registry()), r#"ImporterRegistry { kinds: ["fake"] }"#);
        assert_eq!(ImporterRegistry::builtin().names().collect::<Vec<_>>(), ["curl", "har", "openapi"]);
    }
}
