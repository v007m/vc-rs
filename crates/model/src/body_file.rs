//! `$file` request bodies and multipart parts (D272, D274): the key that names a file, and the
//! helpers the compiler and the executor share to find one.

use serde_json::Value;

/// The key of a single-key map that names a file: `{$file: fixtures/avatar.png}`.
pub const FILE_KEY: &str = "$file";

/// The media type whose map bodies are sent as parts, a `$file` part as its file's bytes (D274).
pub const MULTIPART: &str = "multipart/form-data";

/// The path of `value` when it is a map whose only key is `$file` and whose value is text.
#[must_use]
pub fn file_path(value: &Value) -> Option<&str> {
    let Value::Object(map) = value else { return None };
    map.get(FILE_KEY).filter(|_| map.len() == 1).and_then(Value::as_str)
}

/// The last part of a slash-separated `path`: `avatar.png` for `fixtures/avatar.png`.
#[must_use]
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether the media type of `content_type`, its parameters and case ignored, is
/// `multipart/form-data`.
#[must_use]
pub fn is_multipart(content_type: &str) -> bool {
    content_type.split(';').next().is_some_and(|media| media.trim().eq_ignore_ascii_case(MULTIPART))
}

/// The JSON pointer of the multipart field `name`, or of its item `index` when the field is a
/// list: `/avatar`, `/scans/0`. A whole-body `$file` sits at the empty pointer.
#[must_use]
pub fn part_pointer(name: &str, index: Option<usize>) -> String {
    let name = name.replace('~', "~0").replace('/', "~1");
    index.map_or_else(|| format!("/{name}"), |index| format!("/{name}/{index}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{basename, file_path, is_multipart, part_pointer};

    #[test]
    fn only_a_single_key_file_map_with_text_names_a_file() {
        assert_eq!(file_path(&json!({"$file": "a/b.png"})), Some("a/b.png"));
        assert_eq!(file_path(&json!({"$file": "a.png", "name": "x"})), None);
        assert_eq!(file_path(&json!({"$file": 3})), None);
        assert_eq!(file_path(&json!("a.png")), None);
    }

    #[test]
    fn a_basename_is_the_last_part_of_the_path() {
        assert_eq!(basename("services/users/avatar.png"), "avatar.png");
        assert_eq!(basename("avatar.png"), "avatar.png");
    }

    #[test]
    fn multipart_is_told_by_its_media_type_alone() {
        assert!(is_multipart("Multipart/Form-Data; boundary=x"));
        assert!(is_multipart(" multipart/form-data"));
        assert!(!is_multipart("application/json"));
        assert!(!is_multipart("multipart/mixed"));
    }

    #[test]
    fn a_part_pointer_escapes_its_name() {
        assert_eq!(part_pointer("avatar", None), "/avatar");
        assert_eq!(part_pointer("scans", Some(1)), "/scans/1");
        assert_eq!(part_pointer("a/b~c", None), "/a~1b~0c");
    }
}
