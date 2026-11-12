use std::collections::{BTreeMap, HashMap};

use indexmap::IndexMap;
use serde_json::json;
use vc_model::{Examples, TemplateValue};

use super::row_names;
use crate::fixture;
use crate::fixture::Fixture;

/// An inline example row whose fields are `fields`, each a literal.
fn row(fields: &[(&str, &str)]) -> TemplateValue {
    let fields = fields.iter().map(|(key, text)| ((*key).to_owned(), TemplateValue::String(fixture::literal(text))));
    TemplateValue::Map(fields.collect::<IndexMap<_, _>>())
}

#[test]
fn a_row_is_labeled_by_a_name_given_as_plain_text_and_no_other() {
    let mut fixture = Fixture::new();
    let (inline, templated) = (fixture.test(vec![], None), fixture.test(vec![], None));
    let referenced = TemplateValue::Map(IndexMap::from([("name".to_owned(), fixture.value("${var.region}"))]));
    let rows = vec![row(&[("name", "first term")]), row(&[("term", "shoes")]), row(&[("name", "")]), referenced];
    fixture.test_mut(inline).examples = Some(Examples::Rows(rows));
    fixture.test_mut(templated).examples = Some(Examples::Template(fixture.template("${var.region}")));
    let mut plan = fixture.plan(&[], &[(inline, 4), (templated, 3)]);
    for instance in &mut plan.files[0].instances {
        instance.example_row = Some(instance.repeat_index);
    }
    let rendered = HashMap::from([(templated, vec![json!({ "name": "Oslo" }), json!({ "name": "" }), json!({})])]);
    let expected = BTreeMap::from([((inline, 0), "first term".to_owned()), ((templated, 0), "Oslo".to_owned())]);
    assert_eq!(row_names(&fixture.program, &plan, &rendered), expected);
}
