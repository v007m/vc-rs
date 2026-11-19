use http::Uri;
use vc_interfaces::CertChoice;

use crate::{ClientIdentity, HttpAdapterError};

/// One `hosts` entry, lowercased: `host`, `host:port` or `*.domain`.
#[derive(Debug, PartialEq, Eq)]
struct HostRule {
    /// The host, or for a wildcard the domain after `*.`.
    name: String,
    wildcard: bool,
    port: Option<u16>,
}

impl HostRule {
    fn parse(rule: &str) -> Result<Self, String> {
        let rule = rule.to_ascii_lowercase();
        let (host, port) = match rule.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') || host.ends_with(']') => {
                (host, Some(port.parse().map_err(|_| format!("host `{rule}` has an invalid port"))?))
            }
            _ => (rule.as_str(), None),
        };
        let (name, wildcard) = host.strip_prefix("*.").map_or((host, false), |domain| (domain, true));
        if name.is_empty() || name.contains('*') {
            return Err(format!("host `{rule}` is not a host, host:port or *.domain"));
        }
        Ok(Self { name: name.to_owned(), wildcard, port })
    }

    /// `true` when `host` (lowercase) on `port` matches: `*` covers exactly one leftmost label.
    fn matches(&self, host: &str, port: u16) -> bool {
        if self.port.is_some_and(|wanted| wanted != port) {
            return false;
        }
        if !self.wildcard {
            return host == self.name;
        }
        let label = host.strip_suffix(self.name.as_str()).and_then(|rest| rest.strip_suffix('.'));
        label.is_some_and(|label| !label.is_empty() && !label.contains('.'))
    }
}

/// Picks the client slot for a request: slot 0 sends no certificate, slot `i + 1` is identity `i`.
#[derive(Debug)]
pub struct HostTable(Vec<Vec<HostRule>>);

impl HostTable {
    /// The rules of each identity, in order.
    pub fn new(identities: &[ClientIdentity]) -> Result<Self, HttpAdapterError> {
        let parse = |identity: &ClientIdentity| {
            let rules = identity.host_rules().iter().map(|rule| HostRule::parse(rule));
            rules.collect::<Result<Vec<_>, _>>().map_err(|reason| identity.error(reason))
        };
        Ok(Self(identities.iter().map(parse).collect::<Result<_, _>>()?))
    }

    /// The slot for `choice` on `uri`, matching the URL's own host; `None` for an unknown identity.
    pub fn slot(&self, uri: &Uri, choice: CertChoice) -> Option<usize> {
        match choice {
            CertChoice::Host => Some(self.by_host(uri).map_or(0, |index| index + 1)),
            CertChoice::Named(id) => {
                usize::try_from(id.index()).ok().filter(|&index| index < self.0.len()).map(|index| index + 1)
            }
            CertChoice::Off => Some(0),
        }
    }

    /// The first identity with a rule matching `uri`'s host and port.
    fn by_host(&self, uri: &Uri) -> Option<usize> {
        let host = uri.host()?.to_ascii_lowercase();
        let default = if uri.scheme_str() == Some("https") { 443 } else { 80 };
        let port = uri.port_u16().unwrap_or(default);
        self.0.iter().position(|rules| rules.iter().any(|rule| rule.matches(&host, port)))
    }
}

#[cfg(test)]
mod tests {
    use http::Uri;
    use vc_interfaces::{CertChoice, CertId};

    use super::{HostRule, HostTable};
    use crate::{ClientIdentity, HttpAdapterError};

    fn table(hosts: &[&[&str]]) -> Result<HostTable, HttpAdapterError> {
        let identity = |(index, rules): (usize, &&[&str])| {
            let rules = rules.iter().map(|rule| (*rule).to_owned()).collect();
            ClientIdentity::pem(format!("id{index}"), Vec::new(), Vec::new()).hosts(rules)
        };
        HostTable::new(&hosts.iter().enumerate().map(identity).collect::<Vec<_>>())
    }

    fn slot(table: &HostTable, url: &str) -> Option<usize> {
        table.slot(&Uri::try_from(url).ok()?, CertChoice::Host)
    }

    #[test]
    fn a_wildcard_covers_exactly_one_leftmost_label() -> Result<(), HttpAdapterError> {
        let table = table(&[&["*.pay.test"]])?;
        assert_eq!(slot(&table, "https://a.pay.test/x"), Some(1));
        assert_eq!(slot(&table, "https://A.Pay.Test/x"), Some(1));
        for url in ["https://pay.test/", "https://a.b.pay.test/", "https://apay.test/", "https://.pay.test/"] {
            assert_eq!(slot(&table, url), Some(0), "{url}");
        }
        Ok(())
    }

    #[test]
    fn a_port_rule_matches_only_that_port_and_the_scheme_sets_the_default() -> Result<(), HttpAdapterError> {
        let table = table(&[&["api.test:8443", "web.test:443", "plain.test:80"], &["api.test"]])?;
        assert_eq!(slot(&table, "https://api.test:8443/"), Some(1));
        assert_eq!(slot(&table, "https://api.test:9443/"), Some(2), "falls through to the next identity");
        assert_eq!(slot(&table, "https://web.test/"), Some(1));
        assert_eq!(slot(&table, "http://web.test/"), Some(0));
        assert_eq!(slot(&table, "http://plain.test/"), Some(1));
        assert_eq!(slot(&table, "https://other.test/"), Some(0));
        Ok(())
    }

    #[test]
    fn the_first_matching_identity_wins() -> Result<(), HttpAdapterError> {
        let table = table(&[&[], &["*.api.test"], &["eu.api.test"]])?;
        assert_eq!(slot(&table, "https://eu.api.test/"), Some(2));
        Ok(())
    }

    #[test]
    fn named_and_off_ignore_the_host() -> Result<(), HttpAdapterError> {
        let table = table(&[&["api.test"], &[]])?;
        let uri = Uri::from_static("https://api.test/");
        assert_eq!(table.slot(&uri, CertChoice::Named(CertId::new(1))), Some(2));
        assert_eq!(table.slot(&uri, CertChoice::Named(CertId::new(2))), None);
        assert_eq!(table.slot(&uri, CertChoice::Off), Some(0));
        Ok(())
    }

    #[test]
    fn ipv6_hosts_keep_their_brackets() {
        let rule = HostRule::parse("[::1]:8443");
        assert_eq!(rule, Ok(HostRule { name: "[::1]".into(), wildcard: false, port: Some(8443) }));
        assert_eq!(HostRule::parse("::1").map(|rule| rule.port), Ok(None));
    }

    #[test]
    fn a_bad_rule_names_the_identity() {
        for rule in ["api.test:http", "*.", "a.*.test", ""] {
            let error = table(&[&["ok.test"], &[rule]]).err();
            assert!(matches!(&error, Some(HttpAdapterError::Identity { id, .. }) if id == "id1"), "{rule}: {error:?}");
        }
    }
}
