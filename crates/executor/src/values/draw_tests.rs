use std::collections::HashMap;
use std::sync::{Arc, mpsc};

use serde_json::json;
use vc_interfaces::Random;
use vc_model::{Dataset, DatasetId, Examples};
use vc_testkit::ScriptedRandom;

use super::{draw_rows, fnv1a, row_seed};
use crate::RunValues;
use crate::fixture;
use crate::fixture::Fixture;

#[test]
fn a_test_seeds_its_rows_from_the_run_seed_and_its_id_alone() {
    assert_eq!(row_seed(7, "users.crud#listsPages"), row_seed(7, "users.crud#listsPages"));
    assert_ne!(row_seed(7, "users.crud#listsPages"), row_seed(7, "users.crud#listsUsers"));
    assert_ne!(row_seed(7, "users.crud#listsPages"), row_seed(8, "users.crud#listsPages"));
    assert_eq!(row_seed(7, "a"), 0xaf63_dc4c_8601_ec8b, "the run seed XOR the FNV-1a of the ID");
    assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325, "the FNV-1a offset basis");
    assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c, "FNV-1a of `a`");
}

#[test]
fn only_a_templated_test_draws_its_rows_from_its_own_random_source() {
    let mut fixture = Fixture::new();
    let id = DatasetId::new(fixture.sym("cities"));
    let rows = ["Oslo", "Lima", "Pune"].map(|name| json!({ "name": name })).to_vec();
    fixture.program.datasets.insert(id, Dataset { id, file: "datasets/cities.csv".to_owned(), rows });
    let (templated, inline) = (fixture.test(vec![], None), fixture.test(vec![], None));
    fixture.test_mut(templated).examples = Some(Examples::Template(fixture.template("${dataset.cities | take(2)}")));
    fixture.test_mut(inline).examples = Some(Examples::Rows(vec![fixture.value("${fake.uuid}")]));
    let values = RunValues::new(&fixture.program, fixture::inputs(&[]));
    let (ask, asked) = mpsc::channel();
    let random_for = |name: &str| -> Arc<dyn Random> {
        ask.send(name.to_owned()).unwrap();
        Arc::new(ScriptedRandom::seeded(7))
    };
    let drawn = draw_rows(&fixture.program, &values, (&[templated, inline], &random_for));
    let expected = HashMap::from([(templated, vec![json!({ "name": "Oslo" }), json!({ "name": "Lima" })])]);
    assert_eq!(drawn.map_err(|error| error.message), Ok(expected));
    assert_eq!(
        asked.try_iter().collect::<Vec<_>>(),
        ["users#n0", "users#n1"],
        "each test's source is asked for by its full ID"
    );
}
