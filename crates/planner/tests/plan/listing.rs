//! `select_apis` and `select_files`, the selections behind `vc list apis` and `vc list files`.

use std::collections::BTreeSet;

use vc_model::Program;
use vc_planner::{PlanError, Selection, select_apis, select_files};

use crate::common;

fn apis(program: &Program, selection: &Selection) -> Result<Vec<String>, PlanError> {
    let apis = select_apis(program, selection)?;
    Ok(apis.iter().map(|api| common::name(program, api.sym()).to_owned()).collect())
}

fn files(program: &Program, selection: &Selection) -> Result<Vec<String>, PlanError> {
    let files = select_files(program, selection)?;
    let id = |file| program.files.get(file).and_then(|source| source.path.file_id()).unwrap_or_default();
    Ok(files.into_iter().map(id).collect())
}

fn changed(files: &[&str]) -> Selection {
    Selection {
        changed: Some(files.iter().map(|file| (*file).to_owned()).collect::<BTreeSet<_>>()),
        ..Selection::default()
    }
}

#[test]
fn apis_take_ids_files_and_flags() {
    let program = common::project();
    let mut everything = apis(&program, &Selection::default()).unwrap();
    everything.sort_unstable();
    assert_eq!(everything, ["orders#list", "orders#pay", "users#create", "users#list"]);
    assert_eq!(apis(&program, &common::ids(&["users#create"])).unwrap(), ["users#create"]);
    let mut users = apis(&program, &common::ids(&["users"])).unwrap();
    users.sort_unstable();
    assert_eq!(users, ["users#create", "users#list"]);
    let mut changed_orders = apis(&program, &changed(&["orders"])).unwrap();
    changed_orders.sort_unstable();
    assert_eq!(changed_orders, ["orders#list", "orders#pay"], "a flag filters even with no positional selector");
    let error = apis(&program, &common::ids(&["users.crud"])).unwrap_err();
    assert!(error.to_string().contains("no api or api file matches `users.crud`"), "{error}");
}

#[test]
fn an_api_file_with_no_apis_is_a_known_selector() {
    let program = common::project_with(&[("services/spare/spare.api.yaml", "name: Spare\napis: []\n")]);
    assert_eq!(apis(&program, &common::ids(&["spare"])).unwrap(), Vec::<String>::new());
}

#[test]
fn files_take_ids_paths_and_the_tags_of_their_test_file() {
    let program = common::project();
    let everything = files(&program, &Selection::default()).unwrap();
    for file in ["users", "orders", "users.crud", "orders.checkout", "legacy.old"] {
        assert!(everything.iter().any(|listed| listed == file), "{file} in {everything:?}");
    }
    let tagged = Selection { tags: vec!["checkout".into()], ..Selection::default() };
    assert_eq!(files(&program, &tagged).unwrap(), ["orders.checkout"]);
    let smoke = Selection { tags: vec!["smoke".into()], ..Selection::default() };
    assert_eq!(files(&program, &smoke).unwrap(), ["users.crud"]);
    let mut orders = files(&program, &common::ids(&["services/orders"])).unwrap();
    orders.sort_unstable();
    assert_eq!(orders, ["orders", "orders.checkout"]);
    let error = files(&program, &common::ids(&["nothing.here"])).unwrap_err();
    assert!(error.to_string().contains("no definition file matches `nothing.here`"), "{error}");
}
