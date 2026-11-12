//! The `$file` bodies a plan sends, read from the page's files before the run (D272), as the
//! engine's `body_files.rs` reads them from disk. The playground keeps no cross-run cache, so no
//! cache cleanup is reached.

use bytes::Bytes;
use vc_compiler::{FilePath, Unreadable};
use vc_executor::{BodyFiles, Unread};
use vc_interfaces::SourceProvider as _;
use vc_model::{Plan, Program};

use vc_interfaces::MemorySources;

/// The bytes of each `$file` the actions of `plan` send, or why it was not read.
pub fn read(project: &MemorySources, program: &Program, plan: &Plan) -> BodyFiles {
    let found = vc_compiler::body_files(program, vc_planner::bodies(program, plan));
    BodyFiles::new(found.into_iter().map(|(slot, path)| (slot, bytes(project, path))))
}

/// One `$file`'s bytes, or why the compile left it unread or the page does not hold it.
fn bytes(project: &MemorySources, path: FilePath) -> Result<Bytes, Unread> {
    let path = path.map_err(|why| match why {
        Unreadable::Templated => Unread::Templated,
        Unreadable::Refused(why) => Unread::Refused(why),
        _ => Unread::Refused("an internal error: the compile gave an unknown reason".to_owned()),
    })?;
    project.read_bytes(&path).map(Bytes::from).map_err(|error| Unread::Failed(error.to_string()))
}

#[cfg(test)]
mod tests {
    use vc_compiler::Unreadable;
    use vc_executor::Unread;
    use vc_model::SourcePath;

    use super::bytes;
    use vc_interfaces::MemorySources;

    #[test]
    fn a_body_file_is_read_from_the_page_or_says_why_not() {
        let project = MemorySources::from_files([("body.json".to_owned(), b"{}".to_vec())]);
        let found = SourcePath::new("body.json").unwrap();
        assert_eq!(bytes(&project, Ok(found)).unwrap().as_ref(), b"{}");
        let missing = SourcePath::new("gone.json").unwrap();
        assert!(matches!(bytes(&project, Ok(missing)), Err(Unread::Failed(_))));
        assert_eq!(bytes(&project, Err(Unreadable::Templated)), Err(Unread::Templated));
        let refused = bytes(&project, Err(Unreadable::Refused("outside".to_owned())));
        assert_eq!(refused, Err(Unread::Refused("outside".to_owned())));
    }
}
