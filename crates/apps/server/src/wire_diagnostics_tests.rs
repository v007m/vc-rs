use vc_contracts::engine::v1 as wire;
use vc_engine::CheckRequest;

use super::diagnostics;
use crate::engine_fixture::{TESTS, project};

const MISSPELT: &str = "tests:\n  - id: listsUsers\n    nmae: Lists every user\n    call: users#list\n";

#[tokio::test]
async fn a_fix_names_its_file_by_path_and_its_edit_by_byte_offsets() {
    let (_dir, api) = project();
    let request = CheckRequest { overlay: [(TESTS.to_owned(), MISSPELT.to_owned())].into(), ..CheckRequest::default() };
    let report = api.engine().check(&request).unwrap();
    let found = diagnostics(&report);
    let fixed = found.iter().find(|found| !found.fixes.is_empty()).unwrap();
    assert_eq!(fixed.severity(), wire::Severity::Error);
    assert_eq!(
        fixed.primary.as_ref().and_then(|label| label.span.as_ref()).map(|span| span.file.as_str()),
        Some(TESTS)
    );
    let [edit] = fixed.fixes[0].edits.as_slice() else { panic!("{fixed:?}") };
    let start = u32::try_from(MISSPELT.find("nmae").unwrap()).unwrap();
    assert_eq!((edit.file.as_str(), edit.start, edit.end, edit.new_text.as_str()), (TESTS, start, start + 4, "name"));
    assert!(fixed.fixes[0].certain);
}
