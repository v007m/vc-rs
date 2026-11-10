//! Path segments that look like IDs, as vars named after the segment before them
//! (`/orders/ord_8Kq2Tz` gives `/orders/${var.orderId}`), shared by the curl and HAR importers.

use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::naming::{camel, singular};

/// A recorded path with its IDs as vars.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Templated {
    /// The path as template text: `/orders/${var.orderId}`.
    pub path: String,
    /// Each var with the value recorded in its place.
    pub vars: IndexMap<String, Value>,
}

/// `path` with each segment that looks like an ID written as a var.
pub fn templated(path: &str) -> Templated {
    let mut out = Templated::default();
    let mut before = "";
    for segment in path.split('/').skip(1) {
        out.path.push('/');
        if identifier(segment) {
            let var = unused(&out.vars, var_for(before));
            out.path.push_str("${var.");
            out.path.push_str(&var);
            out.path.push('}');
            out.vars.insert(var, json!(segment));
        } else {
            out.path.push_str(segment);
        }
        before = segment;
    }
    out
}

/// Whether `segment` looks like an ID: a number, a UUID, long hex, or a prefixed ID such as
/// `ord_8Kq2Tz`.
pub fn identifier(segment: &str) -> bool {
    let hex = |text: &str| text.chars().all(|c| c.is_ascii_hexdigit());
    let digit = |text: &str| text.chars().any(|c| c.is_ascii_digit());
    let number = !segment.is_empty() && segment.chars().all(|c| c.is_ascii_digit());
    let uuid =
        segment.len() == 36 && segment.split('-').map(str::len).eq([8, 4, 4, 4, 12]) && hex(&segment.replace('-', ""));
    let long_hex = segment.len() >= 16 && hex(segment) && digit(segment);
    let prefixed = segment.split_once('_').is_some_and(|(prefix, rest)| {
        (1..=5).contains(&prefix.len())
            && prefix.chars().all(|c| c.is_ascii_lowercase())
            && rest.len() >= 4
            && rest.chars().all(|c| c.is_ascii_alphanumeric())
            && digit(rest)
    });
    number || uuid || long_hex || prefixed
}

/// The var for an ID after `before`: `orders` gives `orderId`; none, or another ID, gives `id`.
fn var_for(before: &str) -> String {
    if before.is_empty() || identifier(before) {
        return "id".to_owned();
    }
    let word = camel(&singular(before));
    if word.is_empty() { "id".to_owned() } else { format!("{word}Id") }
}

/// `name`, or `name2`, `name3`… when it is taken.
fn unused(vars: &IndexMap<String, Value>, name: String) -> String {
    if !vars.contains_key(&name) {
        return name;
    }
    (2..vars.len() + 3).map(|n| format!("{name}{n}")).find(|next| !vars.contains_key(next)).unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{identifier, templated};

    #[test]
    fn ids_in_a_path_become_vars_named_after_the_segment_before_them() {
        let read = templated("/api/v1/users/u_7f3k/orders/42/items/3f2c9a1e-7b4d-4e8a-9c1f-0a2b3c4d5e6f");
        assert_eq!(read.path, "/api/v1/users/${var.userId}/orders/${var.orderId}/items/${var.itemId}");
        let values: Vec<_> = read.vars.values().cloned().collect();
        assert_eq!(values, [json!("u_7f3k"), json!("42"), json!("3f2c9a1e-7b4d-4e8a-9c1f-0a2b3c4d5e6f")]);
        assert_eq!(templated("/7/8/categories/9").path, "/${var.id}/${var.id2}/categories/${var.categoryId}");
        assert_eq!(templated("/").path, "/");
        assert_eq!(templated("/users/").path, "/users/");
    }

    #[test]
    fn only_segments_that_look_like_ids_are_ids() {
        let ids = ["42", "ord_8Kq2Tz", "u_7f3k", "0123456789abcdef0", "3F2C9A1E-7B4D-4E8A-9C1F-0A2B3C4D5E6F"];
        assert!(ids.iter().all(|id| identifier(id)), "{ids:?}");
        let words = ["", "users", "v1", "user_profile", "deadbeefdeadbeef", "abc_12", "Ord_8Kq2Tz", "2024-01-01"];
        assert!(words.iter().all(|word| !identifier(word)), "{words:?}");
    }
}
