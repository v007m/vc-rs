use indexmap::IndexMap;
use vc_model::{AgentPolicy, Examples, Namespace, Profile, ProfileId, TestId};

use super::{UnreadVar, unread_var};
use crate::RunValues;
use crate::fixture;
use crate::fixture::Fixture;

/// A test whose `examples` is `${var.region}`, with `test_vars` on the test and `config` on the config.
fn reading_region(test_vars: &[(&str, &str)], config: &[(&str, &str)]) -> (Fixture, TestId) {
    let mut fixture = Fixture::new();
    let test = fixture.test(vec![], None);
    fixture.test_mut(test).vars.values = fixture.vars(test_vars);
    fixture.program.config.vars.values = fixture.vars(config);
    fixture.test_mut(test).examples = Some(Examples::Template(fixture.template("${var.region}")));
    (fixture, test)
}

/// The var `examples` reads, the var that needs a root and the root, with `profiles` active.
fn needs(fixture: &Fixture, test: TestId, profiles: Vec<ProfileId>) -> Option<(String, String, Namespace)> {
    let values = RunValues::new(&fixture.program, fixture::inputs(&[]).with_profiles(profiles));
    let found = unread_var(&fixture.program, &values, &fixture.program.tests[&test]);
    found.map(|UnreadVar { read, var, root, .. }| (read, var, root))
}

fn profile(fixture: &mut Fixture, word: &str, vars: &[(&str, &str)]) -> ProfileId {
    let id = ProfileId::new(fixture.sym(word));
    let vars: IndexMap<_, _> = fixture.vars(vars);
    fixture
        .program
        .profiles
        .insert(id, Profile { id, name: None, default: false, vars, agents: AgentPolicy::default() });
    id
}

#[test]
fn a_config_var_reads_the_test_var_that_shadows_the_config_one() {
    let (fixture, test) = reading_region(&[("token", "${secret.S}")], &[("region", "${var.token}"), ("token", "lit")]);
    assert_eq!(needs(&fixture, test, vec![]), Some(("region".into(), "token".into(), Namespace::Secret)));
}

#[test]
fn a_test_var_that_shadows_a_secret_config_var_hides_it() {
    let (fixture, test) = reading_region(&[("token", "lit")], &[("region", "${var.token}"), ("token", "${secret.S}")]);
    assert_eq!(needs(&fixture, test, vec![]), None);
}

#[test]
fn a_var_reading_its_own_name_reads_the_level_past_its_own() {
    let (fixture, test) = reading_region(&[("region", "${var.region}-x")], &[("region", "${secret.S}")]);
    assert_eq!(needs(&fixture, test, vec![]), Some(("region".into(), "region".into(), Namespace::Secret)));
    let (fixture, test) = reading_region(&[("region", "${var.region | default('x')}")], &[]);
    assert_eq!(needs(&fixture, test, vec![]), None, "an unset var is the render's to report");
}

#[test]
fn vars_that_read_each_other_end_the_walk() {
    let (fixture, test) = reading_region(&[], &[("region", "${var.token}"), ("token", "${var.region} ${env.HOME}")]);
    assert_eq!(needs(&fixture, test, vec![]), None);
}

#[test]
fn a_profile_var_counts_only_when_the_profile_is_active() {
    let (mut fixture, test) = reading_region(&[], &[("region", "${env.REGION}")]);
    let prod = profile(&mut fixture, "prod", &[("region", "${secret.S}")]);
    let dev = profile(&mut fixture, "dev", &[]);
    assert_eq!(needs(&fixture, test, vec![prod]), Some(("region".into(), "region".into(), Namespace::Secret)));
    assert_eq!(needs(&fixture, test, vec![dev]), None);
    assert_eq!(needs(&fixture, test, vec![]), None);
}

#[test]
fn the_span_is_the_reference_in_examples_and_no_template_needs_nothing() {
    let (mut fixture, test) = reading_region(&[("region", "${response.body}")], &[]);
    let values = RunValues::new(&fixture.program, fixture::inputs(&[]));
    let found = unread_var(&fixture.program, &values, &fixture.program.tests[&test]);
    let written = fixture.template("${var.region}").single_ref().map(|reference| reference.span);
    assert_eq!(found.map(|found| (found.root, found.span)), written.map(|span| (Namespace::Response, span)));
    fixture.test_mut(test).examples = None;
    assert_eq!(needs(&fixture, test, vec![]), None);
}
