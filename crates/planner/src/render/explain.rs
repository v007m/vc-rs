//! `vc plan --explain`: for each planned test, the merged expectations of each request its body
//! makes, with where each check came from (D123).

use std::collections::BTreeSet;
use std::fmt::Write as _;

use vc_model::{Check, CheckKind, Expect, Matcher, Origin, Path, Plan, Program, Segment, StatusPattern};

use crate::visit;

/// Each planned test once, in plan order, with its checks.
#[must_use]
pub fn explain(program: &Program, plan: &Plan) -> String {
    let mut out = String::new();
    let mut seen = BTreeSet::new();
    let tests = plan.files.iter().flat_map(|file| &file.instances).map(|instance| instance.test);
    for test in tests.filter(|test| seen.insert(*test)).filter_map(|test| program.tests.get(&test)) {
        let _ = writeln!(out, "{}", super::name(program, test.id.sym()));
        for action in visit::body_actions(&test.body) {
            let _ = writeln!(out, "  {}", super::action_label(program, action));
            let checks = usize::try_from(action.index()).ok().and_then(|at| program.actions.get(at));
            for line in checks.iter().flat_map(|action| check_lines(program, &action.expect)) {
                let _ = writeln!(out, "    {line}");
            }
        }
    }
    out
}

/// Each check of `expect` on a line, with where it came from: `status: 201  (api users#create)`.
#[must_use]
pub fn check_lines(program: &Program, expect: &Expect) -> Vec<String> {
    expect.checks.iter().map(|check| format!("{}  ({})", check_text(check), origin(program, check.origin))).collect()
}

/// `header content-type: contains` or `status: 200, 2xx`.
fn check_text(check: &Check) -> String {
    let target = check.target.as_ref().map(path).filter(|text| !text.is_empty());
    let kind = target.map_or_else(|| kind(check.kind).to_owned(), |target| format!("{} {target}", kind(check.kind)));
    format!("{kind}: {}", matcher(&check.matcher))
}

const fn kind(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Status => "status",
        CheckKind::Header => "header",
        CheckKind::Schema => "schema",
        CheckKind::Match => "match",
        CheckKind::Equals => "equals",
        CheckKind::Size => "size",
        CheckKind::Timing => "timing",
        CheckKind::Cel => "cel",
        CheckKind::Custom => "custom",
    }
}

fn path(path: &Path) -> String {
    let mut text = String::new();
    for segment in &path.segments {
        match segment {
            Segment::Key(key) if text.is_empty() => text.push_str(key),
            Segment::Key(key) => {
                text.push('.');
                text.push_str(key);
            }
            Segment::Index(index) => {
                let _ = write!(text, "[{index}]");
            }
            Segment::All => text.push_str("[*]"),
        }
    }
    text
}

fn matcher(matcher: &Matcher) -> String {
    match matcher {
        Matcher::Equals(_) => "equals".to_owned(),
        Matcher::All(calls) => calls.iter().map(|call| call.name.as_str()).collect::<Vec<_>>().join(", "),
        Matcher::Status(patterns) => patterns.iter().map(|pattern| status(*pattern)).collect::<Vec<_>>().join(", "),
    }
}

fn status(pattern: StatusPattern) -> String {
    match pattern {
        StatusPattern::Code(code) => code.to_string(),
        StatusPattern::Class(class) => format!("{class}xx"),
    }
}

fn origin(program: &Program, origin: Origin) -> String {
    match origin {
        Origin::Api(api) => format!("api {}", super::name(program, api.sym())),
        Origin::Test(test) => format!("test {}", super::name(program, test.sym())),
        Origin::Action(action) => format!("step {}", super::action_label(program, action)),
    }
}
