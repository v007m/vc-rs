use std::sync::Arc;

use vc_testkit::{FakeVcs, MemorySources};

use super::CompleteKind;
use crate::sample::{builder, engine};

#[test]
fn file_ids_come_first_and_test_ids_once_the_word_has_a_hash() {
    let engine = engine(FakeVcs::default());
    assert_eq!(engine.complete(CompleteKind::Tests, "users"), ["users.crud"]);
    assert_eq!(engine.complete(CompleteKind::Tests, ""), ["orders.flow", "users.crud"]);
    assert_eq!(engine.complete(CompleteKind::Tests, "users.crud#l"), ["users.crud#listsUsers"]);
    assert_eq!(engine.complete(CompleteKind::Apis, "u"), ["users"]);
    assert_eq!(engine.complete(CompleteKind::Apis, "users#"), ["users#list"]);
    assert_eq!(engine.complete(CompleteKind::Ids, "users"), ["users.crud", "users"]);
}

#[test]
fn profiles_and_tags_complete_the_last_entry_of_a_list() {
    let engine = engine(FakeVcs::default());
    assert_eq!(engine.complete(CompleteKind::Profiles, "st"), ["staging"]);
    assert_eq!(engine.complete(CompleteKind::Profiles, "dev,s"), ["dev,staging"]);
    assert_eq!(engine.complete(CompleteKind::Tags, ""), ["smoke", "fast"]);
    assert_eq!(engine.complete(CompleteKind::Tags, "smoke,f"), ["smoke,fast"]);
}

#[test]
fn a_project_with_errors_completes_nothing() {
    let broken = MemorySources::new(&[("vibranium.config.yaml", b"syntaxVersion: [\n")]);
    let engine = builder(FakeVcs::default()).sources(Arc::new(broken)).build().unwrap();
    assert_eq!(engine.complete(CompleteKind::Tests, "").len(), 0);
}
