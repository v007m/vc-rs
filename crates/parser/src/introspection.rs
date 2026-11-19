//! Introspection results (D347): the `__schema` an introspection query answers with, written out
//! as SDL and then read as any schema file is ([`parse_schema`]). The built-in scalars and
//! directives, and the `__` types, are left out, as SDL leaves them out.

use std::fmt::Write as _;
use std::ops::Range;

use crate::graphql::GraphqlProblem;
use crate::graphql_schema::{GraphqlSchema, parse_schema};
use crate::node::{Node, Value};
use crate::scalar::{ScalarKind, double_quote};

/// How many `NON_NULL` and `LIST` wrappers one type reference may nest; real schemas use a few.
pub const MAX_WRAPPERS: usize = 16;

/// The scalars every schema has.
const BUILT_IN_SCALARS: [&str; 5] = ["String", "Int", "Float", "Boolean", "ID"];

/// The directives every schema has.
const BUILT_IN_DIRECTIVES: [&str; 5] = ["skip", "include", "deprecated", "specifiedBy", "oneOf"];

/// The root operation types, by the `__schema` key that names each.
const ROOTS: [(&str, &str); 3] =
    [("queryType", "query"), ("mutationType", "mutation"), ("subscriptionType", "subscription")];

/// A type reference wrapper: its kind, and the text written before and after the inner type.
const WRAPPERS: [(&str, &str, &str); 2] = [("NON_NULL", "", "!"), ("LIST", "[", "]")];

/// Writes the rest of a type definition, after its keyword and name.
type Writer = fn(&Node, &mut String) -> Result<(), GraphqlProblem>;

/// Each named type kind: its introspection name, its SDL keyword and the writer of its body.
const KINDS: [(&str, &str, Writer); 6] = [
    ("SCALAR", "scalar", scalar),
    ("OBJECT", "type", object),
    ("INTERFACE", "interface", object),
    ("UNION", "union", union),
    ("ENUM", "enum", enumeration),
    ("INPUT_OBJECT", "input", input),
];

/// Reads an introspection result, `{"data": {"__schema": …}}` or `{"__schema": …}`, as a schema.
///
/// # Errors
///
/// The node that is not what an introspection result holds there, and why.
pub fn schema_from_introspection(root: &Node) -> Result<GraphqlSchema, GraphqlProblem> {
    let schema = root.get("data").unwrap_or(root).get("__schema");
    let schema = schema.ok_or_else(|| problem(root, "it is not an introspection result: no `__schema` in it"))?;
    let types = schema.get("types").filter(|types| matches!(types.value, Value::Seq(_)));
    let types = types.ok_or_else(|| problem(schema, "`__schema` has no `types` list"))?;
    let mut sdl = roots(schema)?;
    for ty in types.items() {
        named_type(ty, &mut sdl)?;
    }
    for directive in schema.get("directives").map(Node::items).unwrap_or_default() {
        self::directive(directive, &mut sdl)?;
    }
    parse_schema(&sdl).map_err(|problems| {
        let first = problems.first().map_or("", |problem| problem.message.as_str());
        problem(schema, &format!("its types do not make a schema: {first}"))
    })
}

/// The `schema { … }` block naming the root operation types, with its description, when any
/// is named.
fn roots(schema: &Node) -> Result<String, GraphqlProblem> {
    let mut block = String::new();
    for (key, operation) in ROOTS {
        let Some(root) = present(schema, key) else { continue };
        let _ = writeln!(block, "  {operation}: {}", name(root)?);
    }
    if block.is_empty() {
        return Ok(block);
    }
    let mut sdl = String::new();
    description(schema, "", &mut sdl);
    let _ = write!(sdl, "schema {{\n{block}}}\n");
    Ok(sdl)
}

/// Writes one named type, unless every schema has it.
fn named_type(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    let type_name = name(ty)?;
    if type_name.starts_with("__") || BUILT_IN_SCALARS.contains(&type_name) {
        return Ok(());
    }
    let kind = text(ty, "kind")?;
    let Some((_, keyword, writer)) = KINDS.iter().find(|(name, _, _)| *name == kind) else {
        return Err(problem(field(ty, "kind")?, &format!("`{kind}` is not a type kind")));
    };
    sdl.push('\n');
    description(ty, "", sdl);
    let _ = write!(sdl, "{keyword} {type_name}");
    writer(ty, sdl)?;
    sdl.push('\n');
    Ok(())
}

/// A scalar's `@specifiedBy`.
fn scalar(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    let Some(url) = present(ty, "specifiedByURL").or_else(|| present(ty, "specifiedByUrl")) else { return Ok(()) };
    let url = url.as_str().ok_or_else(|| problem(url, "`specifiedByURL` must be text"))?;
    let _ = write!(sdl, " @specifiedBy(url: {})", double_quote(url));
    Ok(())
}

/// An object's or an interface's interfaces and fields.
fn object(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    let interfaces = names(ty, "interfaces")?;
    if !interfaces.is_empty() {
        let _ = write!(sdl, " implements {}", interfaces.join(" & "));
    }
    sdl.push_str(" {\n");
    for field in members(ty, "fields")? {
        description(field, "  ", sdl);
        let _ = writeln!(sdl, "  {}{}: {}{}", name(field)?, arguments(field)?, type_ref(field)?, deprecated(field));
    }
    sdl.push('}');
    Ok(())
}

/// A union's members.
fn union(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    let possible: Result<Vec<&str>, _> = members(ty, "possibleTypes")?.iter().map(name).collect();
    let _ = write!(sdl, " = {}", possible?.join(" | "));
    Ok(())
}

/// An enum's values.
fn enumeration(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    sdl.push_str(" {\n");
    for value in members(ty, "enumValues")? {
        description(value, "  ", sdl);
        let _ = writeln!(sdl, "  {}{}", name(value)?, deprecated(value));
    }
    sdl.push('}');
    Ok(())
}

/// An input object's `@oneOf` and fields.
fn input(ty: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    sdl.push_str(if flag(ty, "isOneOf") { " @oneOf {\n" } else { " {\n" });
    for field in members(ty, "inputFields")? {
        description(field, "  ", sdl);
        let _ = writeln!(sdl, "  {}", input_value(field)?);
    }
    sdl.push('}');
    Ok(())
}

/// A directive definition, unless every schema has it.
fn directive(directive: &Node, sdl: &mut String) -> Result<(), GraphqlProblem> {
    let directive_name = name(directive)?;
    if BUILT_IN_DIRECTIVES.contains(&directive_name) {
        return Ok(());
    }
    let locations: Result<Vec<&str>, _> = members(directive, "locations")?.iter().map(graphql_name).collect();
    sdl.push('\n');
    description(directive, "", sdl);
    let repeatable = if flag(directive, "isRepeatable") { " repeatable" } else { "" };
    let _ =
        writeln!(sdl, "directive @{directive_name}{}{repeatable} on {}", arguments(directive)?, locations?.join(" | "));
    Ok(())
}

/// The `(…)` arguments of a field or directive, empty when it takes none.
fn arguments(holder: &Node) -> Result<String, GraphqlProblem> {
    let args = holder.get("args").map(Node::items).unwrap_or_default();
    if args.is_empty() {
        return Ok(String::new());
    }
    let written: Result<Vec<String>, GraphqlProblem> = args
        .iter()
        .map(|arg| {
            let said = arg.get("description").and_then(Node::as_str).map(|said| format!("{} ", double_quote(said)));
            Ok(format!("{}{}", said.unwrap_or_default(), input_value(arg)?))
        })
        .collect();
    Ok(format!("({})", written?.join(", ")))
}

/// `name: Type = default @deprecated` of an argument or an input field.
fn input_value(value: &Node) -> Result<String, GraphqlProblem> {
    let default = value.get("defaultValue").and_then(Node::as_str).map(|default| format!(" = {default}"));
    Ok(format!("{}: {}{}{}", name(value)?, type_ref(value)?, default.unwrap_or_default(), deprecated(value)))
}

/// The SDL of the `type` reference of `holder`, with its wrappers, at most [`MAX_WRAPPERS`].
fn type_ref(holder: &Node) -> Result<String, GraphqlProblem> {
    let mut at = field(holder, "type")?;
    let mut wrappers: Vec<&(&str, &str, &str)> = Vec::new();
    for _ in 0..=MAX_WRAPPERS {
        let kind = text(at, "kind")?;
        let Some(wrapper) = WRAPPERS.iter().find(|(name, _, _)| *name == kind) else {
            if !KINDS.iter().any(|(name, _, _)| *name == kind) {
                return Err(problem(field(at, "kind")?, &format!("`{kind}` is not a type kind")));
            }
            let inner = name(at)?.to_owned();
            return Ok(wrappers.iter().rev().fold(inner, |inner, &(_, open, close)| format!("{open}{inner}{close}")));
        };
        wrappers.push(wrapper);
        at = field(at, "ofType")?;
    }
    Err(problem(
        field(holder, "type")?,
        &format!("the type nests more than {MAX_WRAPPERS} `NON_NULL` and `LIST` wrappers"),
    ))
}

/// ` @deprecated(reason: …)` when `node` is deprecated, else nothing.
fn deprecated(node: &Node) -> String {
    let reason = node.get("deprecationReason").and_then(Node::as_str);
    match (flag(node, "isDeprecated"), reason) {
        (false, _) => String::new(),
        (true, None) => " @deprecated".to_owned(),
        (true, Some(reason)) => format!(" @deprecated(reason: {})", double_quote(reason)),
    }
}

/// Writes the `description` of `node` on a line of its own, indented, when it has one.
fn description(node: &Node, indent: &str, sdl: &mut String) {
    if let Some(said) = node.get("description").and_then(Node::as_str) {
        let _ = writeln!(sdl, "{indent}{}", double_quote(said));
    }
}

/// The value at `key` of `node`, unless it is missing or `null`.
fn present<'n>(node: &'n Node, key: &str) -> Option<&'n Node> {
    node.get(key).filter(|value| value.as_scalar().is_none_or(|scalar| scalar.kind != ScalarKind::Null))
}

/// Whether `key` of `node` is `true`.
fn flag(node: &Node, key: &str) -> bool {
    node.get(key).and_then(Node::as_scalar).is_some_and(|flag| flag.kind == ScalarKind::Bool && flag.text == "true")
}

/// The items listed under `key` of `node`, which a schema needs at least one of: an object's
/// fields, a union's members, an enum's values, an input's fields or a directive's locations.
fn members<'n>(node: &'n Node, key: &str) -> Result<&'n [Node], GraphqlProblem> {
    let items = node.get(key).map(Node::items).unwrap_or_default();
    if items.is_empty() {
        return Err(problem(node, &format!("`{key}` lists nothing here; a schema needs at least one")));
    }
    Ok(items)
}

/// The names of the types listed under `key`, such as an object's `interfaces`.
fn names<'n>(ty: &'n Node, key: &str) -> Result<Vec<&'n str>, GraphqlProblem> {
    ty.get(key).map(Node::items).unwrap_or_default().iter().map(name).collect()
}

/// The `name` of `node`, which must be a GraphQL name.
fn name(node: &Node) -> Result<&str, GraphqlProblem> {
    graphql_name(field(node, "name")?)
}

/// The text of `node`, which must be a GraphQL name: a letter or `_`, then letters, digits or `_`.
fn graphql_name(node: &Node) -> Result<&str, GraphqlProblem> {
    let named = node.as_str().filter(|text| {
        let mut chars = text.chars();
        chars.next().is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
            && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    });
    named.ok_or_else(|| problem(node, "expected a GraphQL name here"))
}

/// The text at `key` of `node`.
fn text<'n>(node: &'n Node, key: &str) -> Result<&'n str, GraphqlProblem> {
    let value = field(node, key)?;
    value.as_str().ok_or_else(|| problem(value, &format!("`{key}` must be text")))
}

/// The value at `key` of `node`.
fn field<'n>(node: &'n Node, key: &str) -> Result<&'n Node, GraphqlProblem> {
    node.get(key).ok_or_else(|| problem(node, &format!("expected `{key}` here")))
}

/// The problem `message`, at the bytes of `node`.
fn problem(node: &Node, message: &str) -> GraphqlProblem {
    let at: Range<usize> = node.span.start.offset..node.span.end.offset;
    GraphqlProblem { message: message.to_owned(), at }
}

#[cfg(test)]
#[path = "introspection_tests.rs"]
mod tests;
