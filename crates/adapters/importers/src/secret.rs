//! Secrets in what an importer reads (a recording, a collection). A body field named like a credential, and any word too random to be
//! anything else (24 or more characters with lower case, upper case and digits, at least 16 of
//! them different, and not shaped like an ID), become a `${secret.NAME}` reference, named
//! `<FILE>_<FIELD>` in upper snake case. The value is dropped: only the place it was found is
//! kept.

use serde_json::{Value, json};
use vc_interfaces::SecretFinding;

use crate::body::Scrub;
use crate::path_vars::identifier;

/// What a note says in place of a credential header.
pub const DROPPED: &str = "never written; it maps to the file's auth";

/// What one request left out: the secrets that replaced values, and notes.
#[derive(Default)]
pub struct Found {
    pub secrets: Vec<SecretFinding>,
    pub notes: Vec<String>,
}

/// Scrubs one request of an api file: `at` names the request (`POST /v1/payments`).
pub struct Secrets<'f> {
    pub file: &'f str,
    pub at: String,
    pub found: &'f mut Found,
}

impl Secrets<'_> {
    /// The reference to the secret found in `place` (`query api_key`), named after its last word.
    pub fn reference(&mut self, place: &str) -> String {
        let field = place.rsplit(' ').next().unwrap_or(place);
        let name = upper_snake(&format!("{} {field}", self.file));
        if !self.found.secrets.iter().any(|secret| secret.name == name) {
            let found_at = format!("{}: {place}", self.at);
            self.found.secrets.push(SecretFinding { name: name.clone(), found_at });
        }
        format!("${{secret.{name}}}")
    }

    /// `text` with each word too random to be anything but a secret replaced by a reference.
    pub fn text(&mut self, place: &str, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut word = String::new();
        for c in text.chars() {
            if word_char(c) {
                word.push(c);
                continue;
            }
            out.push_str(&self.word(place, &std::mem::take(&mut word)));
            out.push(c);
        }
        out + &self.word(place, &word)
    }

    /// `value` with each text in it scrubbed, named after the field that holds it.
    pub fn value(&mut self, place: &str, value: Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(place, &text)),
            Value::Array(items) => items.into_iter().map(|item| self.value(place, item)).collect(),
            Value::Object(fields) => fields
                .into_iter()
                .map(|(name, value)| {
                    let value = self.value(&format!("body field {name}"), value);
                    (name, value)
                })
                .collect(),
            value => value,
        }
    }

    fn word(&mut self, place: &str, word: &str) -> String {
        if random(word) { self.reference(place) } else { word.to_owned() }
    }
}

impl Scrub for Secrets<'_> {
    fn credential(&mut self, name: &str) -> Value {
        json!(self.reference(&format!("body field {name}")))
    }

    fn note(&mut self, message: String) {
        self.found.notes.push(message);
    }
}

/// Whether `c` can be part of a token: base64, URL-safe base64 and JWT characters.
fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_.+/=~".contains(c)
}

/// Whether `word` is too random to be anything but a secret.
pub fn random(word: &str) -> bool {
    let has = |class: fn(&char) -> bool| word.chars().any(|c| class(&c));
    let mut seen: Vec<char> = word.chars().collect();
    seen.sort_unstable();
    seen.dedup();
    word.len() >= 24
        && has(char::is_ascii_lowercase)
        && has(char::is_ascii_uppercase)
        && has(char::is_ascii_digit)
        && seen.len() >= 16
        && !identifier(word)
}

/// `text` in upper snake case: `pay api_key` and `pay apiKey` give `PAY_API_KEY`.
fn upper_snake(text: &str) -> String {
    let mut out = String::new();
    let mut lower_before = false;
    for c in text.chars() {
        if !c.is_ascii_alphanumeric() {
            lower_before = false;
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if c.is_ascii_uppercase() && lower_before {
            out.push('_');
        }
        lower_before = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(c.to_ascii_uppercase());
    }
    out.trim_end_matches('_').to_owned()
}

#[cfg(test)]
mod tests {
    use super::{random, upper_snake};

    #[test]
    fn a_word_is_random_when_long_mixed_and_varied_but_not_an_id() {
        let random_words =
            ["Xk9pQ2mZ7vR4tL8wB3nY6cH1", "key_demo_4eC39HqLyjWDarjtT1zdp7dc", "eyJhbGciOiJIUzI1NiJ9.e30.Zm9v"];
        assert!(random_words.iter().all(|word| random(word)), "{random_words:?}");
        let plain = [
            "3f2c9a1e-7b4d-4e8a-9c1f-0a2b3c4d5e6f",
            "cus_9aB3kLmN0pQrStUvWxYz12",
            "Xk9pQ2mZ7vR4tL8wB3nY6c",
            "AAAAAAAAAAaaaaaaaaaa11111111",
            "https://shop.test/v1/carts",
        ];
        assert!(plain.iter().all(|word| !random(word)), "{plain:?}");
    }

    #[test]
    fn secret_names_are_upper_snake_case() {
        assert_eq!(upper_snake("pay api_key"), "PAY_API_KEY");
        assert_eq!(upper_snake("pay apiKey"), "PAY_API_KEY");
        assert_eq!(upper_snake("shop x-auth-token"), "SHOP_X_AUTH_TOKEN");
        assert_eq!(upper_snake("shop2 card"), "SHOP2_CARD");
    }
}
