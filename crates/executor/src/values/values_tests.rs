use std::collections::BTreeMap;
use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{
    ActionId, Dataset, DatasetId, Examples, GeneratorDef, Instance, InstanceId, Needs, OwnerKey, Profile, ProfileId,
    TemplateValue,
};
use vc_template::RenderError;

use super::{Frame, Iteration, Render};
use crate::fixture;
use crate::fixture::Fixture;
use crate::{ActionCx, ActionExecutors, ActionReport, ActionRequest, Saved, SharedSlots};

/// Renders `probe` for `action` in `frame`, owned by `owner`.
fn render(
    cx: &Arc<ActionCx>,
    (action, owner): (ActionId, OwnerKey),
    frame: Frame,
    probe: &TemplateValue,
) -> Result<Value, RenderError> {
    let request = ActionRequest { action, owner, cx: Arc::clone(cx), frame };
    Render::new(&request)?.value(probe)
}

fn render_one(cx: &Arc<ActionCx>, action: ActionId, frame: Frame, probe: &TemplateValue) -> Result<Value, RenderError> {
    render(cx, (action, OwnerKey::Run), frame, probe)
}

fn run_level(cx: &Arc<ActionCx>, action: ActionId, probe: &TemplateValue) -> Result<Value, RenderError> {
    render(cx, (action, OwnerKey::Run), Frame::default(), probe)
}

fn saved(pairs: &[(&str, Value)]) -> Arc<Saved> {
    Arc::new(pairs.iter().map(|(name, value)| ((*name).to_owned(), value.clone())).collect())
}

#[test]
fn profiles_layer_over_the_config_in_order() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let (prod, dev) = (ProfileId::new(fixture.sym("prod")), ProfileId::new(fixture.sym("dev")));
    let by_profile = fixture.vars(&[("region", "us-config"), ("name", "p")]);
    fixture.program.config.vars.values = fixture.vars(&[("region", "eu")]);
    fixture.program.config.vars.by_profile.insert(prod, by_profile);
    let vars = fixture.vars(&[("region", "us")]);
    fixture.program.profiles.insert(prod, Profile { id: prod, name: None, default: false, vars });
    fixture.program.profiles.insert(dev, Profile { id: dev, name: None, default: true, vars: IndexMap::new() });
    let probe = TemplateValue::List(vec![fixture.value("${var.region}"), fixture.value("${run.profiles}")]);
    let program = Arc::new(fixture.program);
    let cx = |inputs| Arc::new(ActionCx::new(Arc::clone(&program), Arc::new(ActionExecutors::new()), inputs));
    let default = run_level(&cx(fixture::inputs(&[])), action, &probe);
    assert_eq!(default, Ok(json!(["eu", ["dev"]])));
    let chosen = run_level(&cx(fixture::inputs(&[]).with_profiles(vec![dev, prod])), action, &probe);
    assert_eq!(chosen, Ok(json!(["us", ["dev", "prod"]])), "a profile's vars win over by-profile config vars");
}

#[test]
fn run_id_is_the_history_run_number() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let probe = fixture.value("user-${run.id}");
    let inputs = fixture::inputs(&[]).with_run_id(483);
    assert!(format!("{inputs:?}").contains("run_id: 483"), "{inputs:?}");
    let cx = Arc::new(ActionCx::new(Arc::new(fixture.program), Arc::new(ActionExecutors::new()), inputs));
    assert_eq!(run_level(&cx, action, &probe), Ok(json!("user-483")));
}

#[test]
fn vars_set_from_outside_win_over_the_config_and_its_profiles() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let prod = ProfileId::new(fixture.sym("prod"));
    fixture.program.config.vars.values = fixture.vars(&[("region", "eu"), ("page", "free")]);
    let vars = fixture.vars(&[("region", "us")]);
    fixture.program.profiles.insert(prod, Profile { id: prod, name: None, default: true, vars });
    let probe = TemplateValue::List(vec![fixture.value("${var.region}"), fixture.value("${var.page}")]);
    let set = std::iter::once(("region".to_owned(), TemplateValue::String(fixture::literal("ap")))).collect();
    let inputs = fixture::inputs(&[]).with_vars(set);
    let cx = Arc::new(ActionCx::new(Arc::new(fixture.program), Arc::new(ActionExecutors::new()), inputs));
    assert_eq!(run_level(&cx, action, &probe), Ok(json!(["ap", "free"])));
}

#[test]
fn a_var_defined_from_itself_reads_the_next_level_out() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.program.config.vars.values = fixture.vars(&[("greeting", "hi")]);
    let file = fixture.vars(&[("greeting", "${var.greeting} there")]);
    fixture.program.test_files.get_mut(&fixture.file).unwrap().vars.values = file;
    fixture.action_mut(action).vars.values = fixture.vars(&[("greeting", "${var.greeting}!")]);
    let probe = fixture.value("${var.greeting}");
    let owner = OwnerKey::File(fixture.file);
    let cx = fixture.cx(ActionExecutors::new());
    assert_eq!(render(&cx, (action, owner), Frame::default(), &probe), Ok(json!("hi there!")));
}

#[test]
fn the_closest_level_wins_and_saved_values_beat_file_vars() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.program.config.vars.values = fixture.vars(&[("userId", "config"), ("name", "c")]);
    fixture.api_mut(action).vars.values = fixture.vars(&[("name", "api")]);
    fixture.group_mut().vars.values = fixture.vars(&[("name", "group"), ("email", "g@x")]);
    let file = fixture.vars(&[("userId", "file")]);
    fixture.program.test_files.get_mut(&fixture.file).unwrap().vars.values = file;
    let probe = fixture.value("${var.userId} ${var.name} ${var.email}");
    let owner = OwnerKey::File(fixture.file);
    let cx = fixture.cx(ActionExecutors::new());
    assert_eq!(render(&cx, (action, owner), Frame::default(), &probe), Ok(json!("file api g@x")));
    let frame = Frame { saved: saved(&[("userId", json!("u-1"))]), ..Frame::default() };
    assert_eq!(render(&cx, (action, owner), frame, &probe), Ok(json!("u-1 api g@x")));
}

#[test]
fn vars_that_refer_to_each_other_fail_instead_of_looping() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.action_mut(action).vars.values = fixture.vars(&[("name", "${var.token}"), ("token", "${var.name}")]);
    let probe = fixture.value("${var.name}");
    let cx = fixture.cx(ActionExecutors::new());
    let error = run_level(&cx, action, &probe).unwrap_err();
    assert!(!error.missing);
    assert!(error.message.contains("more than 16 vars"), "{}", error.message);
}

#[test]
fn a_fake_var_is_drawn_once_per_instance_and_an_inline_fake_each_time() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.action_mut(action).vars.values = fixture.vars(&[("userId", "${fake.string.uuid}")]);
    let texts = ["${var.userId}", "${var.userId}", "${fake.string.uuid}", "${fake.string.uuid}"];
    let probe = TemplateValue::List(texts.map(|text| fixture.value(text)).to_vec());
    let single = fixture.value("${var.userId}");
    let cx = fixture.cx(ActionExecutors::new());
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let render = Render::new(&request).unwrap();
    let values = render.value(&probe).unwrap();
    let [kept, again, one, two] = values.as_array().unwrap().as_slice() else { panic!("four values") };
    assert_eq!(kept, again);
    assert!(kept != one && one != two && kept.as_str().is_some_and(|id| id.len() == 36), "{values}");
    let memo = render.into_memo();
    assert_eq!(memo.get("userId"), Some(kept));
    let frame = Frame { fakes: Arc::new(memo), ..Frame::default() };
    assert_eq!(render_one(&cx, action, frame, &single), Ok(kept.clone()), "the frame keeps it");
}

/// Renders `${var.userId} ${fake.string.uuid}` with `userId` and `extra` as action vars, and
/// returns the text and the memo it leaves.
fn draws_with(extra: &[(&str, &str)]) -> (Value, Saved) {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let mut vars = vec![("userId", "${fake.string.uuid}")];
    vars.extend_from_slice(extra);
    fixture.action_mut(action).vars.values = fixture.vars(&vars);
    let probe = fixture.value("${var.userId} ${fake.string.uuid}");
    let cx = fixture.cx(ActionExecutors::new());
    let request = ActionRequest { action, owner: OwnerKey::Run, cx, frame: Frame::default() };
    let render = Render::new(&request).unwrap();
    (render.value(&probe).unwrap(), render.into_memo())
}

#[test]
fn a_fake_var_is_drawn_only_when_first_rendered() {
    let (plain, plain_memo) = draws_with(&[]);
    let (spare, spare_memo) = draws_with(&[("aaaId", "${fake.string.uuid}"), ("spareId", "${fake.string.uuid}")]);
    assert_eq!(plain, spare, "unused fake vars draw nothing");
    assert_eq!(spare_memo.keys().collect::<Vec<_>>(), ["userId"], "only the rendered var is kept");
    assert_eq!(plain_memo, spare_memo);
}

#[test]
fn a_named_generator_calls_its_built_in_and_dates_follow_the_run_start() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let id = fixture.sym("code");
    let generator = GeneratorDef { id, fake: "string.fromRegExp".to_owned(), args: vec![json!("[A-C]{3}")] };
    fixture.program.generators.insert(id, generator);
    let (named, recent) = (fixture.value("${fake.code}"), fixture.value("${fake.date.recent}"));
    let unknown = fixture.value("${fake.person.age}");
    let cx = fixture.cx_with(ActionExecutors::new(), fixture::inputs(&[]).with_started_at(86_400_000));
    let code = run_level(&cx, action, &named).unwrap();
    let letters = |code: &str| code.len() == 3 && code.chars().all(|letter| ('A'..='C').contains(&letter));
    assert!(code.as_str().is_some_and(letters), "{code}");
    let date = run_level(&cx, action, &recent).unwrap();
    assert!(date.as_str().is_some_and(|date| date.starts_with("1970-01-01T")), "{date}");
    assert!(run_level(&cx, action, &unknown).unwrap_err().message.contains("is not a generator"));
}

#[test]
fn a_missing_var_reaches_the_default_pipe_and_otherwise_fails() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let (fallback, bare) = (fixture.value("${var.email | default('none')}"), fixture.value("${var.email}"));
    let cx = fixture.cx(ActionExecutors::new());
    assert_eq!(run_level(&cx, action, &fallback), Ok(json!("none")));
    let error = run_level(&cx, action, &bare).unwrap_err();
    assert_eq!((error.missing, error.message.as_str()), (true, "`var.email` is not set"));
}

#[test]
fn env_reads_the_environment_port() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let (set, unset) = (fixture.value("Bearer ${env.TOKEN}"), fixture.value("${env.NOPE}"));
    let cx = fixture.cx_with(ActionExecutors::new(), fixture::inputs(&[("TOKEN", "t-1")]));
    assert_eq!(run_level(&cx, action, &set), Ok(json!("Bearer t-1")));
    let error = run_level(&cx, action, &unset).unwrap_err();
    assert_eq!((error.missing, error.message.as_str()), (true, "`env.NOPE` is not set"));
}

#[test]
fn cel_values_read_vars_env_and_bound_roots() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.action_mut(action).vars.values = [(fixture.var("page"), TemplateValue::Number(2.into()))].into();
    let probe = fixture.cel("var.page + 1 + size(env.TOKEN) + (has(var.email) ? 100 : 0)");
    let cx = fixture.cx_with(ActionExecutors::new(), fixture::inputs(&[("TOKEN", "abc")]));
    assert_eq!(run_level(&cx, action, &probe), Ok(json!(6)));
}

#[test]
fn an_instance_binds_its_iteration_and_example_row() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let test = fixture.test(vec![action], None);
    let rows = ["a", "b"]
        .map(|name| TemplateValue::Map([("name".to_owned(), TemplateValue::String(fixture::literal(name)))].into()));
    fixture.test_mut(test).examples = Some(Examples::Rows(rows.to_vec()));
    fixture.test_mut(test).repeat = 3;
    let probe = fixture.value("${example.name}:${iteration.index}/${iteration.count}");
    let cx = fixture.cx(ActionExecutors::new());
    let instance =
        Instance { id: InstanceId::new(0), test, example_row: Some(1), repeat_index: 1, needs: Needs::default() };
    let frame = Frame { instance: Some(Arc::new(instance)), ..Frame::default() };
    let owner = OwnerKey::Instance(InstanceId::new(0));
    assert_eq!(render(&cx, (action, owner), frame, &probe), Ok(json!("b:2/3")));
}

#[test]
fn examples_from_a_file_do_not_render_yet() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let test = fixture.test(vec![action], None);
    fixture.test_mut(test).examples = Some(Examples::File(fixture::literal("rows.csv")));
    let probe = fixture.value("x");
    let cx = fixture.cx(ActionExecutors::new());
    let instance =
        Instance { id: InstanceId::new(0), test, example_row: Some(0), repeat_index: 0, needs: Needs::default() };
    let frame = Frame { instance: Some(Arc::new(instance)), ..Frame::default() };
    let error = render(&cx, (action, OwnerKey::Instance(InstanceId::new(0))), frame, &probe).unwrap_err();
    assert!(error.message.contains("later milestone"), "{}", error.message);
}

#[test]
fn step_ids_and_shared_saves_are_read_by_name() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let shared = fixture.call("/token");
    fixture.action_mut(shared).id = Some(fixture.sym("user"));
    let probe = TemplateValue::List(vec![fixture.value("${created.body.id}"), fixture.value("${var.user.token}")]);
    let (created, unbound) = (fixture.sym("created"), fixture.value("${response.status}"));
    let cx = fixture.cx(ActionExecutors::new());
    let run = SharedSlots::new([shared]);
    let report = ActionReport { saved: Saved::clone(&saved(&[("token", json!("t"))])), ..ActionReport::passed() };
    assert!(run.fill(shared, &report));
    let steps = BTreeMap::from([(created, Arc::new(json!({ "body": { "id": 7 } })))]);
    let frame = Frame { run: Arc::new(run), steps: Arc::new(steps), ..Frame::default() };
    assert_eq!(render_one(&cx, action, frame, &probe), Ok(json!([7, "t"])));
    let error = run_level(&cx, action, &unbound).map_err(|error| error.message);
    assert_eq!(error, Err("`response` has no value here".to_owned()));
}

#[test]
fn a_cleanup_reads_its_given_vars_instead_of_its_own() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    fixture.action_mut(action).vars.values = fixture.vars(&[("userId", "${response.body.id}")]);
    let probe = fixture.value("${var.userId}");
    let cx = fixture.cx(ActionExecutors::new());
    let frame = Frame::default().cleaning(&saved(&[("userId", json!(42))]));
    assert_eq!(render_one(&cx, action, frame, &probe), Ok(json!(42)));
}

#[test]
fn datasets_are_read_by_their_dotted_ids() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    for (word, rows) in [("cities", json!([{"name": "Oslo"}])), ("users.crud", json!([1, 2]))] {
        let id = DatasetId::new(fixture.sym(word));
        let rows = rows.as_array().cloned().unwrap_or_default();
        fixture.program.datasets.insert(id, Dataset { id, file: format!("datasets/{word}.csv"), rows });
    }
    let probe = fixture.value("${dataset.cities[0].name}/${dataset.users.crud[1]}");
    assert_eq!(run_level(&fixture.cx(ActionExecutors::new()), action, &probe), Ok(json!("Oslo/2")));
}

#[test]
fn a_row_without_the_column_read_is_v5112_unless_a_default_stands_in() {
    let mut fixture = Fixture::new();
    let action = fixture.call("/users");
    let (probe, fallback, other) =
        (fixture.value("${item.name}"), fixture.value("${item.name | default('-')}"), fixture.value("${run.nope}"));
    let cx = fixture.cx(ActionExecutors::new());
    let turn = Iteration { index: 1, count: 1, item: Some(json!({"country": "NO"})) };
    let frame = || Frame { iteration: Some(Arc::new(turn.clone())), ..Frame::default() };
    let error = render_one(&cx, action, frame(), &probe).unwrap_err();
    assert_eq!((error.code, error.missing), (Some(Code::V5112), true), "{}", error.message);
    assert_eq!(render_one(&cx, action, frame(), &fallback), Ok(json!("-")));
    assert_eq!(render_one(&cx, action, frame(), &other).unwrap_err().code, None, "only rows are V5112");
}
