//! The config's `auth` definitions, with a `flow`'s steps as run-owned actions, and the
//! `secrets` and `redact` settings.

use indexmap::IndexMap;
use serde_json::Number;
use vc_model::{
    ActionId, AuthDef, AuthId, Config, InlineRequest, Owner, Part, RedactRule, SecretsConfig, Template, TemplateValue,
};
use vc_parser::{Node, ScalarKind, Value};

use super::action::Place;
use super::{Build, Built, read};
use crate::link::project;
use crate::link::project::Source;
use crate::parts::Piece;
use crate::shapes;

/// The keys an [`AuthDef`] holds in its own fields; every other key is a setting of its type.
const OWN_KEYS: [&str; 8] = ["id", "type", "vars", "steps", "expiresIn", "refreshBefore", "refreshOn", "cache"];

impl<'a> Build<'_, 'a> {
    /// Adds every entry of the config's `auth`.
    pub fn auth_defs(&mut self, source: &Source<'a>) -> Built<()> {
        for node in project::list(source.root, "auth") {
            let Some(def) = self.auth_def(source, node)? else { continue };
            self.builder.auth(def);
        }
        Ok(())
    }

    fn auth_def(&mut self, source: &Source<'a>, node: &Node) -> Built<Option<AuthDef>> {
        let (Some(id), Some(kind)) = (project::text(node, "id"), project::text(node, "type")) else {
            return Ok(None);
        };
        let mut def = AuthDef::new(AuthId::new(self.sym(id)?), kind);
        def.name = read::owned(node, "name");
        def.vars = self.vars(source, node)?;
        def.settings = settings(source, node);
        def.steps = self.flow_steps(source, node)?;
        def.expires_in = node.get("expiresIn").and_then(|at| duration_text(source, at));
        def.refresh_before = read::duration(source, node, "refreshBefore").unwrap_or(def.refresh_before);
        if let Some(statuses) = node.get("refreshOn") {
            def.refresh_on =
                statuses.items().iter().filter_map(|status| status.as_scalar()?.text.parse().ok()).collect();
        }
        def.cache = read::flag(shapes::auth(), node, "cache");
        Ok(Some(def))
    }

    fn flow_steps(&mut self, source: &Source<'a>, node: &Node) -> Built<Vec<ActionId>> {
        let place = Place { owner: Owner::Run, defaults: None, test: None };
        let mut ids = Vec::new();
        for step in project::list(node, "steps") {
            let Some(action) = self.action(source, step, &place)? else { continue };
            ids.push(self.builder.action(action)?);
        }
        Ok(ids)
    }
}

/// A flow step's request written in place: `url` with its method, headers and body.
pub fn inline(source: &Source<'_>, node: &Node) -> Option<InlineRequest> {
    let url = read::template(source, node.get("url")?)?;
    let body = node.get("body").and_then(|body| read::body(source, body)).unwrap_or_default();
    let (query, headers) = (read::templates(source, node, "query"), read::templates(source, node, "headers"));
    Some(InlineRequest { method: read::method(node), url, query, headers, body })
}

/// `secrets`, with the names the schema declares.
fn secrets(root: &Node, declared: &[String]) -> SecretsConfig {
    let defaults = SecretsConfig::default();
    let secrets = root.get("secrets");
    SecretsConfig {
        provider: secrets.and_then(|node| read::owned(node, "provider")).unwrap_or(defaults.provider),
        schema: secrets.and_then(|node| read::owned(node, "schema")).unwrap_or(defaults.schema),
        declared: declared.to_vec(),
    }
}

/// `redact.rules`, in the order written.
fn redact_rules(source: &Source<'_>, root: &Node) -> Vec<RedactRule> {
    let rules = root.get("redact").map(|redact| project::list(redact, "rules")).unwrap_or_default();
    rules.iter().filter_map(|rule| redact_rule(source, rule)).collect()
}

fn redact_rule(source: &Source<'_>, rule: &Node) -> Option<RedactRule> {
    if let Some(regex) = read::owned(rule, "regex") {
        return Some(RedactRule::Regex(regex));
    }
    match source.piece(rule.get("cel")?)? {
        Piece::Cel(program) => Some(RedactRule::Cel(program.clone())),
        _ => None,
    }
}

/// Keeps the config's other settings and adds `secrets` and `redact.rules`.
pub fn with_secrets(config: Config, source: &Source<'_>, declared: &[String]) -> Config {
    Config { secrets: secrets(source.root, declared), redact_rules: redact_rules(source, source.root), ..config }
}

/// A duration that may be a template (`${var.expiresIn}s`), as the text to render and parse.
fn duration_text(source: &Source<'_>, node: &Node) -> Option<Template> {
    match source.piece(node) {
        Some(Piece::Template(template)) => Some(template.clone()),
        _ => node.as_scalar().map(|scalar| literal(&scalar.text)),
    }
}

/// The type's own keys: every key but the [`OWN_KEYS`], as compiled.
fn settings(source: &Source<'_>, node: &Node) -> IndexMap<String, TemplateValue> {
    let entries = node.entries().iter().filter(|entry| !OWN_KEYS.contains(&entry.key.as_str()));
    entries.map(|entry| (entry.key.clone(), setting(source, &entry.value))).collect()
}

/// The value the compiler lowered at `node`, or the value as written where it lowered nothing.
fn setting(source: &Source<'_>, node: &Node) -> TemplateValue {
    match source.piece(node) {
        Some(Piece::Value(value)) => return value.clone(),
        Some(Piece::Template(template)) => return TemplateValue::String(template.clone()),
        _ => {}
    }
    match &node.value {
        Value::Scalar(scalar) => match scalar.kind {
            ScalarKind::Bool => TemplateValue::Bool(scalar.text.eq_ignore_ascii_case("true")),
            ScalarKind::Int => scalar.text.parse::<i64>().map_or_else(|_| text(&scalar.text), number),
            ScalarKind::Null | ScalarKind::Float | ScalarKind::String => text(&scalar.text),
        },
        Value::Seq(items) => TemplateValue::List(items.iter().map(|item| setting(source, item)).collect()),
        Value::Map(entries) => {
            TemplateValue::Map(entries.iter().map(|entry| (entry.key.clone(), setting(source, &entry.value))).collect())
        }
    }
}

fn number(value: i64) -> TemplateValue {
    TemplateValue::Number(Number::from(value))
}

fn text(written: &str) -> TemplateValue {
    TemplateValue::String(literal(written))
}

fn literal(written: &str) -> Template {
    Template { parts: vec![Part::Literal(written.to_owned())] }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
