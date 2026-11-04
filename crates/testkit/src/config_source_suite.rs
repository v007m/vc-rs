use vc_interfaces::ConfigSource;

/// The contract every [`ConfigSource`] meets (D112): reading the layer twice gives the same
/// entries, and every entry names its key and its origin.
///
/// # Panics
///
/// When `source` breaks the contract.
pub fn config_source_suite(source: &dyn ConfigSource) {
    let first = source.entries();
    assert_eq!(source.entries(), first, "reading the layer again gives the same entries");
    for entry in first.unwrap_or_default() {
        assert!(!entry.key.is_empty(), "every entry has a key: {entry:?}");
        assert!(!entry.origin.is_empty(), "every entry names where it was set: {entry:?}");
    }
}
