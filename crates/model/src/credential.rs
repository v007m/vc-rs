//! The names a credential goes by: an importer never writes one's value into a file, and a
//! `source.headers` entry so named takes a template, not a literal (D314). Every header the
//! redactor always masks is one.

/// The words in a header, query or body field name that mark its value as a credential.
const MARKS: [&str; 12] = [
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "accesskey",
    "privatekey",
    "signature",
    "credential",
    "authoriz",
    "session",
    "cookie",
];

/// The names that are a credential only when they are the whole name.
const WHOLE: [&str; 4] = ["auth", "key", "sig", "pwd"];

/// Whether a field called `name` holds a credential: `authorization`, `x-api-key`,
/// `access_token`, `client_secret`, `Cookie`, ….
pub fn credential(name: &str) -> bool {
    let name = name.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
    MARKS.iter().any(|mark| name.contains(mark)) || WHOLE.contains(&name.as_str())
}

#[cfg(test)]
mod tests {
    use super::credential;

    #[test]
    fn credential_names_are_found_however_they_are_written() {
        let named =
            ["Authorization", "x-api-key", "X-Auth-Token", "access_token", "client_secret", "Cookie", "key", "auth"];
        assert!(named.iter().all(|name| credential(name)), "{named:?}");
        let plain = ["accept", "content-type", "author", "sortKey", "q", "x-request-id"];
        assert!(plain.iter().all(|name| !credential(name)), "{plain:?}");
    }

    #[test]
    fn every_header_the_redactor_always_masks_is_a_credential() {
        let masked = ["authorization", "Proxy-Authorization", "cookie", "Set-Cookie"];
        assert!(masked.iter().all(|name| credential(name)), "{masked:?}");
    }
}
