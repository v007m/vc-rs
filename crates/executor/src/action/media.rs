//! The bytes of a `$file` body or multipart part, which the engine read before the run, and
//! the `content-type` its extension implies (D272).

use bytes::Bytes;
use vc_model::{ActionId, Failure};

use crate::action::request::rendering;
use crate::{BodyFiles, Unread};

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

/// A read `$file`'s bytes and media type.
type Read = (Bytes, &'static str);

/// The bytes the engine read for the `$file` at `path`, at `pointer` in `action`'s body, and its
/// media type; `None` when the body does not write one there. V5111 when the path holds `${…}`
/// or was refused, or the file could not be read.
pub fn file(files: &BodyFiles, (action, pointer): (ActionId, &str), path: &str) -> Result<Option<Read>, Failure> {
    let why = match files.get(action, pointer) {
        None => return Ok(None),
        Some(Ok(bytes)) => return Ok(Some((bytes.clone(), media_type(path)))),
        Some(Err(Unread::Refused(why))) => format!("the `$file` `{path}` is refused: {why}"),
        Some(Err(Unread::Failed(why))) => format!("the `$file` `{path}` cannot be read: {why}"),
        Some(Err(Unread::Templated)) => return Err(templated(pointer)),
    };
    Err(rendering(&why))
}

/// V5111 for the `$file` at `pointer` whose written path holds `${…}`, whatever it renders to.
pub fn templated(pointer: &str) -> Failure {
    rendering(&format!(
        "the `$file` at `{pointer}` cannot hold `${{…}}` references in its path; write it as plain text"
    ))
}

/// V5111 for a written `$file` body the engine did not read before the run: an internal error,
/// since the engine reads every `$file` the actions a run can send write.
pub fn not_read((action, pointer): (ActionId, &str), path: &str) -> Failure {
    let at = action.index();
    rendering(&format!(
        "the `$file` `{path}` at `{pointer}` in action {at} was not read before the run (an internal error)"
    ))
}

/// The media type of the file at `path`, by its extension in any case.
pub fn media_type(path: &str) -> &'static str {
    let extension = vc_model::basename(path).rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
    let known = extension.and_then(|extension| MEDIA.iter().find(|(known, _)| *known == extension));
    known.map_or(OCTETS, |(_, media)| media)
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use vc_diagnostics::Code;

    use vc_model::ActionId;

    use super::{OCTETS, file, media_type};
    use crate::{BodyFiles, Unread};

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
    fn a_file_is_its_bytes_and_type_or_says_why_it_was_not_read() {
        let at = |pointer: &str| (ActionId::new(0), pointer.to_owned());
        let files = BodyFiles::new([
            (at(""), Ok(Bytes::from_static(b"x"))),
            (at("/refused"), Err(Unread::Refused("it leads out of the project".to_owned()))),
            (at("/gone"), Err(Unread::Failed("no such file".to_owned()))),
        ]);
        let found = |pointer: &str, path: &str| file(&files, (ActionId::new(0), pointer), path);
        assert_eq!(found("", "a.csv"), Ok(Some((Bytes::from_static(b"x"), "text/csv"))));
        assert_eq!(found("/other", "a.csv"), Ok(None), "not given");
        assert_eq!(file(&files, (ActionId::new(1), ""), "a.csv"), Ok(None), "another action's");
        let failed = |pointer: &str| found(pointer, "../x.png").err().map(|failure| (failure.code, failure.message));
        let v5111 = Some(Code::V5111.to_string());
        let refused = "the `$file` `../x.png` is refused: it leads out of the project".to_owned();
        assert_eq!(failed("/refused"), Some((v5111.clone(), refused)));
        assert_eq!(failed("/gone"), Some((v5111, "the `$file` `../x.png` cannot be read: no such file".to_owned())));
    }
}
