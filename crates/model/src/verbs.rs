//! The third-person verbs test IDs start with (cli-workflow "Naming conventions"): the
//! importers name tests with them and the V4001 lint holds test IDs to them.

/// `id` with its first word as a third-person verb (`getUser` gives `getsUser`,
/// `searchUsers` gives `searchesUsers`, `copyUser` gives `copiesUser`).
#[must_use]
pub fn third_person(id: &str) -> String {
    let (verb, rest) = first_word(id);
    let verb = if ["s", "sh", "ch", "x", "z"].iter().any(|end| verb.ends_with(end)) {
        format!("{verb}es")
    } else if let Some(stem) = verb.strip_suffix('y').filter(|stem| !stem.ends_with(['a', 'e', 'i', 'o', 'u'])) {
        format!("{stem}ies")
    } else {
        format!("{verb}s")
    };
    verb + rest
}

/// Whether the camelCase `id` starts with a word that reads as a third-person verb: one ending
/// in `s` but not `ss` or `us` (`getsUser`, `isHealthy`; not `getUser`, `userTest`,
/// `processOrder`). A plural noun (`usersList`) passes: the words alone cannot tell.
#[must_use]
pub fn is_third_person(id: &str) -> bool {
    let (verb, _) = first_word(id);
    verb.len() > 1 && verb.ends_with('s') && !verb.ends_with("ss") && !verb.ends_with("us")
}

/// The first word of the camelCase `id`, up to its first capital, and the rest.
fn first_word(id: &str) -> (&str, &str) {
    let split = id.find(|c: char| c.is_ascii_uppercase()).unwrap_or(id.len());
    id.split_at_checked(split).unwrap_or((id, ""))
}

#[cfg(test)]
mod tests {
    use super::{is_third_person, third_person};

    #[test]
    fn test_ids_start_with_a_third_person_verb() {
        assert_eq!(third_person("getUser"), "getsUser");
        assert_eq!(third_person("searchUsers"), "searchesUsers");
        assert_eq!(third_person("copyUser"), "copiesUser");
        assert_eq!(third_person("deployApp"), "deploysApp");
        assert_eq!(third_person("list"), "lists");
    }

    #[test]
    fn tells_a_third_person_verb_from_an_imperative_or_a_noun() {
        let read = ["getsUser", "isHealthy", "has", "rejectsDeclinedCard", "getUser", "userTest", "processOrder", "s"];
        let third: Vec<bool> = read.iter().map(|id| is_third_person(id)).collect();
        assert_eq!(third, [true, true, true, true, false, false, false, false]);
    }
}
