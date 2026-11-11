use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use vc_model::{Dataset, DatasetId, Examples, Instance, InstanceId, Needs, OwnerKey, TemplateValue};

use super::example_rows;
use crate::fixture;
use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionRequest, Frame, Render, RunValues};

/// A fixture whose `cities` dataset has three rows.
fn with_cities() -> Fixture {
    let mut fixture = Fixture::new();
    let id = DatasetId::new(fixture.sym("cities"));
    let rows = ["Oslo", "Lima", "Pune"].map(|name| json!({ "name": name })).to_vec();
    fixture.program.datasets.insert(id, Dataset { id, file: "datasets/cities.csv".to_owned(), rows });
    fixture
}

#[test]
fn a_template_gives_its_rows_from_vars_and_datasets_before_the_run() {
    let mut fixture = with_cities();
    let test = fixture.test(vec![], None);
    fixture.test_mut(test).vars.values = fixture.vars(&[("region", "${dataset.cities | take(2)}")]);
    let template = fixture.template("${var.region}");
    fixture.test_mut(test).examples = Some(Examples::Template(template));
    let values = RunValues::new(&fixture.program, fixture::inputs(&[]));
    let rows = example_rows(&fixture.program, &values, &fixture.program.tests[&test]);
    assert_eq!(rows, Ok(Some(vec![json!({ "name": "Oslo" }), json!({ "name": "Lima" })])));
}

#[test]
fn inline_rows_and_no_rows_give_nothing_before_the_run() {
    let mut fixture = Fixture::new();
    let (bare, inline) = (fixture.test(vec![], None), fixture.test(vec![], None));
    fixture.test_mut(inline).examples = Some(Examples::Rows(vec![fixture.value("${fake.uuid}")]));
    let values = RunValues::new(&fixture.program, fixture::inputs(&[]));
    for test in [bare, inline] {
        assert_eq!(example_rows(&fixture.program, &values, &fixture.program.tests[&test]), Ok(None));
    }
}

#[test]
fn a_template_that_gives_anything_but_a_list_of_maps_is_an_error() {
    let mut fixture = with_cities();
    let test = fixture.test(vec![], None);
    for text in ["${dataset.cities[0]}", "${dataset.cities[0].name}"] {
        fixture.test_mut(test).examples = Some(Examples::Template(fixture.template(text)));
        let values = RunValues::new(&fixture.program, fixture::inputs(&[]));
        let error = example_rows(&fixture.program, &values, &fixture.program.tests[&test]).unwrap_err();
        assert!(error.message.contains("must give a list of maps"), "{text}: {}", error.message);
    }
}

#[test]
fn a_row_a_template_gave_binds_as_data_and_is_never_rendered_again() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let test = fixture.test(vec![action], None);
    fixture.test_mut(test).examples = Some(Examples::Template(fixture.template("${var.region}")));
    let probe: TemplateValue = fixture.value("${example.name}");
    let rows = HashMap::from([(test, vec![json!({ "name": "a" }), json!({ "name": "${env.TOKEN}" })])]);
    let inputs = fixture::inputs(&[("TOKEN", "t-1")]).with_examples(rows);
    let cx = fixture.cx_with(ActionExecutors::new(), inputs);
    for (row, expected) in [(1, json!("${env.TOKEN}")), (2, json!(null))] {
        let instance =
            Instance { id: InstanceId::new(0), test, example_row: Some(row), repeat_index: 0, needs: Needs::default() };
        let frame = Frame { instance: Some(Arc::new(instance)), ..Frame::default() };
        let owner = OwnerKey::Instance(InstanceId::new(0));
        let request = ActionRequest { action, owner, cx: Arc::clone(&cx), frame };
        let rendered = Render::new(&request).and_then(|render| render.value(&probe));
        let rendered = rendered.map_err(|error| error.message);
        let expected =
            if expected.is_null() { Err("the example row is not in the test".to_owned()) } else { Ok(expected) };
        assert_eq!(rendered, expected, "row {row}");
    }
}
