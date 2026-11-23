//! `http.tls.clients` (D350): each entry gives its certificate one way (V1016), and each of its
//! `hosts` is a host pattern (V1017). The patterns follow the HTTP adapter's host rules, so a
//! pattern that compiles is one the run can match.

mod written;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{FileKind, HostPattern, Pos, SourceFiles, SourceSpan, inline_pem};
use vc_parser::{Entry, Node, ScalarKind};

use super::project::Project;
use super::{project, sigv4};
use crate::cache::CacheEntry;

/// The help shared by every V1017.
const PATTERN_HELP: &str = "write `host`, `host:port` or `*.domain`, such as `api.partner.example:8443`";

/// The characters that make a plain YAML scalar something else when it starts with one.
const YAML_INDICATORS: &str = "-?:,[]{}#&*!|>'\"%@`";

/// The `key` and `passphrase` entries of the config `root`'s `http.tls.clients` that hold the
/// secret as written, not read from `${secret.NAME}` (D353).
pub fn literals(root: &Node) -> Vec<&Entry> {
    let entries =
        project::tls_clients(root).iter().flat_map(|client| ["key", "passphrase"].map(|key| client.entry(key)));
    entries.flatten().filter(|entry| literal(entry)).collect()
}

/// Where the config writes a client key or passphrase (D353), or an AWS secret access key or
/// session token (D440), as text. A config that does not parse has no tree to find them in, so
/// the text fallback masks the value of every key of those names in its `files` text instead.
pub fn secrets(project: &Project<'_>, (entries, files): (&[CacheEntry], &SourceFiles)) -> Vec<SourceSpan> {
    let Some(config) = project.config() else { return unparsed(entries, files) };
    let written = literals(config.root).into_iter().chain(sigv4::secrets(config.root));
    written.map(|entry| unindented(config.text, entry.value.span)).collect()
}

/// The text fallback's spans over the config, when there is one; it has no tree here.
fn unparsed(entries: &[CacheEntry], files: &SourceFiles) -> Vec<SourceSpan> {
    let config =
        entries.iter().find(|entry| entry.unit.as_deref().is_some_and(|unit| unit.kind == Some(FileKind::Config)));
    let file = config.and_then(|entry| Some((entry.file, files.get(entry.file)?)));
    file.map(|(id, file)| written::spans(id, &file.text)).unwrap_or_default()
}

/// `span` without the indentation it ends in: a block scalar runs up to the next key, and a mask
/// over that key's indent would print `•••` in front of it. A span that ends after text on its
/// last line, as every one-line value does, is kept whole.
fn unindented(text: &str, span: SourceSpan) -> SourceSpan {
    let end = span.end;
    let indent = usize::try_from(end.column.saturating_sub(1)).unwrap_or(usize::MAX);
    let Some(line_start) = end.offset.checked_sub(indent) else { return span };
    let blank = text.get(line_start..end.offset).is_some_and(|run| run.bytes().all(|byte| byte == b' '));
    if !blank {
        return span;
    }
    SourceSpan::new(span.file, span.start, Pos::new(end.line, 1, line_start))
}

/// `diagnostics` with `secrets` masked in each one that shows a place in a secret's file, so
/// no excerpt prints a client key or passphrase, whatever it reports (D353).
#[must_use]
pub fn masked(diagnostics: Diagnostics, secrets: &[SourceSpan]) -> Diagnostics {
    let mask = |mut diagnostic: Diagnostic| {
        let files: Vec<_> =
            std::iter::once(&diagnostic.primary).chain(&diagnostic.secondary).map(|at| at.span.file).collect();
        diagnostic.masked.extend(secrets.iter().filter(|secret| files.contains(&secret.file)));
        diagnostic
    };
    diagnostics.into_iter().map(mask).collect()
}

/// Whether `entry` holds text with no `${…}`: for `key`, inline PEM, which starts with
/// `-----BEGIN`, says `PRIVATE KEY` or runs over more than one line; for `passphrase`, any text.
/// A one-line key that is none of these is a path; the run's file check reports one that is not.
fn literal(entry: &Entry) -> bool {
    let Some(text) = written_text(entry).map(|text| text.replace("$${", "")) else { return false };
    if entry.key == "key" { inline_pem(&text) } else { !text.is_empty() }
}

/// The text `entry`'s value is written as: a scalar that is not null and holds no `${…}`, where
/// a `$${` is a literal `${`.
pub fn written_text(entry: &Entry) -> Option<&str> {
    let scalar = entry.value.as_scalar().filter(|scalar| scalar.kind != ScalarKind::Null)?;
    (!scalar.text.replace("$${", "").contains("${")).then_some(scalar.text.as_str())
}

/// Every `http.tls.clients` entry of the config.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let Some(config) = project.config() else { return diagnostics };
    for client in project::tls_clients(config.root) {
        diagnostics.extend(material(client));
        diagnostics.extend(project::list(client, "hosts").iter().filter_map(host));
    }
    diagnostics
}

/// V1016 when `client` writes `pkcs12` beside `cert` or `key`, or none of the three.
fn material(client: &Node) -> Option<Diagnostic> {
    let id = project::text(client, "id").map_or_else(|| "this client".to_owned(), |id| format!("`{id}`"));
    let pem = ["cert", "key"].into_iter().find_map(|key| client.entry(key));
    let Some(pkcs12) = client.entry("pkcs12") else {
        let at = client.entry("id").map_or(client.span, |entry| entry.key_span);
        let message = format!("{id} gives no certificate");
        return pem
            .is_none()
            .then(|| one_way(Diagnostic::new(Code::V1016, at, message).label("no `cert` or `pkcs12`")));
    };
    let pem = pem?;
    let message = format!("{id} gives its certificate two ways");
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
    let why = HostPattern::parse(written).err()?;
    let diagnostic = Diagnostic::new(Code::V1017, node.span, format!("`{written}` is not a host pattern"));
    let diagnostic = diagnostic.label(why.to_string()).help(PATTERN_HELP);
    let Some(host) = host_of(written).filter(|host| host != written) else { return Some(diagnostic) };
    let edit = TextEdit::replace(node.span, yaml_text(&host));
    Some(diagnostic.fix(Fix::certain(format!("Write {host}"), vec![edit])))
}

/// `host` as a YAML scalar: quoted when its first character would make it something else, as
/// `*` an alias and `[` a list. A host pattern holds no `'`, so single quotes need no escape.
fn yaml_text(host: &str) -> String {
    if host.starts_with(|first| YAML_INDICATORS.contains(first)) { format!("'{host}'") } else { host.to_owned() }
}

/// The host of `url`, without its scheme, user, path, query or fragment, when that is a pattern.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    (!host.is_empty() && HostPattern::parse(host).is_ok()).then(|| host.to_owned())
}

#[cfg(test)]
mod tests {
    use vc_model::{HostPattern, Pos, SourceFileId, SourceSpan};

    use super::{host_of, unindented, yaml_text};

    /// Why V1017 refuses `pattern`, as its label says it.
    fn problem(pattern: &str) -> Option<String> {
        HostPattern::parse(pattern).err().map(|why| why.to_string())
    }

    #[test]
    fn a_mask_stops_before_the_next_keys_indent() {
        let text = "  key: |\n    secret\n  passphrase: x\n";
        let at = |line, column, offset| Pos::new(line, column, offset);
        let span = |end| SourceSpan::new(SourceFileId::new(0), at(1, 8, 7), end);
        let block = unindented(text, span(at(3, 3, 22)));
        assert_eq!(block.end, at(3, 1, 20), "the block scalar's mask ends at the line start");
        let inline = span(at(1, 9, 8));
        assert_eq!(unindented(text, inline), inline, "a value that ends after text is kept whole");
        let short = span(at(1, 9, 3));
        assert_eq!(unindented(text, short), short, "an end column past its offset is kept whole");
    }

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
            assert_eq!(problem(pattern).as_deref(), Some(why), "{pattern}");
        }
    }

    #[test]
    fn a_url_gives_its_host() {
        assert_eq!(host_of("https://api.partner.example/v1?x=1#top").as_deref(), Some("api.partner.example"));
        assert_eq!(host_of("https://ada:pw@api.test:8443/").as_deref(), Some("api.test:8443"));
        assert_eq!(host_of("api.test#frag").as_deref(), Some("api.test"));
        assert_eq!(host_of("https:///v1"), None);
        assert_eq!(host_of("https://api.*.test/"), None);
        assert_eq!(host_of("https://[::1]:8443/").as_deref(), Some("[::1]:8443"));
    }

    #[test]
    fn a_host_that_yaml_would_misread_is_quoted() {
        assert_eq!(yaml_text("[::1]:8443"), "'[::1]:8443'");
        assert_eq!(yaml_text("*.partner.example"), "'*.partner.example'");
        assert_eq!(yaml_text("api.partner.example:8443"), "api.partner.example:8443");
        assert_eq!(yaml_text("10.0.0.7"), "10.0.0.7");
        for indicator in super::YAML_INDICATORS.chars() {
            assert_eq!(yaml_text(&format!("{indicator}x")), format!("'{indicator}x'"));
        }
    }
}
