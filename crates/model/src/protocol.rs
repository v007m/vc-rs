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
    /// `sse`: server-sent events read from one request's streamed response (D376).
    Sse,
    /// `websocket`: messages sent and received over one WebSocket connection (D376).
    Websocket,
}

impl Protocol {
    /// Every type, in the order the docs list them.
    pub const ALL: [Self; 4] = [Self::Http, Self::Graphql, Self::Sse, Self::Websocket];

    /// The `type` value it is written as: `http`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Graphql => "graphql",
            Self::Sse => "sse",
            Self::Websocket => "websocket",
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
    /// Every key its shapes declare but these, which belong to other types.
    Except(AllowedKeys),
    /// Only these; any other key of the same place is V2213.
    Only(AllowedKeys),
}

impl ProtocolKeys {
    /// Whether `key`, written at the place `at` picks from [`AllowedKeys`], applies.
    #[must_use]
    pub fn takes(&self, at: fn(&AllowedKeys) -> &'static [&'static str], key: &str) -> bool {
        match self {
            Self::Except(keys) => !at(keys).contains(&key),
            Self::Only(keys) => at(keys).contains(&key),
        }
    }
}

/// The keys a type lists, at each place they are written.
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
pub const PROTOCOL_KEYS: [(Protocol, ProtocolKeys); 4] = [
    (Protocol::Http, ProtocolKeys::Except(HTTP_NOT)),
    (Protocol::Graphql, ProtocolKeys::Only(GRAPHQL_KEYS)),
    (Protocol::Sse, ProtocolKeys::Except(SSE_NOT)),
    (Protocol::Websocket, ProtocolKeys::Except(WEBSOCKET_NOT)),
];

/// The keys an api of each type cannot do without, as dotted paths; a missing one is V1005.
pub const PROTOCOL_NEEDS: [(Protocol, &[&str]); 4] = [
    (Protocol::Http, &["path"]),
    (Protocol::Graphql, &["body.query"]),
    (Protocol::Sse, &["path"]),
    (Protocol::Websocket, &["path"]),
];

/// An `http` api takes every key but `expect.errors`, a `graphql` response's (D345), and `sse`,
/// an `sse` api's settings (D385).
const HTTP_NOT: AllowedKeys = AllowedKeys { api: &["sse"], expect: &["errors"], call: &[] };

/// An `sse` api sends its request as an `http` one does, and reads a stream instead of polling
/// it (D376).
const SSE_NOT: AllowedKeys = AllowedKeys { api: &[], expect: &["errors"], call: &["poll"] };

/// A `websocket` api opens with a `GET` and no body: its messages are what it sends (D376).
const WEBSOCKET_NOT: AllowedKeys =
    AllowedKeys { api: &["method", "body", "sse"], expect: &["errors"], call: &["body", "poll"] };

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
    expect: &["status", "headers", "body", "size", "timing", "cel", "custom", "errors", "replace"],
    call: &["vars", "auth", "headers", "query", "expect", "save", "poll", "retry", "timeout"],
};

#[cfg(test)]
mod tests {
    use super::{AllowedKeys, PROTOCOL_KEYS, PROTOCOL_NEEDS, Protocol};

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
    fn http_takes_all_but_errors_and_graphql_takes_away_method_and_a_call_body() {
        let [(_, http), (graphql, keys), ..] = PROTOCOL_KEYS;
        let (expect, call, api) =
            (|keys: &AllowedKeys| keys.expect, |keys: &AllowedKeys| keys.call, |keys: &AllowedKeys| keys.api);
        assert_eq!(
            (http.takes(api, "method"), http.takes(expect, "status"), http.takes(expect, "errors")),
            (true, true, false)
        );
        assert_eq!(graphql, Protocol::Graphql);
        assert_eq!((keys.takes(api, "method"), keys.takes(call, "body")), (false, false));
        assert!(keys.takes(api, "body") && keys.takes(expect, "status") && keys.takes(expect, "errors"));
    }

    #[test]
    fn sse_keeps_the_http_request_and_websocket_drops_method_and_body_and_neither_polls() {
        let [(_, http), _, (sse, sse_keys), (websocket, ws_keys)] = PROTOCOL_KEYS;
        assert_eq!((sse, websocket), (Protocol::Sse, Protocol::Websocket));
        let (api, expect, call) =
            (|keys: &AllowedKeys| keys.api, |keys: &AllowedKeys| keys.expect, |keys: &AllowedKeys| keys.call);
        assert_eq!(["method", "body", "sse"].map(|key| sse_keys.takes(api, key)), [true; 3]);
        assert_eq!((sse_keys.takes(call, "body"), sse_keys.takes(call, "poll")), (true, false));
        let ws_api = ["method", "body", "sse", "path", "http"].map(|key| ws_keys.takes(api, key));
        assert_eq!(ws_api, [false, false, false, true, true]);
        assert_eq!(["body", "poll", "headers"].map(|key| ws_keys.takes(call, key)), [false, false, true]);
        assert!(
            ws_keys.takes(expect, "status") && !ws_keys.takes(expect, "errors") && !sse_keys.takes(expect, "errors")
        );
        assert!(!http.takes(api, "sse"));
        let needs: Vec<_> = PROTOCOL_NEEDS[2..].iter().map(|(_, needs)| *needs).collect();
        assert_eq!(needs, [["path"], ["path"]]);
    }

    #[test]
    fn http_is_the_default_and_an_unknown_key_is_no_type() {
        assert_eq!(Protocol::default(), Protocol::Http);
        assert_eq!((Protocol::Http.key(), Protocol::Graphql.key()), ("http", "graphql"));
        assert_eq!((Protocol::Sse.key(), Protocol::Websocket.key()), ("sse", "websocket"));
        assert_eq!(Protocol::from_key("htp"), None);
        assert_eq!(Protocol::from_key("HTTP"), None);
    }
}
