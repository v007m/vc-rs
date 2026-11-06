use serde_json::{Map, Value};
use vc_interfaces::{CachedCleanup, CachedValues};
use zeroize::Zeroizing;

/// The sealed columns of a cache row. Each value is bound to its row's key and its column, so
/// a sealed value cannot be moved to another row or column and still open.
#[derive(Clone, Copy, Debug)]
pub enum Columns {
    Saved,
    Cleanups,
}

impl Columns {
    /// The associated data a value of this column in row `key` is sealed with.
    pub fn aad(self, key: &[u8; 32]) -> Vec<u8> {
        let column: &[u8] = match self {
            Self::Saved => b"saved",
            Self::Cleanups => b"cleanups",
        };
        [key.as_slice(), column].concat()
    }
}

/// `values` as JSON bytes, wiped when dropped.
pub fn encode_values(values: &CachedValues) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(Value::Object(values.clone()).to_string().into_bytes())
}

/// The values in `bytes`; `None` unless they are a JSON object.
pub fn decode_values(bytes: &[u8]) -> Option<CachedValues> {
    match serde_json::from_slice(bytes).ok()? {
        Value::Object(values) => Some(values),
        _ => None,
    }
}

/// `cleanups` as a JSON array of `{"api", "vars"}`, wiped when dropped.
pub fn encode_cleanups(cleanups: &[CachedCleanup]) -> Zeroizing<Vec<u8>> {
    let cleanups = cleanups.iter().map(|cleanup| {
        let mut object = Map::new();
        object.insert("api".into(), Value::String(cleanup.api.clone()));
        object.insert("vars".into(), Value::Object(cleanup.vars.clone()));
        Value::Object(object)
    });
    Zeroizing::new(Value::Array(cleanups.collect()).to_string().into_bytes())
}

/// The cleanups in `bytes`; `None` unless every one has an api and vars.
pub fn decode_cleanups(bytes: &[u8]) -> Option<Vec<CachedCleanup>> {
    let Value::Array(cleanups) = serde_json::from_slice(bytes).ok()? else { return None };
    cleanups.into_iter().map(cleanup).collect()
}

fn cleanup(value: Value) -> Option<CachedCleanup> {
    let Value::Object(mut object) = value else { return None };
    let Some(Value::String(api)) = object.remove("api") else { return None };
    let Some(Value::Object(vars)) = object.remove("vars") else { return None };
    Some(CachedCleanup { api, vars })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_interfaces::CachedCleanup;

    use super::{Columns, decode_cleanups, decode_values, encode_cleanups, encode_values};

    #[test]
    fn values_and_cleanups_read_back_as_written() {
        let values = json!({"userId": 7, "user": {"name": "Ann"}}).as_object().cloned().unwrap_or_default();
        assert_eq!(decode_values(&encode_values(&values)), Some(values.clone()));
        let cleanups = vec![CachedCleanup { api: "users#create".into(), vars: values }];
        assert_eq!(decode_cleanups(&encode_cleanups(&cleanups)), Some(cleanups));
        assert_eq!(decode_cleanups(b"[]"), Some(Vec::new()));
    }

    #[test]
    fn anything_else_does_not_decode() {
        assert_eq!(decode_values(b"[1]"), None);
        assert_eq!(decode_values(b"{"), None);
        assert_eq!(decode_cleanups(b"{}"), None);
        assert_eq!(decode_cleanups(br#"[{"api": 1, "vars": {}}]"#), None);
        assert_eq!(decode_cleanups(br#"[{"api": "a", "vars": []}]"#), None);
        assert_eq!(decode_cleanups(br"[1]"), None);
    }

    #[test]
    fn columns_bind_their_row_and_name() {
        let key = [9; 32];
        assert_ne!(Columns::Saved.aad(&key), Columns::Cleanups.aad(&key));
        assert_ne!(Columns::Saved.aad(&key), Columns::Saved.aad(&[8; 32]));
        assert!(Columns::Cleanups.aad(&key).ends_with(b"cleanups"));
    }
}
