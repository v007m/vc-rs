//! The naming conventions (cli-workflow "Naming conventions"): what a good name looks like, and
//! the good name closest to a bad one. The V4 lints and every command that creates or renames
//! an item use these.

/// Whether `id` is a camelCase item ID: a lower-case letter, then letters and digits.
#[must_use]
pub fn is_camel_id(id: &str) -> bool {
    id.starts_with(|c: char| c.is_ascii_lowercase()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `text` as a camelCase ID: `Lists users` and `Lists_users` give `listsUsers`, `userID` gives
/// `userId`. Empty when `text` has no letters or digits.
#[must_use]
pub fn camel_id(text: &str) -> String {
    let mut id = String::new();
    for (index, word) in words(text).iter().enumerate() {
        let lower = word.to_lowercase();
        let mut chars = lower.chars();
        if index > 0
            && let Some(first) = chars.next()
        {
            id.extend(first.to_uppercase());
        }
        id.push_str(chars.as_str());
    }
    id
}

/// Whether `tag` is lower case, with a dash between words: `smoke`, `slow-network`.
#[must_use]
pub fn is_tag(tag: &str) -> bool {
    tag.split('-').all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// `text` as a tag: `SlowNetwork` gives `slow-network`.
#[must_use]
pub fn tag(text: &str) -> String {
    words(text).iter().map(|word| word.to_lowercase()).collect::<Vec<_>>().join("-")
}

/// Whether `name` is a lower-case single word (a profile), or with dashes (a file ID word).
#[must_use]
pub fn is_lower_word(name: &str, dashes: bool) -> bool {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || (dashes && c == '-');
    name.starts_with(|c: char| c.is_ascii_lowercase()) && name.chars().all(allowed)
}

/// The words of `text`: runs of letters and digits, split again where a capital follows a
/// lower-case letter or a digit.
fn words(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut previous: Option<char> = None;
    for c in text.chars() {
        if !c.is_ascii_alphanumeric() {
            previous = None;
            continue;
        }
        let boundary = c.is_ascii_uppercase() && previous.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
        match words.last_mut() {
            Some(word) if previous.is_some() && !boundary => word.push(c),
            _ => words.push(c.to_string()),
        }
        previous = Some(c);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{camel_id, is_camel_id, is_lower_word, is_tag, tag};

    #[test]
    fn derives_camel_case_ids() {
        assert_eq!(camel_id("Lists_users"), "listsUsers");
        assert_eq!(camel_id("Lists users"), "listsUsers");
        assert_eq!(camel_id("Deactivate_User"), "deactivateUser");
        assert_eq!(camel_id("ListAll"), "listAll");
        assert_eq!(camel_id("userID"), "userId");
        assert_eq!(camel_id("Rejects a declined card"), "rejectsADeclinedCard");
        assert_eq!(camel_id("--"), "");
    }

    #[test]
    fn checks_camel_case_ids() {
        assert!(is_camel_id("listsUsers") && is_camel_id("get2"));
        assert!(!is_camel_id("ListAll") && !is_camel_id("lists_users") && !is_camel_id("") && !is_camel_id("2a"));
    }

    #[test]
    fn checks_and_derives_tags() {
        assert!(is_tag("smoke") && is_tag("slow-network") && is_tag("p1"));
        assert!(!is_tag("SlowNetwork") && !is_tag("slow--network") && !is_tag("") && !is_tag("slow_network"));
        assert_eq!(tag("SlowNetwork"), "slow-network");
        assert_eq!(tag("slow_network"), "slow-network");
    }

    #[test]
    fn checks_lower_words() {
        assert!(is_lower_word("dev", false) && is_lower_word("eu2", false) && is_lower_word("a-b", true));
        assert!(!is_lower_word("Dev", false) && !is_lower_word("a-b", false) && !is_lower_word("CRUD", true));
    }
}
