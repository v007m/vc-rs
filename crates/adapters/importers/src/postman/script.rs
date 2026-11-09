//! The values a test script saves from the response, as a step's `save`: a script that sets a
//! variable to a field of the response body (`pm.environment.set("cartId",
//! pm.response.json().id)`, directly or through a name the body was given) saves
//! `cartId: ${response.body.id}`. Nothing else a script does is imported.

use indexmap::IndexMap;

use crate::naming::camel;

/// The calls that set a variable.
const SETTERS: [&str; 6] = [
    "pm.environment.set(",
    "pm.collectionVariables.set(",
    "pm.globals.set(",
    "pm.variables.set(",
    "postman.setEnvironmentVariable(",
    "postman.setGlobalVariable(",
];

/// The expressions that read the response body as JSON.
const BODIES: [&str; 2] = ["pm.response.json()", "JSON.parse(responseBody)"];

/// The vars `script` saves, each with the template that reads its value from the response.
pub fn saves(script: &str) -> IndexMap<String, String> {
    let aliases = aliases(script);
    let mut found = Vec::new();
    for setter in SETTERS {
        for (at, _) in script.match_indices(setter) {
            let args = script.get(at + setter.len()..).unwrap_or_default();
            let Some((name, value)) = arguments(args) else { continue };
            let var = camel(name);
            if let Some(path) = body_path(value, &aliases).filter(|_| !var.is_empty()) {
                found.push((at, var, format!("${{response.body{path}}}")));
            }
        }
    }
    found.sort_by_key(|(at, _, _)| *at);
    found.into_iter().map(|(_, var, template)| (var, template)).collect()
}

/// The names the script gives the response body: `var data = pm.response.json();`.
fn aliases(script: &str) -> Vec<&str> {
    let mut aliases = Vec::new();
    for statement in script.split([';', '\n']) {
        let statement = statement.trim();
        let declared = ["var ", "let ", "const "].iter().find_map(|word| statement.strip_prefix(word));
        let Some((name, value)) = declared.and_then(|rest| rest.split_once('=')) else { continue };
        let name = name.trim();
        if plain(name) && BODIES.contains(&value.trim()) {
            aliases.push(name);
        }
    }
    aliases
}

/// The quoted name and the value expression of a setter's arguments.
fn arguments(args: &str) -> Option<(&str, &str)> {
    let args = args.trim_start();
    let quote = args.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let (name, rest) = args.get(1..)?.split_once(quote)?;
    let rest = rest.trim_start().strip_prefix(',')?;
    let mut depth = 0_usize;
    for (at, c) in rest.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' if depth == 0 => return Some((name, rest.get(..at)?.trim())),
            ')' | ']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// The path into the body that `value` reads (`.items[0].id`), when it reads one.
fn body_path(value: &str, aliases: &[&str]) -> Option<String> {
    let root = BODIES
        .iter()
        .chain(aliases)
        .find(|root| value.strip_prefix(**root).is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '['])))?;
    let mut rest = value.get(root.len()..)?;
    let mut path = String::new();
    while !rest.is_empty() {
        let (step, after) = access(rest)?;
        path.push_str(&step);
        rest = after;
    }
    Some(path)
}

/// The first access of `text` as template text (`.id`, `[0]`, `["id"]` gives `.id`) and the
/// rest.
fn access(text: &str) -> Option<(String, &str)> {
    if let Some(rest) = text.strip_prefix('.') {
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
        let (key, rest) = rest.split_at_checked(end)?;
        return plain(key).then(|| (format!(".{key}"), rest));
    }
    let (inside, rest) = text.strip_prefix('[')?.split_once(']')?;
    let inside = inside.trim();
    if !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
        return Some((format!("[{inside}]"), rest));
    }
    let key = inside.strip_prefix(['"', '\''])?.strip_suffix(['"', '\''])?;
    plain(key).then(|| (format!(".{key}"), rest))
}

/// Whether `key` can follow a `.` in a template.
fn plain(key: &str) -> bool {
    key.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::saves;

    fn saved(script: &str) -> Vec<(String, String)> {
        saves(script).into_iter().collect()
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected.iter().map(|(name, template)| ((*name).to_owned(), (*template).to_owned())).collect()
    }

    #[test]
    fn a_variable_set_from_the_body_is_saved() {
        let script = "pm.test('ok', () => pm.response.to.have.status(201));\n\
                      pm.collectionVariables.set(\"cart_id\", pm.response.json().id);\n\
                      pm.environment.set('firstSku', pm.response.json().items[0][\"sku\"]);";
        assert_eq!(
            saved(script),
            pairs(&[("cartId", "${response.body.id}"), ("firstSku", "${response.body.items[0].sku}")])
        );
    }

    #[test]
    fn a_name_given_to_the_body_reads_it_too() {
        let script = "var jsonData = JSON.parse(responseBody);\nconst body = pm.response.json()\n\
                      postman.setEnvironmentVariable(\"orderId\", jsonData.order.id);\n\
                      pm.globals.set(\"total\", body.total);";
        assert_eq!(
            saved(script),
            pairs(&[("orderId", "${response.body.order.id}"), ("total", "${response.body.total}")])
        );
    }

    #[test]
    fn anything_but_a_field_of_the_body_is_not_saved() {
        let script = "pm.environment.set(\"stamp\", Date.now());\n\
                      pm.environment.set(\"upper\", pm.response.json().name.toUpperCase());\n\
                      pm.environment.set(\"header\", pm.response.headers.get(\"X-Id\"));\n\
                      pm.environment.set(\"odd\", pm.response.json()[\"a-b\"]);\n\
                      pm.environment.set(name, pm.response.json().id);";
        assert_eq!(saved(script), pairs(&[]));
    }
}
