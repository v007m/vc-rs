//! `vc plan --explain`: for each planned test, the merged expectations of each request its body
//! makes, with where each check came from (D123).

use std::collections::BTreeSet;
use std::fmt::Write as _;

use vc_model::{Check, CheckKind, Matcher, Origin, Path, Plan, Program, Segment, StatusPattern};

use super::{action_label, name};
use crate::visit::body_actions;

/// Each planned test once, in plan order, with its checks.
#[must_use]
pub fn explain(program: &Program, plan: &Plan) -> String {
    let mut out = String::new();
    let mut seen = BTreeSet::new();
    let tests = plan.files.iter().flat_map(|file| &file.instances).map(|instance| instance.test);
    for test in tests.filter(|test| seen.insert(*test)).filter_map(|test| program.tests.get(&test)) {
        let _ = writeln!(out, "{}", name(program, test.id.sym()));
        for action in body_actions(&test.body) {
            let _ = writeln!(out, "  {}", action_label(program, action));
            let checks = usize::try_from(action.index()).ok().and_then(|at| program.actions.get(at));
            for check in checks.iter().flat_map(|action| &action.expect.checks) {
                let _ = writeln!(out, "    {}  ({})", check_text(check), origin(program, check.origin));
            }
        }
    }
    out
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
        Origin::Api(api) => format!("api {}", name(program, api.sym())),
        Origin::Test(test) => format!("test {}", name(program, test.sym())),
        Origin::Action(action) => format!("step {}", action_label(program, action)),
    }
}
