//! Names in a draft: camelCase IDs and vars, the third-person verbs tests are named with, and
//! the file ID an origin gives.

pub use vc_model::third_person;

use crate::url::host;

/// `text` in camelCase: words split at anything not a letter or digit, the first lowercased,
/// the rest capitalised (`get_user`, `GetUser` and `users.get-user` give `getUser`,
/// `usersGetUser`). A name that would start with a digit gets `n` in front.
pub fn camel(text: &str) -> String {
    let mut words = text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|word| !word.is_empty());
    let first = words.next().map(lowered).unwrap_or_default();
    let name = words.fold(first, |name, word| name + &capitalised(word));
    if name.starts_with(|c: char| c.is_ascii_digit()) { format!("n{name}") } else { name }
}

/// The ID of an operation without an `operationId`: its method and path words
/// (`POST /users/{id}/deactivate` gives `postUsersIdDeactivate`).
pub fn from_route(method: &str, path: &str) -> String {
    camel(&format!("{} {path}", method.to_ascii_lowercase()))
}

/// The singular of an English plural, as far as a path needs it (`orders` gives `order`,
/// `categories` gives `category`, `addresses` gives `address`); `status`, `class` and other
/// words stay as they are.
pub fn singular(word: &str) -> String {
    if let Some(stem) = word.strip_suffix("ies") {
        return format!("{stem}y");
    }
    let es = ["sses", "xes", "ches", "shes", "zes"].iter().any(|end| word.ends_with(end));
    let plain = word.ends_with('s') && !["ss", "us", "is"].iter().any(|end| word.ends_with(end));
    let cut = if es { 2 } else { usize::from(plain) };
    word.get(..word.len() - cut).unwrap_or(word).to_owned()
}

/// `word` with its first letter in capitals.
pub fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default()
}

/// The file ID for `origin`: its host's first label other than `www` and `api` that starts with
/// a letter, else `fallback`, else `recorded`.
pub fn file_id(origin: &str, fallback: &str) -> String {
    let host = host(origin).unwrap_or_default();
    let label = host
        .split('.')
        .filter(|label| !matches!(*label, "www" | "api"))
        .find(|label| label.starts_with(|c: char| c.is_ascii_alphabetic()));
    let word = label.map(|label| label.chars().filter(char::is_ascii_alphanumeric).collect::<String>());
    let fallback = if fallback.is_empty() { "recorded" } else { fallback };
    word.unwrap_or_else(|| fallback.to_owned())
}

/// `base`, or `base2`, `base3`… when `taken`; `count` bounds how many are taken.
pub fn free(base: &str, count: usize, taken: impl Fn(&str) -> bool) -> String {
    let mut names = (1..=count + 1).map(|n| if n == 1 { base.to_owned() } else { format!("{base}{n}") });
    names.find(|name| !taken(name)).unwrap_or_else(|| base.to_owned())
}

/// The file and test ID of a flow named `id`: `<fileId>#<testId>`, or a bare test ID in
/// `<fallback>.recorded`.
pub fn flow_id(id: &str, fallback: &str) -> (String, String) {
    id.split_once('#').map_or_else(
        || (format!("{fallback}.recorded"), id.to_owned()),
        |(file, test)| (file.to_owned(), test.to_owned()),
    )
}

fn lowered(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{camel, from_route, singular};

    #[test]
    fn ids_are_camel_case_whatever_the_spec_wrote() {
        assert_eq!(camel("get_user"), "getUser");
        assert_eq!(camel("GetUser"), "getUser");
        assert_eq!(camel("users.get-user"), "usersGetUser");
        assert_eq!(camel("listUsers"), "listUsers");
        assert_eq!(camel("2fa"), "n2fa");
        assert_eq!(camel(""), "");
    }

    #[test]
    fn routes_name_operations_without_an_id() {
        assert_eq!(from_route("POST", "/users/{id}/deactivate"), "postUsersIdDeactivate");
        assert_eq!(from_route("GET", "/"), "get");
        assert_eq!(from_route("GET", "/userGroups"), "getUserGroups");
    }

    #[test]
    fn plurals_in_a_path_read_as_singular() {
        let read: Vec<_> = ["orders", "categories", "addresses", "boxes", "batches", "status", "class", "me"]
            .iter()
            .map(|word| singular(word))
            .collect();
        assert_eq!(read, ["order", "category", "address", "box", "batch", "status", "class", "me"]);
    }
}
