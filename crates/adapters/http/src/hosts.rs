use http::Uri;
use vc_interfaces::CertChoice;
use vc_model::HostPattern;

use crate::{ClientIdentity, HttpAdapterError};

/// One `hosts` entry: `host`, `host:port` or `*.domain`, where an IPv6 host may be bracketed.
#[derive(Debug, PartialEq, Eq)]
struct HostRule {
    /// The host, lowercased, without IPv6 brackets or a trailing dot; for a wildcard, the domain
    /// after `*.`.
    name: String,
    wildcard: bool,
    port: Option<u16>,
}

impl HostRule {
    /// `rule` as [`HostPattern::parse`] reads it, as the compiler's V1017 does, so a rule that
    /// compiles is one the run can match.
    fn parse(rule: &str) -> Result<Self, String> {
        let pattern = HostPattern::parse(rule).map_err(|why| format!("host `{rule}` is not a host pattern: {why}"))?;
        Ok(Self { name: pattern.name, wildcard: pattern.wildcard, port: pattern.port })
    }

    /// `true` when `host` (bare, see [`HostPattern::bare`]) on `port` matches: `*` covers exactly one
    /// leftmost label.
    fn matches(&self, host: &str, port: u16) -> bool {
        if self.port.is_some_and(|wanted| wanted != port) {
            return false;
        }
        if !self.wildcard {
            return host.eq_ignore_ascii_case(&self.name);
        }
        let split = host.len().checked_sub(self.name.len()).and_then(|at| host.split_at_checked(at));
        let Some((label, domain)) = split else { return false };
        let label = label.strip_suffix('.');
        domain.eq_ignore_ascii_case(&self.name) && label.is_some_and(|label| !label.is_empty() && !label.contains('.'))
    }
}

/// One identity's host rules and its client.
#[derive(Debug)]
struct Entry<C> {
    rules: Vec<HostRule>,
    client: C,
}

/// Client identities with their host rules, each parsed and checked once.
#[derive(Debug)]
pub struct HostRules {
    identities: Vec<(ClientIdentity, Vec<HostRule>)>,
}

impl HostRules {
    /// `identities` with each `hosts` rule parsed.
    ///
    /// # Errors
    ///
    /// [`HttpAdapterError::Identity`] naming the first identity with a rule that is not a host
    /// pattern.
    pub fn check(identities: Vec<ClientIdentity>) -> Result<Self, HttpAdapterError> {
        let parse = |identity: ClientIdentity| {
            let rules = identity.host_rules().iter().map(|rule| HostRule::parse(rule));
            let rules = rules.collect::<Result<Vec<_>, _>>().map_err(|reason| identity.error(reason))?;
            Ok((identity, rules))
        };
        Ok(Self { identities: identities.into_iter().map(parse).collect::<Result<_, _>>()? })
    }

    /// The identities, in order.
    #[cfg(test)]
    pub fn identities(&self) -> Vec<&ClientIdentity> {
        self.identities.iter().map(|(identity, _)| identity).collect()
    }
}

/// The client for each identity, with its host rules, and the client that sends no certificate.
#[derive(Debug)]
pub struct ClientTable<C> {
    none: C,
    identities: Vec<Entry<C>>,
}

impl<C> ClientTable<C> {
    /// `none` sends no certificate; `client` builds the client for each identity of `checked`,
    /// in order, its rules parsed already.
    ///
    /// # Errors
    ///
    /// The first error `client` gives.
    pub fn new(
        checked: HostRules,
        none: C,
        mut client: impl FnMut(&ClientIdentity) -> Result<C, HttpAdapterError>,
    ) -> Result<Self, HttpAdapterError> {
        let entry =
            |(identity, rules): (ClientIdentity, Vec<HostRule>)| Ok(Entry { rules, client: client(&identity)? });
        Ok(Self { none, identities: checked.identities.into_iter().map(entry).collect::<Result<_, _>>()? })
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
        if self.identities.is_empty() {
            return None;
        }
        let host = HostPattern::bare(uri.host()?);
        let default = if uri.scheme_str() == Some("https") { 443 } else { 80 };
        let port = uri.port_u16().unwrap_or(default);
        let entry = self.identities.iter().find(|entry| entry.rules.iter().any(|rule| rule.matches(host, port)));
        entry.map(|entry| &entry.client)
    }
}

#[cfg(test)]
mod tests {
    use http::Uri;
    use vc_interfaces::{CertChoice, CertId};

    use super::{ClientTable, HostRule, HostRules};
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
        ClientTable::new(HostRules::check(hosts.iter().enumerate().map(identity).collect())?, 0, client)
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
    fn ipv6_rules_match_with_or_without_brackets() -> Result<(), HttpAdapterError> {
        let rule = HostRule::parse("[::1]:8443");
        assert_eq!(rule, Ok(HostRule { name: "::1".into(), wildcard: false, port: Some(8443) }));
        assert_eq!(HostRule::parse("::1").map(|rule| rule.port), Ok(None));
        for rule in ["::1", "[::1]", "[::1]:8443"] {
            assert_eq!(slot(&table(&[&[rule]])?, "https://[::1]:8443/"), Some(1), "{rule}");
        }
        assert_eq!(slot(&table(&[&["[::A]"]])?, "https://[::a]/"), Some(1));
        assert_eq!(slot(&table(&[&["[::1]:8443"]])?, "https://[::1]:9443/"), Some(0));
        Ok(())
    }

    #[test]
    fn a_trailing_dot_is_dropped_on_both_sides() -> Result<(), HttpAdapterError> {
        assert_eq!(slot(&table(&[&["api.test."]])?, "https://api.test/"), Some(1));
        assert_eq!(slot(&table(&[&["api.test"]])?, "https://api.test./"), Some(1));
        assert_eq!(slot(&table(&[&["*.api.test."]])?, "https://eu.api.test./"), Some(1));
        Ok(())
    }

    #[test]
    fn an_empty_table_sends_no_certificate() -> Result<(), HttpAdapterError> {
        assert_eq!(slot(&table(&[])?, "https://api.test/"), Some(0));
        Ok(())
    }

    #[test]
    fn a_bad_rule_names_the_identity() {
        let never_match = [
            "https://api.test",
            "api.test/v1",
            "api.test/",
            "api test",
            " api.test",
            "api.test..",
            "a..test",
            ".api.test",
            "[api.test]",
            "[::1",
            "::g",
            "*.::1",
            "user@api.test",
        ];
        for rule in ["api.test:http", "*.", "a.*.test", ""].into_iter().chain(never_match) {
            let error = table(&[&["ok.test"], &[rule]]).err();
            assert!(matches!(&error, Some(HttpAdapterError::Identity { id, .. }) if id == "id1"), "{rule}: {error:?}");
        }
    }
}
