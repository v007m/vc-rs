use indexmap::IndexMap;
use vc_model::{
    ActionId, ActionKind, AuthDef, AuthId, AuthRef, BodySpec, CacheSpec, CelProgram, CelSource, Check, CheckKind,
    Duration, Examples, Expect, HookRef, InlineRequest, Loop, Matcher, MatcherArg, MatcherCall, Method, Namespace,
    Origin, Part, PartSpec, Poll, Profile, ProfileId, ScalarStyle, Segment, Size, Template, TemplateValue, TestId,
    VarDefs, VarName,
};

use crate::needs::Shared;
use crate::testing::{Fixture, read, span};

type Place = fn(&mut Fixture, TestId, ActionId);

fn value() -> TemplateValue {
    TemplateValue::String(read("warm"))
}

fn token(fixture: &Fixture) -> VarName {
    VarName::new(fixture.sym("token"))
}

fn vars(fixture: &Fixture) -> VarDefs {
    VarDefs { values: IndexMap::from([(token(fixture), value())]), by_profile: IndexMap::new() }
}

/// A new `call: users#list` that sends a header reading `warm`.
fn reader(fixture: &mut Fixture) -> ActionId {
    let action = fixture.call(None);
    fixture.action_mut(action).headers.insert("a".into(), read("warm"));
    action
}

fn expect(matcher: Matcher) -> Expect {
    let check = Check {
        kind: CheckKind::Equals,
        target: None,
        matcher,
        origin: Origin::Action(ActionId::new(0)),
        span: span(),
    };
    Expect { checks: vec![check], replace: false }
}

fn call_with(arg: MatcherArg) -> Matcher {
    Matcher::All(vec![MatcherCall { name: "equals".into(), arg, span: span() }])
}

fn inline(url: Template, headers: IndexMap<String, Template>) -> InlineRequest {
    InlineRequest { method: Method::Get, url, headers, body: BodySpec::None }
}

/// Whether test `a` needs the `beforeRun` action `warm` once `place` puts a read of it somewhere.
fn needs_warm(place: Place) -> bool {
    let mut fixture = Fixture::new();
    let warm = fixture.call(Some("warm"));
    let test = fixture.test("a", &[]);
    let call = ActionId::new(warm.index() + 1);
    place(&mut fixture, test, call);
    let mut shared = Shared::default();
    shared.plan(&fixture.program, &[warm]);
    shared.instance(&fixture.program, &fixture.program.tests[&test]).contains(warm)
}

const ACTION: &[(&str, Place)] = &[
    ("header", |f, _, c| _ = f.action_mut(c).headers.insert("a".into(), read("warm"))),
    ("query", |f, _, c| _ = f.action_mut(c).query.insert("a".into(), read("warm"))),
    ("var", |f, _, c| f.action_mut(c).vars = vars(f)),
    ("save", |f, _, c| {
        let name = token(f);
        f.action_mut(c).save.values.insert(name, value());
    }),
    ("json body", |f, _, c| f.action_mut(c).body = Some(BodySpec::Json(value()))),
    ("text body", |f, _, c| f.action_mut(c).body = Some(BodySpec::Text(read("warm")))),
    ("file body", |f, _, c| f.action_mut(c).body = Some(BodySpec::File(read("warm")))),
    ("base64 body", |f, _, c| f.action_mut(c).body = Some(BodySpec::Base64(read("warm")))),
    ("form body", |f, _, c| f.action_mut(c).body = Some(BodySpec::Form(IndexMap::from([("a".into(), value())])))),
    ("extend base", |f, _, c| {
        let changes = TemplateValue::Null;
        f.action_mut(c).body = Some(BodySpec::Extend { base: read("warm"), changes, remove: Vec::new() });
    }),
    ("extend changes", |f, _, c| {
        f.action_mut(c).body = Some(BodySpec::Extend { base: read("x"), changes: value(), remove: Vec::new() });
    }),
    ("multipart value", |f, _, c| {
        f.action_mut(c).body = Some(BodySpec::Multipart(IndexMap::from([("a".into(), PartSpec::Value(value()))])));
    }),
    ("multipart file", |f, _, c| {
        let part = PartSpec::File(read("warm"));
        f.action_mut(c).body = Some(BodySpec::Multipart(IndexMap::from([("a".into(), part)])));
    }),
    ("equals", |f, _, c| f.action_mut(c).expect = expect(Matcher::Equals(value()))),
    ("matcher arg", |f, _, c| f.action_mut(c).expect = expect(call_with(MatcherArg::Value(value())))),
    ("range", |f, _, c| {
        let bounds = Box::new([MatcherArg::Size(Size::default()), MatcherArg::Value(value())]);
        f.action_mut(c).expect = expect(call_with(MatcherArg::Range(bounds)));
    }),
    ("nested matcher", |f, _, c| {
        let inner = Box::new(Matcher::Equals(value()));
        f.action_mut(c).expect = expect(call_with(MatcherArg::Matcher(inner)));
    }),
    ("matcher list", |f, _, c| {
        let inner = vec![Matcher::Status(Vec::new()), Matcher::Equals(value())];
        f.action_mut(c).expect = expect(call_with(MatcherArg::Matchers(inner)));
    }),
    ("poll", |f, _, c| {
        let (every, timeout) = (Duration::default(), Duration::default());
        f.action_mut(c).poll = Some(Poll { until: expect(Matcher::Equals(value())), every, timeout });
    }),
    ("forEach", |f, _, c| f.action_mut(c).repeat = Some(Loop::ForEach { items: value(), parallel: 1 })),
    ("cache check", |f, _, c| {
        let check = reader(f);
        f.action_mut(c).cache = Some(CacheSpec { ttl: Duration::default(), check: Some(check) });
    }),
    ("auth vars", |f, _, c| {
        f.action_mut(c).auth = Some(AuthRef::Use { id: AuthId::new(f.sym("admin")), vars: vars(f) });
    }),
    ("auth settings", |f, _, c| {
        let id = AuthId::new(f.sym("admin"));
        let settings = IndexMap::from([("token".into(), value())]);
        let def = AuthDef { id, name: None, kind: "bearer".into(), settings, vars: VarDefs::default() };
        f.program.auth.insert(id, def);
        f.action_mut(c).auth = Some(AuthRef::Use { id, vars: VarDefs::default() });
    }),
    ("auth def vars", |f, _, c| {
        let id = AuthId::new(f.sym("admin"));
        let def = AuthDef { id, name: None, kind: "bearer".into(), settings: IndexMap::new(), vars: vars(f) };
        f.program.auth.insert(id, def);
        f.action_mut(c).auth = Some(AuthRef::Use { id, vars: VarDefs::default() });
    }),
];

const KINDS: &[(&str, Place)] = &[
    ("command", |f, _, c| f.action_mut(c).kind = ActionKind::Command(read("warm"))),
    ("inline url", |f, _, c| f.action_mut(c).kind = ActionKind::Inline(inline(read("warm"), IndexMap::new()))),
    ("inline header", |f, _, c| {
        f.action_mut(c).kind = ActionKind::Inline(inline(read("x"), IndexMap::from([("a".into(), read("warm"))])));
    }),
    ("inline body", |f, _, c| {
        let mut request = inline(read("x"), IndexMap::new());
        request.body = BodySpec::Json(value());
        f.action_mut(c).kind = ActionKind::Inline(request);
    }),
    ("reused call", |f, _, c| {
        let reused = f.test("b", &[]);
        let call = ActionId::new(c.index() + 1);
        f.action_mut(call).headers.insert("a".into(), read("warm"));
        f.action_mut(c).kind = ActionKind::Test(reused);
    }),
    ("reused vars", |f, _, c| {
        let reused = f.test("b", &[]);
        f.test_mut(reused).vars = vars(f);
        f.action_mut(c).kind = ActionKind::Test(reused);
    }),
];

const SCOPE: &[(&str, Place)] = &[
    ("test var", |f, t, _| f.test_mut(t).vars = vars(f)),
    ("test var by profile", |f, t, _| {
        let values = IndexMap::from([(token(f), value())]);
        let profile = ProfileId::new(f.sym("dev"));
        f.test_mut(t).vars.by_profile.insert(profile, values);
    }),
    ("test expect", |f, t, _| f.test_mut(t).expect = expect(Matcher::Equals(value()))),
    ("example rows", |f, t, _| f.test_mut(t).examples = Some(Examples::Rows(vec![TemplateValue::List(vec![value()])]))),
    ("example map", |f, t, _| {
        let row = TemplateValue::Map(IndexMap::from([("a".into(), value())]));
        f.test_mut(t).examples = Some(Examples::Rows(vec![row]));
    }),
    ("example file", |f, t, _| f.test_mut(t).examples = Some(Examples::File(read("warm")))),
    ("example template", |f, t, _| f.test_mut(t).examples = Some(Examples::Template(read("warm")))),
    ("before", |f, t, _| {
        let before = reader(f);
        f.test_mut(t).before.push(before);
    }),
    ("after", |f, t, _| {
        let after = reader(f);
        f.test_mut(t).after.push(after);
    }),
    ("beforeEach", |f, _, _| {
        let each = reader(f);
        f.file_mut().lifecycle.before_each.push(each);
    }),
    ("afterEach", |f, _, _| {
        let each = reader(f);
        f.file_mut().lifecycle.after_each.push(each);
    }),
    ("file var", |f, _, _| f.file_mut().vars = vars(f)),
    ("config var", |f, _, _| f.program.config.vars = vars(f)),
    ("config header", |f, _, _| _ = f.program.config.defaults.headers.insert("a".into(), read("warm"))),
    ("profile", |f, _, _| {
        let id = ProfileId::new(f.sym("dev"));
        let vars = IndexMap::from([(token(f), value())]);
        f.program.profiles.insert(id, Profile { id, name: None, default: true, vars });
    }),
    ("cel", |f, t, _| {
        let refs =
            read("warm").parts.into_iter().filter_map(|part| if let Part::Ref(r) = part { Some(r) } else { None });
        let source = CelSource { text: "x".into(), span: span(), style: ScalarStyle::Plain };
        let cel = TemplateValue::Cel(CelProgram { source, refs: refs.collect() });
        let name = token(f);
        f.test_mut(t).vars.values.insert(name, cel);
    }),
    ("extend value", |f, t, _| {
        let changes = Box::new(TemplateValue::Null);
        let extend = TemplateValue::Extend { base: read("warm"), changes, remove: Vec::new() };
        let name = token(f);
        f.test_mut(t).vars.values.insert(name, extend);
    }),
    ("extend value changes", |f, t, _| {
        let extend = TemplateValue::Extend { base: read("x"), changes: Box::new(value()), remove: Vec::new() };
        let name = token(f);
        f.test_mut(t).vars.values.insert(name, extend);
    }),
];

const API: &[(&str, Place)] = &[
    ("api path", |f, _, _| f.program.apis[0].path = read("warm")),
    ("api var", |f, _, _| f.program.apis[0].vars = vars(f)),
    ("api query", |f, _, _| _ = f.program.apis[0].query.insert("a".into(), read("warm"))),
    ("api header", |f, _, _| _ = f.program.apis[0].headers.insert("a".into(), read("warm"))),
    ("api body", |f, _, _| f.program.apis[0].body = BodySpec::Json(value())),
    ("api expect", |f, _, _| f.program.apis[0].expect = expect(Matcher::Equals(value()))),
    ("api save", |f, _, _| {
        let name = token(f);
        f.program.apis[0].save.values.insert(name, value());
    }),
    ("api auth", |f, _, _| f.program.apis[0].auth = AuthRef::Use { id: AuthId::new(f.sym("admin")), vars: vars(f) }),
    ("api cleanup", |f, _, _| f.program.apis[0].cleanup = Some(reader(f))),
    ("base url", |f, _, _| f.program.api_groups[0].base_url = Some(read("warm"))),
    ("group header", |f, _, _| _ = f.program.api_groups[0].headers.insert("a".into(), read("warm"))),
    ("group var", |f, _, _| f.program.api_groups[0].vars = vars(f)),
    ("group auth", |f, _, _| {
        f.program.api_groups[0].auth = AuthRef::Use { id: AuthId::new(f.sym("admin")), vars: vars(f) };
    }),
];

#[test]
fn a_read_anywhere_in_an_action_is_a_need() {
    assert!(!needs_warm(|_, _, _| {}), "nothing reads warm");
    for (place, put) in ACTION.iter().chain(KINDS) {
        assert!(needs_warm(*put), "a read in the {place}");
    }
}

#[test]
fn a_read_in_the_scope_or_the_called_api_is_a_need() {
    for (place, put) in SCOPE.iter().chain(API) {
        assert!(needs_warm(*put), "a read in the {place}");
    }
}

#[test]
fn a_function_reads_nothing() {
    assert!(!needs_warm(|f, _, c| {
        f.action_mut(c).kind = ActionKind::Function(HookRef { file: "a.ts".into(), export: "f".into() });
    }));
}

/// `read("warm")` with its reference changed by `change`.
fn changed(change: fn(&mut vc_model::Reference)) -> Template {
    let mut template = read("warm");
    for part in &mut template.parts {
        if let Part::Ref(reference) = part {
            change(reference);
        }
    }
    template
}

#[test]
fn only_var_reads_keyed_by_a_shared_name_are_needs() {
    let reads: [fn() -> Template; 3] = [
        || read("seed"),
        || changed(|reference| reference.ns = Namespace::Env),
        || changed(|reference| reference.path.segments[0] = Segment::Index(0)),
    ];
    for read in reads {
        let mut fixture = Fixture::new();
        let warm = fixture.call(Some("warm"));
        let test = fixture.test("a", &[]);
        fixture.action_mut(ActionId::new(1)).headers.insert("a".into(), read());
        let mut shared = Shared::default();
        shared.plan(&fixture.program, &[warm]);
        assert!(!shared.instance(&fixture.program, &fixture.program.tests[&test]).contains(warm));
    }
}
