//! The settings an `awsSigV4` auth in the config writes as text (D440): a `secretAccessKey` or
//! `sessionToken` (V4014, masked in every excerpt) and a `region` or `service` (V2316, checked
//! as a scope name). A value with a `${…}` in it is checked when it renders.

use vc_parser::{Entry, Node};

use super::project;
use super::tls::written_text;

/// The auth type that signs each request.
const KIND: &str = "awsSigV4";

/// The credentials that sign, which are secrets.
const SECRETS: [&str; 2] = ["secretAccessKey", "sessionToken"];

/// The parts of the credential scope.
const SCOPES: [&str; 2] = ["region", "service"];

/// Each `secretAccessKey` and `sessionToken` written as text that is not empty; an empty
/// `sessionToken` sends none.
pub fn secrets(root: &Node) -> Vec<&Entry> {
    written(root, SECRETS).filter(|(_, text)| !text.is_empty()).map(|(entry, _)| entry).collect()
}

/// Each `region` and `service` written as text, with that text.
pub fn scopes(root: &Node) -> Vec<(&Entry, &str)> {
    written(root, SCOPES).collect()
}

/// Each of `keys` written as text in the config `root`'s `awsSigV4` auths.
fn written<'n>(root: &'n Node, keys: [&'static str; 2]) -> impl Iterator<Item = (&'n Entry, &'n str)> {
    let auths = project::list(root, "auth").iter().filter(|auth| project::text(auth, "type") == Some(KIND));
    let entries = auths.flat_map(move |auth| keys.map(|key| auth.entry(key))).flatten();
    entries.filter_map(|entry| Some((entry, written_text(entry)?)))
}

#[cfg(test)]
mod tests {
    use vc_parser::{Entry, Node};

    use super::{scopes, secrets};

    fn parsed(yaml: &str) -> Node {
        vc_parser::parse(vc_model::SourceFileId::new(0), vc_parser::Syntax::Yaml, yaml).0.unwrap()
    }

    fn keys<'e>(entries: impl IntoIterator<Item = &'e Entry>) -> Vec<&'e str> {
        entries.into_iter().map(|entry| entry.key.as_str()).collect()
    }

    #[test]
    fn only_text_in_a_signing_auth_counts() {
        let root = parsed(concat!(
            "auth:\n",
            "  - {type: awsSigV4, region: Us-1, service: '${var.s}', secretAccessKey: k, sessionToken: ''}\n",
            "  - {type: awsSigV4, secretAccessKey: '$${x}', sessionToken: t, region: ~}\n",
            "  - {type: bearer, region: r, secretAccessKey: k}\n",
        ));
        assert_eq!(keys(secrets(&root)), ["secretAccessKey", "secretAccessKey", "sessionToken"]);
        let found: Vec<(&str, &str)> =
            scopes(&root).into_iter().map(|(entry, text)| (entry.key.as_str(), text)).collect();
        assert_eq!(found, [("region", "Us-1")], "a template, a null and another type are left out");
    }
}
