//! `application/x-www-form-urlencoded` text as the WHATWG URL standard serializes it (D273):
//! alphanumerics and `*-._` stay, a space becomes `+`, and every other byte of the UTF-8 is
//! `%XX`. Form bodies and OAuth token requests both send it.

/// `text` encoded as a form name or value.
#[must_use]
pub fn form_text(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b' ' => encoded.push('+'),
            b'*' | b'-' | b'.' | b'_' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => encoded.push(char::from(byte)),
            _ => {
                encoded.push('%');
                let digits = [byte >> 4, byte & 0xF].map(|digit| char::from_digit(u32::from(digit), 16));
                encoded.extend(digits.into_iter().flatten().map(|digit| digit.to_ascii_uppercase()));
            }
        }
    }
    encoded
}

/// `name=value` pairs, each side [`form_text`], joined with `&`.
#[must_use]
pub fn form_pairs<'p>(pairs: impl IntoIterator<Item = (&'p str, &'p str)>) -> String {
    let encoded = pairs.into_iter().map(|(name, value)| format!("{}={}", form_text(name), form_text(value)));
    encoded.collect::<Vec<_>>().join("&")
}

#[cfg(test)]
mod tests {
    use super::{form_pairs, form_text};

    #[test]
    fn form_text_keeps_the_whatwg_set_and_sends_a_space_as_plus() {
        assert_eq!(form_text("A-z0_9.*"), "A-z0_9.*");
        assert_eq!(form_text("a b+c~"), "a+b%2Bc%7E");
        assert_eq!(form_text("x&y=z/é"), "x%26y%3Dz%2F%C3%A9");
    }

    #[test]
    fn pairs_join_with_an_ampersand() {
        assert_eq!(
            form_pairs([("grant_type", "client_credentials"), ("scope", "read write")]),
            "grant_type=client_credentials&scope=read+write"
        );
        assert_eq!(form_pairs([]), "");
    }
}
