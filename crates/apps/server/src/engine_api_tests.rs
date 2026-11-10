use std::sync::Arc;

use vc_engine::EngineError;

use super::{EngineApi, failed};
use crate::Code;
use crate::engine_fixture::project;

#[test]
fn an_engine_error_keeps_its_message_and_help_under_the_code_that_fits() {
    let stale = failed(&EngineError::Stale { reason: "a.test.yaml: nothing was written".into() });
    assert_eq!(stale.code, Code::Aborted);
    let refused =
        failed(&EngineError::Edit { reason: "`.env` is not a definition file".into(), help: Some("open one".into()) });
    assert_eq!(refused.code, Code::InvalidArgument);
    assert!(refused.message.ends_with("\nhelp: open one"), "{refused:?}");
    let missing = failed(&EngineError::NoFilesMatch { path: "a.test.yaml".into() });
    assert_eq!(missing.code, Code::NotFound);
}

#[test]
fn a_call_with_no_profile_takes_the_servers_and_its_sets_come_last() {
    let (_dir, api) = project();
    let engine = Arc::clone(&api.engine);
    let api = EngineApi::new(engine, vec!["bare".into()], vec!["vars.a=1".into()]);
    let (profiles, sets) = api.layers(Vec::new(), vec!["vars.a=2".into()]);
    assert_eq!((profiles, sets), (vec!["bare".to_owned()], vec!["vars.a=1".to_owned(), "vars.a=2".to_owned()]));
    let (profiles, _) = api.layers(vec!["dev".into()], Vec::new());
    assert_eq!(profiles, ["dev"]);
}
