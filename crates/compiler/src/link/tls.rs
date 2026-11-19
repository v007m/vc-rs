//! `http.tls.clients` (D350): each entry gives its certificate one way (V1016), and each of its
//! `hosts` is a host pattern (V1017). The patterns follow the HTTP adapter's host rules, so a
//! pattern that compiles is one the run can match.

use std::net::Ipv6Addr;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_parser::Node;

use super::project;
use super::project::Project;

/// The help shared by every V1017.
const PATTERN_HELP: &str = "write `host`, `host:port` or `*.domain`, such as `api.partner.example:8443`";

/// Every `http.tls.clients` entry of the config.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let Some(config) = project.config() else { return diagnostics };
    let Some(tls) = config.root.get("http").and_then(|http| http.get("tls")) else { return diagnostics };
    for client in project::list(tls, "clients") {
        diagnostics.extend(material(client));
        diagnostics.extend(project::list(client, "hosts").iter().filter_map(host));
    }
    diagnostics
}

/// V1016 when `client` writes `pkcs12` beside `cert` or `key`, or none of the three.
fn material(client: &Node) -> Option<Diagnostic> {
    let id = project::text(client, "id").unwrap_or("this client");
    let pem = ["cert", "key"].into_iter().find_map(|key| client.entry(key));
    let Some(pkcs12) = client.entry("pkcs12") else {
        let at = client.entry("id").map_or(client.span, |entry| entry.key_span);
        let message = format!("`{id}` gives no certificate");
        return pem
            .is_none()
            .then(|| one_way(Diagnostic::new(Code::V1016, at, message).label("no `cert` or `pkcs12`")));
    };
    let pem = pem?;
    let message = format!("`{id}` gives its certificate two ways");
    let diagnostic = Diagnostic::new(Code::V1016, pkcs12.key_span, message).label("a PKCS#12 bundle here");
    Some(one_way(diagnostic.secondary(pem.key_span, "and PEM here")))
}

/// The help shared by every V1016.
fn one_way(diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.help("give `cert` and `key` (PEM) or `pkcs12`")
}

/// V1017 when the `hosts` item `node` is not a host pattern, with the host as a certain fix when
/// it was written as a URL.
fn host(node: &Node) -> Option<Diagnostic> {
    let written = node.as_str()?;
    let why = problem(written)?;
    let diagnostic = Diagnostic::new(Code::V1017, node.span, format!("`{written}` is not a host pattern"));
    let diagnostic = diagnostic.label(why).help(PATTERN_HELP);
    let Some(host) = host_of(written).filter(|host| host != written) else { return Some(diagnostic) };
    let text = if host.starts_with('*') { format!("'{host}'") } else { host.clone() };
    Some(diagnostic.fix(Fix::certain(format!("Write {host}"), vec![TextEdit::replace(node.span, text)])))
}

/// Why `pattern` is not `host`, `host:port` or `*.domain`; `None` when it is one.
fn problem(pattern: &str) -> Option<&'static str> {
    if pattern.contains("://") {
        return Some("a URL; write only its host");
    }
    if pattern.contains(['/', '?', '#', '@']) {
        return Some("only a host and a port go here");
    }
    let lower = pattern.to_ascii_lowercase();
    let host = match lower.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') || host.ends_with(']') => {
            if port.parse::<u16>().is_err() {
                return Some("the port is not a number up to 65535");
            }
            host
        }
        _ => lower.as_str(),
    };
    let (host, wildcard) = host.strip_prefix("*.").map_or((host, false), |domain| (domain, true));
    if host.contains('*') {
        return Some("`*` stands only for a whole leftmost label, as in `*.partner.example`");
    }
    let name = bare(host);
    let valid = if name.contains(':') {
        !wildcard && name.parse::<Ipv6Addr>().is_ok()
    } else {
        name == host.trim_end_matches('.') && name.split('.').all(is_label)
    };
    (!valid).then_some("not a host name or IP address")
}

/// The host of `url`, without its scheme, user, path, query or fragment, when that is a pattern.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    (!host.is_empty() && problem(host).is_none()).then(|| host.to_owned())
}

/// `host` without IPv6 brackets or one trailing dot.
fn bare(host: &str) -> &str {
    let host = host.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')).unwrap_or(host);
    host.strip_suffix('.').unwrap_or(host)
}

/// A DNS label: non-empty, letters, digits, `-` and `_` only.
fn is_label(label: &str) -> bool {
    !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::{host_of, problem};

    #[test]
    fn host_patterns_match_the_adapters_rules() {
        let good = ["api.partner.example", "API.Partner.Example.", "api.test:8443", "*.partner.example", "[::1]:8443"];
        for pattern in good.into_iter().chain(["::1", "10.0.0.7", "svc_1.internal", "*.test:443"]) {
            assert_eq!(problem(pattern), None, "{pattern}");
        }
        let bad = [
            ("https://api.test", "a URL; write only its host"),
            ("api.test/v1", "only a host and a port go here"),
            ("ada@api.test", "only a host and a port go here"),
            ("api.test:99999", "the port is not a number up to 65535"),
            ("api.test:", "the port is not a number up to 65535"),
            ("*", "`*` stands only for a whole leftmost label, as in `*.partner.example`"),
            ("api.*.test", "`*` stands only for a whole leftmost label, as in `*.partner.example`"),
            ("*api.test", "`*` stands only for a whole leftmost label, as in `*.partner.example`"),
            ("*.::1", "not a host name or IP address"),
            ("api..test", "not a host name or IP address"),
            ("api.test..", "not a host name or IP address"),
            ("api test", "not a host name or IP address"),
            ("", "not a host name or IP address"),
        ];
        for (pattern, why) in bad {
            assert_eq!(problem(pattern), Some(why), "{pattern}");
        }
    }

    #[test]
    fn a_url_gives_its_host() {
        assert_eq!(host_of("https://api.partner.example/v1?x=1#top").as_deref(), Some("api.partner.example"));
        assert_eq!(host_of("https://ada:pw@api.test:8443/").as_deref(), Some("api.test:8443"));
        assert_eq!(host_of("api.test#frag").as_deref(), Some("api.test"));
        assert_eq!(host_of("https:///v1"), None);
        assert_eq!(host_of("https://api.*.test/"), None);
    }
}
