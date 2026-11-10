//! A fetcher over JSON documents held in memory, for the importer's tests.

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;
use vc_interfaces::{FetchError, SpecFetcher};

/// Serves the documents it was given by location; anything else is not found.
#[derive(Default)]
pub struct MapFetcher(pub HashMap<String, Value>);

impl MapFetcher {
    pub fn with(documents: &[(&str, Value)]) -> Self {
        Self(documents.iter().map(|(location, document)| ((*location).to_owned(), document.clone())).collect())
    }
}

#[async_trait]
impl SpecFetcher for MapFetcher {
    async fn fetch(&self, location: &str) -> Result<Vec<u8>, FetchError> {
        let document = self
            .0
            .get(location)
            .ok_or_else(|| FetchError::Failed { location: location.to_owned(), reason: "not found".to_owned() })?;
        Ok(document.to_string().into_bytes())
    }

    fn decode(&self, location: &str, bytes: &[u8]) -> Result<Value, FetchError> {
        serde_json::from_slice(bytes)
            .map_err(|error| FetchError::Undecodable { location: location.to_owned(), reason: error.to_string() })
    }
}
