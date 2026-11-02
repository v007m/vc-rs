//! Finds the roots a parsed CEL expression reads, and the calls `vc check` can reject early.

use std::ops::Range;

use cel::common::ast::{
    CallExpr, ComprehensionExpr, EntryExpr, Expr, IdedEntryExpr, IdedExpr, LiteralValue, SourceInfo,
};
use vc_model::Segment;

use crate::cel_lex::VAR_ALIAS;

/// Words CEL resolves itself: type names and the `optional` namespace.
const BUILTINS: &[&str] =
    &["bool", "bytes", "double", "dyn", "int", "list", "map", "null_type", "optional", "string", "type", "uint"];

/// Functions the interpreter is built without (the `chrono` feature fails `cargo deny`).
const UNSUPPORTED: &[&str] = &["timestamp", "duration"];

/// The function whose second argument (or first, called on a string) is a regex.
const MATCHES: &str = "matches";

/// The index operator, `x[i]`.
const INDEX: &str = "_[_]";

/// A root read by the expression: `response.body.items[0]`.
#[derive(Debug, PartialEq, Eq)]
pub struct Root {
    /// As written; `var` for the renamed root.
    pub name: String,
    /// The fields and literal indexes selected after the root.
    pub path: Vec<Segment>,
    /// The root and its path.
    pub range: Range<usize>,
    /// The root's identifier alone.
    pub ident: Range<usize>,
}

/// A call `vc check` rejects.
#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    /// A standard function the interpreter does not have: its name.
    Unsupported(String, Range<usize>),
    /// A literal pattern given to `matches()` that is not a valid regex, with the reason.
    BadRegex(String, Range<usize>),
}

/// The roots `expr` reads, in source order.
///
/// # Errors
///
/// The first [`Problem`] found.
pub fn roots(expr: &IdedExpr, info: &SourceInfo, text: &str) -> Result<Vec<Root>, Problem> {
    let mut walk = Walk { text, info, scope: Vec::new(), roots: Vec::new() };
    walk.expr(expr)?;
    Ok(walk.roots)
}

struct Walk<'a> {
    text: &'a str,
    info: &'a SourceInfo,
    scope: Vec<String>,
    roots: Vec<Root>,
}

impl Walk<'_> {
    fn expr(&mut self, expr: &IdedExpr) -> Result<(), Problem> {
        if let Some(chain) = chain(expr) {
            self.record(expr, chain);
            return Ok(());
        }
        match &expr.expr {
            Expr::Select(select) => self.expr(&select.operand),
            Expr::Call(call) => self.call(expr, call),
            Expr::Comprehension(comprehension) => self.comprehension(comprehension),
            Expr::List(list) => list.elements.iter().try_for_each(|element| self.expr(element)),
            Expr::Map(map) => self.entries(&map.entries),
            Expr::Struct(object) => self.entries(&object.entries),
            Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => Ok(()),
        }
    }

    fn call(&mut self, expr: &IdedExpr, call: &CallExpr) -> Result<(), Problem> {
        if UNSUPPORTED.contains(&call.func_name.as_str()) {
            let open = self.start(expr);
            let start = open.saturating_sub(call.func_name.len());
            return Err(Problem::Unsupported(call.func_name.clone(), start..open));
        }
        if call.func_name == MATCHES {
            self.regex(call)?;
        }
        call.target.iter().map(AsRef::as_ref).chain(&call.args).try_for_each(|arg| self.expr(arg))
    }

    fn regex(&self, call: &CallExpr) -> Result<(), Problem> {
        let pattern = call.args.get(usize::from(call.target.is_none()));
        let Some(IdedExpr { id, expr: Expr::Literal(LiteralValue::String(pattern)) }) = pattern else {
            return Ok(());
        };
        regex::Regex::new(pattern.inner()).map(drop).map_err(|error| {
            let reason = error.to_string().lines().last().unwrap_or_default().trim().to_owned();
            Problem::BadRegex(reason, self.range(*id))
        })
    }

    fn comprehension(&mut self, comprehension: &ComprehensionExpr) -> Result<(), Problem> {
        self.expr(&comprehension.iter_range)?;
        self.expr(&comprehension.accu_init)?;
        let outer = self.scope.len();
        self.scope.push(comprehension.iter_var.clone());
        self.scope.extend(comprehension.iter_var2.clone());
        self.scope.push(comprehension.accu_var.clone());
        let body = [&comprehension.loop_cond, &comprehension.loop_step, &comprehension.result];
        let walked = body.into_iter().try_for_each(|part| self.expr(part));
        self.scope.truncate(outer);
        walked
    }

    fn entries(&mut self, entries: &[IdedEntryExpr]) -> Result<(), Problem> {
        entries.iter().try_for_each(|entry| match &entry.expr {
            EntryExpr::MapEntry(pair) => self.expr(&pair.key).and_then(|()| self.expr(&pair.value)),
            EntryExpr::StructField(field) => self.expr(&field.value),
        })
    }

    fn record(&mut self, expr: &IdedExpr, chain: Chain<'_>) {
        let name = chain.name;
        if BUILTINS.contains(&name) || self.scope.iter().any(|bound| bound == name) {
            return;
        }
        let ident = self.range(chain.ident);
        let range = ident.start..self.end(expr).max(ident.end);
        let name = if name == VAR_ALIAS { "var" } else { name };
        self.roots.push(Root { name: name.to_owned(), path: chain.path, range, ident });
    }

    fn start(&self, expr: &IdedExpr) -> usize {
        self.range(expr.id).start
    }

    /// The bytes of a token the parser recorded: an identifier or a literal.
    fn range(&self, id: u64) -> Range<usize> {
        let (start, stop) = self.info.offset_for(id).unwrap_or_default();
        let start = usize::try_from(start).unwrap_or_default();
        start..usize::try_from(stop).unwrap_or_default().max(start) + 1
    }

    /// Where a chain of selects and literal indexes ends in the text.
    fn end(&self, expr: &IdedExpr) -> usize {
        match &expr.expr {
            Expr::Select(select) => {
                let after = self.end(&select.operand);
                after_tokens(self.text, after, &[".", &select.field])
            }
            Expr::Call(call) => {
                let after = call.args.get(1).map_or(0, |index| self.range(index.id).end);
                after_tokens(self.text, after, &["]"])
            }
            _ => self.range(expr.id).end,
        }
    }
}

/// The offset after `tokens`, each preceded by optional whitespace, starting at `from`;
/// `from` when the text does not match.
fn after_tokens(text: &str, from: usize, tokens: &[&str]) -> usize {
    tokens
        .iter()
        .try_fold(from, |at, token| {
            let rest = text.get(at..)?;
            let trimmed = rest.trim_start();
            trimmed.starts_with(token).then(|| at + rest.len() - trimmed.len() + token.len())
        })
        .unwrap_or(from)
}

/// An identifier followed only by fields and literal indexes: `response.body.items[0]`.
struct Chain<'a> {
    /// The identifier's node id.
    ident: u64,
    name: &'a str,
    path: Vec<Segment>,
}

fn chain(expr: &IdedExpr) -> Option<Chain<'_>> {
    let mut path = Vec::new();
    let mut node = expr;
    loop {
        node = match &node.expr {
            Expr::Ident(name) => {
                path.reverse();
                return Some(Chain { ident: node.id, name, path });
            }
            Expr::Select(select) => {
                path.push(Segment::Key(select.field.clone()));
                &select.operand
            }
            Expr::Call(call) if call.func_name == INDEX && call.target.is_none() => {
                path.push(index(call.args.get(1)?)?);
                call.args.first()?
            }
            _ => return None,
        };
    }
}

fn index(expr: &IdedExpr) -> Option<Segment> {
    match &expr.expr {
        Expr::Literal(LiteralValue::Int(n)) => Some(Segment::Index(*n.inner())),
        Expr::Literal(LiteralValue::String(key)) => Some(Segment::Key(key.inner().to_owned())),
        _ => None,
    }
}
