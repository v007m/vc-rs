//! The bytes of a `$file` body or multipart part, which the engine read before the run, and
//! the `content-type` its extension implies (D272).

use bytes::Bytes;
use vc_model::Failure;

use crate::BodyFiles;
use crate::action::request::rendering;

/// The media type of each known extension, lower case; any other file is `application/octet-stream`.
const MEDIA: [(&str, &str); 14] = [
    ("json", "application/json"),
    ("csv", "text/csv"),
    ("txt", "text/plain"),
    ("xml", "application/xml"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("yaml", "application/yaml"),
    ("yml", "application/yaml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("pdf", "application/pdf"),
    ("zip", "application/zip"),
];

/// What a file with no known extension is sent as.
pub const OCTETS: &str = "application/octet-stream";

/// The bytes of the file at `path` and its media type; V5111 when the run was not given it (a
/// templated path) or it could not be read.
pub fn file(files: &BodyFiles, path: &str) -> Result<(Bytes, &'static str), Failure> {
    match files.get(path) {
        Some(Ok(bytes)) => Ok((bytes, media_type(path))),
        Some(Err(reason)) => Err(rendering(&format!("the `$file` body cannot be read: {reason}"))),
        None => {
            Err(rendering(&format!("the `$file` `{path}` was not read before the run; its path cannot hold `${{…}}`")))
        }
    }
}

/// The media type of the file at `path`, by its extension in any case.
pub fn media_type(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
    let known = extension.and_then(|extension| MEDIA.iter().find(|(known, _)| *known == extension));
    known.map_or(OCTETS, |(_, media)| media)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use vc_diagnostics::Code;

    use super::{OCTETS, file, media_type};
    use crate::BodyFiles;

    #[test]
    fn a_known_extension_names_its_media_type_in_any_case() {
        assert_eq!(media_type("services/users/fixtures/users.csv"), "text/csv");
        assert_eq!(media_type("logo.PNG"), "image/png");
        assert_eq!(media_type("a/photo.jpeg"), "image/jpeg");
        assert_eq!(media_type("config.yml"), "application/yaml");
    }

    #[test]
    fn any_other_file_is_octets() {
        assert_eq!(media_type("dump.bin"), OCTETS);
        assert_eq!(media_type("Makefile"), OCTETS);
        assert_eq!(media_type("v1.2/README"), OCTETS, "a dot in a folder is not an extension");
    }

    #[test]
    fn a_file_is_its_bytes_and_type_or_v5111_when_it_was_not_read() {
        let files =
            BodyFiles::new([("a.csv".to_owned(), Ok(b"x".to_vec())), ("b.png".to_owned(), Err("gone".to_owned()))]);
        assert_eq!(file(&files, "a.csv"), Ok((Bytes::from_static(b"x"), "text/csv")));
        for unread in ["b.png", "c.json"] {
            let code = file(&files, unread).err().and_then(|failure| failure.code);
            assert_eq!(code, Some(Code::V5111.to_string()), "{unread}");
        }
    }
}
