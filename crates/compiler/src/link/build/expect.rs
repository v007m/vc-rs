//! `expect` blocks as checks, and the merge of the levels a call inherits (D123).

use vc_model::{Check, CheckKind, Expect, Matcher, Origin, Path, Segment, SourceSpan, TemplateValue};
use vc_parser::Node;

use super::read;
use crate::link::project::{Source, text};
use crate::parts::Piece;

/// The checks one level writes, each naming `origin`.
pub fn read(source: &Source<'_>, node: Option<&Node>, origin: Origin) -> Expect {
    let Some(node) = node else { return Expect::default() };
    let mut reader = Reader { source, origin, checks: Vec::new() };
    reader.matcher(CheckKind::Status, node.get("status"), None);
    reader.keyed(CheckKind::Header, node.get("headers"), |name| Path { segments: vec![key(&name.to_lowercase())] });
    let body = node.get("body");
    reader.keyed(CheckKind::Match, body.and_then(|body| body.get("match")), path);
    reader.equals(body.and_then(|body| body.get("equals")));
    reader.matcher(CheckKind::Size, node.get("size"), None);
    reader.keyed(CheckKind::Timing, node.get("timing"), |phase| Path { segments: vec![key(phase)] });
    reader.cel(node.get("cel"));
    reader.custom(node.get("custom"));
    Expect { checks: reader.checks, replace: read::flag(node, "replace") }
}

/// Merges `levels`, least specific first: a check replaces an earlier one with the same kind and
/// target; CEL and custom checks append; a level with `replace: true` drops what came before.
pub fn merge(levels: &[&Expect]) -> Expect {
    let mut checks: Vec<Check> = Vec::new();
    for level in levels {
        if level.replace {
            checks.clear();
        }
        for check in &level.checks {
            let appends = matches!(check.kind, CheckKind::Cel | CheckKind::Custom);
            let same = |old: &Check| old.kind == check.kind && old.target == check.target;
            match checks.iter_mut().find(|old| !appends && same(old)) {
                Some(old) => *old = check.clone(),
                None => checks.push(check.clone()),
            }
        }
    }
    Expect { checks, replace: false }
}

struct Reader<'s, 'a> {
    source: &'s Source<'a>,
    origin: Origin,
    checks: Vec<Check>,
}

impl Reader<'_, '_> {
    fn push(&mut self, (kind, target): (CheckKind, Option<Path>), matcher: Matcher, span: SourceSpan) {
        self.checks.push(Check { kind, target, matcher, origin: self.origin, span });
    }

    fn matcher(&mut self, kind: CheckKind, node: Option<&Node>, target: Option<Path>) {
        let Some(node) = node else { return };
        if let Some(Piece::Matcher(matcher)) = self.source.piece(node) {
            self.push((kind, target), matcher.clone(), node.span);
        }
    }

    fn keyed(&mut self, kind: CheckKind, node: Option<&Node>, target: fn(&str) -> Path) {
        for entry in node.map(Node::entries).unwrap_or_default() {
            self.matcher(kind, Some(&entry.value), Some(target(&entry.key)));
        }
    }

    fn equals(&mut self, node: Option<&Node>) {
        let Some(node) = node else { return };
        if let Some(value) = read::value(self.source, node) {
            self.push((CheckKind::Equals, None), Matcher::Equals(value), node.span);
        }
    }

    /// A CEL check by label; it holds when the expression is true.
    fn cel(&mut self, node: Option<&Node>) {
        for entry in node.map(Node::entries).unwrap_or_default() {
            if let Some(Piece::Cel(cel)) = self.source.piece(&entry.value) {
                let target = Some(Path { segments: vec![key(&entry.key)] });
                self.push((CheckKind::Cel, target), Matcher::Equals(TemplateValue::Cel(cel.clone())), entry.value.span);
            }
        }
    }

    /// A function check, by `path#export`; it holds when the function passes.
    fn custom(&mut self, node: Option<&Node>) {
        for item in node.map(Node::items).unwrap_or_default() {
            let Some(function) = text(item, "function") else { continue };
            let target = Some(Path { segments: vec![key(function)] });
            self.push((CheckKind::Custom, target), Matcher::Equals(TemplateValue::Bool(true)), item.span);
        }
    }
}

fn key(name: &str) -> Segment {
    Segment::Key(name.to_owned())
}

/// A body path as written under `match`: dotted keys, `[n]`, `[*]` and quoted keys.
pub fn path(text: &str) -> Path {
    let mut segments = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (segment, after) = rest.strip_prefix('[').map_or_else(|| plain(rest), bracket);
        segments.extend(segment);
        rest = after.strip_prefix('.').unwrap_or(after);
    }
    Path { segments }
}

fn plain(text: &str) -> (Option<Segment>, &str) {
    let end = text.find(['.', '[']).unwrap_or(text.len());
    let name = &text[..end];
    ((!name.is_empty()).then(|| key(name)), &text[end..])
}

fn bracket(text: &str) -> (Option<Segment>, &str) {
    let Some(end) = text.find(']') else { return (Some(key(text)), "") };
    let inner = &text[..end];
    let quoted = ['\'', '"'].iter().find_map(|quote| inner.strip_prefix(*quote)?.strip_suffix(*quote));
    let segment = match (inner, quoted) {
        (_, Some(name)) => key(name),
        ("*", None) => Segment::All,
        _ => inner.parse().map_or_else(|_| key(inner), Segment::Index),
    };
    (Some(segment), &text[end + 1..])
}

/// The checks under `until`, for `poll`.
pub fn until(source: &Source<'_>, node: &Node, origin: Origin) -> Expect {
    read(source, node.get("until"), origin)
}

#[cfg(test)]
mod tests {
    use vc_model::{
        Check, CheckKind, Expect, Matcher, Origin, Path, Pos, Segment, SourceFileId, SourceSpan, TemplateValue, TestId,
    };

    use super::{merge, path};

    fn check(kind: CheckKind, target: &str, line: u32) -> Check {
        let at = Pos::new(line, 1, 0);
        let target = (!target.is_empty()).then(|| path(target));
        let origin = Origin::Test(TestId::new(vc_model::StringTable::new().intern("t").unwrap()));
        let matcher = Matcher::Equals(TemplateValue::Null);
        Check { kind, target, matcher, origin, span: SourceSpan::new(SourceFileId::new(0), at, at) }
    }

    fn lines(expect: &Expect) -> Vec<u32> {
        expect.checks.iter().map(|check| check.span.start.line).collect()
    }

    #[test]
    fn a_later_level_replaces_the_same_kind_and_target_in_place() {
        let api =
            Expect { checks: vec![check(CheckKind::Status, "", 1), check(CheckKind::Match, "a", 2)], replace: false };
        let test = Expect { checks: vec![check(CheckKind::Match, "b", 3)], replace: false };
        let step =
            Expect { checks: vec![check(CheckKind::Status, "", 4), check(CheckKind::Match, "a", 5)], replace: false };
        assert_eq!(lines(&merge(&[&api, &test, &step])), [4, 5, 3]);
    }

    #[test]
    fn cel_and_custom_checks_append() {
        let api =
            Expect { checks: vec![check(CheckKind::Cel, "x", 1), check(CheckKind::Custom, "f#g", 2)], replace: false };
        let step =
            Expect { checks: vec![check(CheckKind::Cel, "x", 3), check(CheckKind::Custom, "f#g", 4)], replace: false };
        assert_eq!(lines(&merge(&[&api, &step])), [1, 2, 3, 4]);
    }

    #[test]
    fn replace_drops_the_inherited_checks() {
        let api = Expect { checks: vec![check(CheckKind::Status, "", 1)], replace: false };
        let step = Expect { checks: vec![check(CheckKind::Size, "", 2)], replace: true };
        let merged = merge(&[&api, &step]);
        assert_eq!(lines(&merged), [2]);
        assert!(!merged.replace);
    }

    #[test]
    fn reads_body_paths() {
        let segments = |text: &str| path(text).segments;
        let key = |name: &str| Segment::Key(name.to_owned());
        assert_eq!(segments("items[*].price"), [key("items"), Segment::All, key("price")]);
        assert_eq!(segments("a[-1]['x.y'][\"z\"]"), [key("a"), Segment::Index(-1), key("x.y"), key("z")]);
        assert_eq!(segments("a[b].c"), [key("a"), key("b"), key("c")]);
        assert_eq!(segments("a..b[open"), [key("a"), key("b"), key("open")]);
        assert_eq!(path(""), Path { segments: vec![] });
    }
}
