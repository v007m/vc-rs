//! The protocol an api speaks, its `type` (D342), and the keys each type allows.

use serde::{Deserialize, Serialize};

/// What an api speaks: an api item's `type`, else its file's; `http` when neither sets one.
///
/// Each type is a strategy split into facets, one registry entry per layer, all keyed by this
/// value: the keys it allows ([`PROTOCOL_KEYS`]), its compile rules and its runtime.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Protocol {
    /// `http`: a request and its response.
    #[default]
    Http,
    /// `graphql`: a GraphQL operation sent as a JSON `POST` (D343).
    Graphql,
}

impl Protocol {
    /// Every type, in the order the docs list them.
    pub const ALL: [Self; 2] = [Self::Http, Self::Graphql];

    /// The `type` value it is written as: `http`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
        }
    }

    /// The type written as `key`, as [`Protocol::key`] gives it.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|protocol| protocol.key() == key)
    }
}

/// The keys an api of one type may set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ProtocolKeys {
    /// Every key its shapes declare, so no api of the type reports V2213.
    All,
    /// Only these; any other key of the same place is V2213.
    Only(AllowedKeys),
}

/// The keys a type that restricts them allows, at each place they are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct AllowedKeys {
    /// On the api item.
    pub api: &'static [&'static str],
    /// In an `expect` or a `poll.until`, on the api or on a call that reaches it.
    pub expect: &'static [&'static str],
    /// On a test or step that calls the api: the request and check keys.
    pub call: &'static [&'static str],
}

/// Each type with the keys it allows (see the syntax reference, "Api types"). The compiler's
/// V2213 comes from this one table.
pub const PROTOCOL_KEYS: [(Protocol, ProtocolKeys); 2] =
    [(Protocol::Http, ProtocolKeys::All), (Protocol::Graphql, ProtocolKeys::Only(GRAPHQL_KEYS))];

/// The keys an api of each type cannot do without, as dotted paths; a missing one is V1005.
pub const PROTOCOL_NEEDS: [(Protocol, &[&str]); 2] =
    [(Protocol::Http, &["path"]), (Protocol::Graphql, &["body.query"])];

/// A `graphql` api always sends `POST` with its `body` (D343): no `method`, and no `body` on a
/// call.
const GRAPHQL_KEYS: AllowedKeys = AllowedKeys {
    api: &[
        "id",
        "name",
        "path",
        "type",
        "vars",
        "query",
        "headers",
        "body",
        "auth",
        "timeout",
        "http",
        "expect",
        "save",
        "saveOn",
        "cleanup",
        "operationId",
        "beforeRequest",
        "afterResponse",
        "deprecated",
    ],
    expect: &["status", "headers", "body", "size", "timing", "cel", "custom", "replace"],
    call: &["vars", "auth", "headers", "query", "expect", "save", "poll", "retry", "timeout"],
};

#[cfg(test)]
mod tests {
    use super::{PROTOCOL_KEYS, PROTOCOL_NEEDS, Protocol, ProtocolKeys};

    #[test]
    fn every_type_reads_back_from_its_key_and_has_its_keys() {
        for protocol in Protocol::ALL {
            assert_eq!(Protocol::from_key(protocol.key()), Some(protocol));
        }
        let keyed: Vec<_> = PROTOCOL_KEYS.iter().map(|(protocol, _)| *protocol).collect();
        assert_eq!(keyed, Protocol::ALL);
        let needing: Vec<_> = PROTOCOL_NEEDS.iter().map(|(protocol, _)| *protocol).collect();
        assert_eq!(needing, Protocol::ALL);
    }

    #[test]
    fn http_allows_every_key_and_graphql_takes_away_method_and_a_call_body() {
        assert_eq!(PROTOCOL_KEYS[0], (Protocol::Http, ProtocolKeys::All));
        let (graphql, ProtocolKeys::Only(keys)) = PROTOCOL_KEYS[1] else { panic!("graphql restricts its keys") };
        assert_eq!(
            (graphql, keys.api.contains(&"method"), keys.call.contains(&"body")),
            (Protocol::Graphql, false, false)
        );
        assert!(keys.api.contains(&"body") && keys.expect.contains(&"status"));
    }

    #[test]
    fn http_is_the_default_and_an_unknown_key_is_no_type() {
        assert_eq!(Protocol::default(), Protocol::Http);
        assert_eq!((Protocol::Http.key(), Protocol::Graphql.key()), ("http", "graphql"));
        assert_eq!(Protocol::from_key("htp"), None);
        assert_eq!(Protocol::from_key("HTTP"), None);
    }
}
