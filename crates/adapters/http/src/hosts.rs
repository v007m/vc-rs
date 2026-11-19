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

/// One identity's host rules and its client.
#[derive(Debug)]
struct Entry<C> {
    rules: Vec<HostRule>,
    client: C,
}

/// The client for each identity, with its host rules, and the client that sends no certificate.
#[derive(Debug)]
pub struct ClientTable<C> {
    none: C,
    identities: Vec<Entry<C>>,
}

impl<C> ClientTable<C> {
    /// `none` sends no certificate; `client` builds the client for each identity, in order.
    pub fn new(
        identities: &[ClientIdentity],
        none: C,
        mut client: impl FnMut(&ClientIdentity) -> Result<C, HttpAdapterError>,
    ) -> Result<Self, HttpAdapterError> {
        let mut entry = |identity: &ClientIdentity| {
            let rules = identity.host_rules().iter().map(|rule| HostRule::parse(rule));
            let rules = rules.collect::<Result<Vec<_>, _>>().map_err(|reason| identity.error(reason))?;
            Ok(Entry { rules, client: client(identity)? })
        };
        Ok(Self { none, identities: identities.iter().map(&mut entry).collect::<Result<_, _>>()? })
    }

    /// The client for `choice` on `uri`, matching the URL's own host; `None` for an unknown identity.
    pub fn pick(&self, uri: &Uri, choice: CertChoice) -> Option<&C> {
        match choice {
            CertChoice::Host => Some(self.by_host(uri).unwrap_or(&self.none)),
            CertChoice::Named(id) => {
                let entry = usize::try_from(id.index()).ok().and_then(|index| self.identities.get(index));
                entry.map(|entry| &entry.client)
            }
            CertChoice::Off => Some(&self.none),
        }
    }

    /// The client of the first identity with a rule matching `uri`'s host and port.
    fn by_host(&self, uri: &Uri) -> Option<&C> {
        let host = uri.host()?.to_ascii_lowercase();
        let default = if uri.scheme_str() == Some("https") { 443 } else { 80 };
        let port = uri.port_u16().unwrap_or(default);
        let entry = self.identities.iter().find(|entry| entry.rules.iter().any(|rule| rule.matches(&host, port)));
        entry.map(|entry| &entry.client)
    }
}

#[cfg(test)]
mod tests {
    use http::Uri;
    use vc_interfaces::{CertChoice, CertId};

    use super::{ClientTable, HostRule};
    use crate::{ClientIdentity, HttpAdapterError};

    /// The table with client `i + 1` for identity `i`, and client 0 sending no certificate.
    fn table(hosts: &[&[&str]]) -> Result<ClientTable<usize>, HttpAdapterError> {
        let identity = |(index, rules): (usize, &&[&str])| {
            let rules = rules.iter().map(|rule| (*rule).to_owned()).collect();
            ClientIdentity::pem(format!("id{index}"), Vec::new(), Vec::new()).hosts(rules)
        };
        let mut next = 0;
        let client = |_: &ClientIdentity| {
            next += 1;
            Ok(next)
        };
        ClientTable::new(&hosts.iter().enumerate().map(identity).collect::<Vec<_>>(), 0, client)
    }

    fn slot(table: &ClientTable<usize>, url: &str) -> Option<usize> {
        table.pick(&Uri::try_from(url).ok()?, CertChoice::Host).copied()
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
        assert_eq!(table.pick(&uri, CertChoice::Named(CertId::new(1))).copied(), Some(2));
        assert_eq!(table.pick(&uri, CertChoice::Named(CertId::new(2))).copied(), None);
        assert_eq!(table.pick(&uri, CertChoice::Off).copied(), Some(0));
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
