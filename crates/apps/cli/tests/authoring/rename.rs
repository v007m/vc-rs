//! `vc rename <old> <new>` renames an api, test, file, var, auth or dataset ID and every
//! reference to it, keeps every other byte, and refuses a rename the compile guard says would
//! add an error (D87, D107).

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said};

/// `users.flows`, which reuses `users.crud#listsUsers` twice.
const FLOWS: (&str, &str) = (
    "services/users/flows.test.yaml",
    "tests:\n  - id: listsTwice\n    steps:\n      - test: users.crud#listsUsers\n      - test: users.crud#listsUsers\n",
);

#[test]
fn renaming_an_api_updates_its_id_and_every_reference() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["rename", "users#create", "users#createUser"]).assert().code(0);
    assert_eq!(project.read(USERS.0), USERS.1.replace("  - id: create   #", "  - id: createUser   #"));
    assert_eq!(project.read(CRUD.0), CRUD.1.replace("call: users#create\n", "call: users#createUser\n"));
    checks_clean(&project);
}

#[test]
fn renaming_an_api_updates_same_file_short_references() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["rename", "users#remove", "users#removeUser"]).assert().code(0);
    let expected = USERS.1.replace("call: remove\n", "call: removeUser\n").replace("id: remove\n", "id: removeUser\n");
    assert_eq!(project.read(USERS.0), expected);
    assert_eq!(project.read(CRUD.0), CRUD.1, "a file with no reference is untouched");
}

#[test]
fn renaming_a_test_updates_the_tests_that_reuse_it() {
    let project = Project::new(&[CONFIG, USERS, CRUD, FLOWS]);
    project.vc(&["rename", "users.crud#listsUsers", "users.crud#listsAllUsers"]).assert().code(0);
    assert_eq!(project.read(CRUD.0), CRUD.1.replace("id: listsUsers\n", "id: listsAllUsers\n"));
    assert_eq!(project.read(FLOWS.0), FLOWS.1.replace("#listsUsers", "#listsAllUsers"));
    checks_clean(&project);
}

#[test]
fn renaming_a_file_id_moves_the_file_and_updates_references() {
    let project = Project::new(&[CONFIG, USERS, CRUD, FLOWS]);
    project.vc(&["rename", "users.crud", "users.accounts"]).assert().code(0);
    assert!(!project.exists(CRUD.0), "moved away");
    assert_eq!(project.read("services/users/accounts.test.yaml"), CRUD.1);
    assert_eq!(project.read(FLOWS.0), FLOWS.1.replace("users.crud#", "users.accounts#"));
    checks_clean(&project);
}

#[test]
fn refuses_a_rename_onto_an_existing_id_and_writes_nothing() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["rename", "users#create", "users#list"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "{}", said(&output));
    assert_eq!((project.read(USERS.0), project.read(CRUD.0)), (USERS.1.to_owned(), CRUD.1.to_owned()));
}

#[test]
fn refuses_a_new_id_that_breaks_the_convention_and_suggests_one() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["rename", "users.crud#listsUsers", "users.crud#Lists_all"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("listsAll"), "{}", said(&output));
    assert_eq!(project.read(CRUD.0), CRUD.1);
}

#[test]
fn refuses_an_unknown_id() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    let output = project.vc(&["rename", "users#lst", "users#all"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "suggests the close match: {}", said(&output));
}

#[test]
fn renaming_a_var_updates_its_keys_and_every_reference() {
    let project = Project::new(&[CONFIG, USERS, CRUD]);
    project.vc(&["rename", "var.userId", "var.accountId"]).assert().code(0);
    assert_eq!(project.read(USERS.0), USERS.1.replace("userId", "accountId"));
    assert_eq!(project.read(CRUD.0), CRUD.1, "a file with no reference is untouched");
    project.vc(&["rename", "var.baseUrl", "var.usersUrl"]).assert().code(0);
    assert_eq!(project.read(CONFIG.0), CONFIG.1.replace("baseUrl", "usersUrl"));
    assert!(project.read(USERS.0).starts_with("# the users service\nbaseUrl: ${var.usersUrl}\n"));
    checks_clean(&project);
}

#[test]
fn renaming_an_auth_updates_the_config_and_every_auth_key() {
    let config = format!("{}auth:\n  - id: partnerToken\n    type: bearer\n    token: ${{env.TOKEN}}\n", CONFIG.1);
    let users = USERS.1.replace("  - id: list\n", "  - id: list\n    auth: partnerToken\n");
    let project = Project::new(&[(CONFIG.0, &config), (USERS.0, &users), CRUD]);
    let refused = project.vc(&["rename", "auth.partnerToken", "auth.none"]).output().unwrap();
    assert!(!refused.status.success(), "{}", said(&refused));
    project.vc(&["rename", "auth.partnerToken", "auth.partnerLogin"]).assert().code(0);
    assert_eq!(project.read(CONFIG.0), config.replace("partnerToken", "partnerLogin"));
    assert_eq!(project.read(USERS.0), users.replace("partnerToken", "partnerLogin"));
    checks_clean(&project);
}

#[test]
fn renaming_a_listed_dataset_updates_its_entry_and_every_reference() {
    let config = format!("{}datasets:\n  files:\n    - id: capitals\n      file: data/capitals.csv\n", CONFIG.1);
    let by_city = "  - id: listsByCity\n    name: Lists by city\n    examples: ${dataset.capitals}\n    call: users#list\n";
    let crud = format!("{}{by_city}", CRUD.1);
    let files = [(CONFIG.0, config.as_str()), USERS, (CRUD.0, &crud), ("data/capitals.csv", "name\nParis\n")];
    let project = Project::new(&files);
    project.vc(&["rename", "dataset.capitals", "dataset.cities"]).assert().code(0);
    assert_eq!(project.read(CONFIG.0), config.replace("id: capitals", "id: cities"));
    assert_eq!(project.read(CRUD.0), crud.replace("dataset.capitals", "dataset.cities"));
    let output = project.vc(&["rename", "dataset.towns", "dataset.villages"]).output().unwrap();
    assert!(said(&output).contains("datasets.files"), "only a listed dataset is renamed: {}", said(&output));
}
