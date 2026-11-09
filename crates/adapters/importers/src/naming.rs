//! Names in a draft: camelCase IDs and vars, and the third-person verbs tests are named with.

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

/// `id` with its first word as a third-person verb (`getUser` gives `getsUser`,
/// `searchUsers` gives `searchesUsers`, `copyUser` gives `copiesUser`).
pub fn third_person(id: &str) -> String {
    let split = id.find(|c: char| c.is_ascii_uppercase()).unwrap_or(id.len());
    let (verb, rest) = id.split_at_checked(split).unwrap_or((id, ""));
    let verb = if ["s", "sh", "ch", "x", "z"].iter().any(|end| verb.ends_with(end)) {
        format!("{verb}es")
    } else if let Some(stem) = verb.strip_suffix('y').filter(|stem| !stem.ends_with(['a', 'e', 'i', 'o', 'u'])) {
        format!("{stem}ies")
    } else {
        format!("{verb}s")
    };
    verb + rest
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

fn lowered(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map(|first| first.to_ascii_lowercase().to_string() + chars.as_str()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{camel, from_route, singular, third_person};

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
    fn test_ids_start_with_a_third_person_verb() {
        assert_eq!(third_person("getUser"), "getsUser");
        assert_eq!(third_person("searchUsers"), "searchesUsers");
        assert_eq!(third_person("copyUser"), "copiesUser");
        assert_eq!(third_person("deployApp"), "deploysApp");
        assert_eq!(third_person("list"), "lists");
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
