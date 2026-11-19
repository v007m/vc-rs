//! One `http.tls.clients[].hosts` item (D350): `host`, `host:port` or `*.domain`. The compiler
//! checks each item with [`HostPattern::parse`] (V1017) and the HTTP adapter matches requests
//! with what it returns, so a pattern that compiles is one the run can match.

use std::net::Ipv6Addr;

/// A `hosts` item, parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct HostPattern {
    /// The host, lowercased, without IPv6 brackets or a trailing dot; for a wildcard, the domain
    /// after `*.`.
    pub name: String,
    /// `*.domain`: `*` covers exactly one leftmost label.
    pub wildcard: bool,
    /// `host:port`: only that port.
    pub port: Option<u16>,
}

/// Why a `hosts` item is not a [`HostPattern`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HostPatternError {
    /// It has a scheme.
    #[error("a URL; write only its host")]
    Url,
    /// It has a path, query, fragment or user.
    #[error("only a host and a port go here")]
    NotOnlyHost,
    /// Its port is not a `u16`.
    #[error("the port is not a number up to 65535")]
    Port,
    /// A `*` that is not a whole leftmost label.
    #[error("`*` stands only for a whole leftmost label, as in `*.partner.example`")]
    Star,
    /// Neither a DNS name nor an IP address.
    #[error("not a host name or IP address")]
    NotHost,
}

impl HostPattern {
    /// `pattern` parsed, case-insensitively; refuses anything that could never match a URL's
    /// host. A trailing dot is dropped, as it is from the URL's host.
    ///
    /// # Errors
    ///
    /// [`HostPatternError`] naming why `pattern` is no host pattern.
    pub fn parse(pattern: &str) -> Result<Self, HostPatternError> {
        if pattern.contains("://") {
            return Err(HostPatternError::Url);
        }
        if pattern.contains(['/', '?', '#', '@']) {
            return Err(HostPatternError::NotOnlyHost);
        }
        let lower = pattern.to_ascii_lowercase();
        let (host, port) = match lower.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') || host.ends_with(']') => {
                (host, Some(port.parse::<u16>().map_err(|_| HostPatternError::Port)?))
            }
            _ => (lower.as_str(), None),
        };
        let (host, wildcard) = host.strip_prefix("*.").map_or((host, false), |domain| (domain, true));
        if host.contains('*') {
            return Err(HostPatternError::Star);
        }
        let name = Self::bare(host);
        let valid = if name.contains(':') {
            !wildcard && name.parse::<Ipv6Addr>().is_ok()
        } else {
            name == host.trim_end_matches('.') && name.split('.').all(is_label)
        };
        if !valid {
            return Err(HostPatternError::NotHost);
        }
        Ok(Self { name: name.to_owned(), wildcard, port })
    }

    /// `host` without IPv6 brackets or one trailing dot, so `[::1]` and `::1`, or `api.test.` and
    /// `api.test`, compare equal.
    #[must_use]
    pub fn bare(host: &str) -> &str {
        let host = host.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')).unwrap_or(host);
        host.strip_suffix('.').unwrap_or(host)
    }
}

/// A DNS label: non-empty, letters, digits, `-` and `_` only.
fn is_label(label: &str) -> bool {
    !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::{HostPattern, HostPatternError};

    #[test]
    fn a_pattern_keeps_its_bare_lowercased_name_wildcard_and_port() {
        let parsed = |pattern| HostPattern::parse(pattern).map(|host| (host.name, host.wildcard, host.port));
        assert_eq!(parsed("API.Partner.Example."), Ok(("api.partner.example".to_owned(), false, None)));
        assert_eq!(parsed("*.partner.example:8443"), Ok(("partner.example".to_owned(), true, Some(8443))));
        assert_eq!(parsed("[::1]:8443"), Ok(("::1".to_owned(), false, Some(8443))));
        assert_eq!(parsed("::1"), Ok(("::1".to_owned(), false, None)));
        assert_eq!(parsed("svc_1.internal"), Ok(("svc_1.internal".to_owned(), false, None)));
        assert_eq!(HostPattern::bare("[::1]"), "::1");
    }

    #[test]
    fn each_refusal_says_why() {
        let refused = [
            ("https://api.test", HostPatternError::Url),
            ("api.test/v1", HostPatternError::NotOnlyHost),
            ("ada@api.test", HostPatternError::NotOnlyHost),
            ("api.test:99999", HostPatternError::Port),
            ("*", HostPatternError::Star),
            ("api.*.test", HostPatternError::Star),
            ("*.::1", HostPatternError::NotHost),
            ("api..test", HostPatternError::NotHost),
            ("", HostPatternError::NotHost),
        ];
        for (pattern, why) in refused {
            assert_eq!(HostPattern::parse(pattern), Err(why), "{pattern}");
        }
    }
}
