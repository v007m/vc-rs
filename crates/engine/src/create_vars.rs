//! The vars a test from `vc new test` sets for the api it calls (D262): each `${var.…}` in the
//! api's path that the config does not set, given the api's own value (a sample from the spec,
//! when the api was imported) or a placeholder, and marked `# replace`, so the new file passes
//! `vc check`.

use std::collections::BTreeSet;

use vc_model::FileKind;
use vc_parser::{Document, Node};

use crate::EngineError;
use crate::authoring::{Authoring, Named, item};

/// What a path var with no value of its own is set to.
pub const PLACEHOLDER: &str = "replace-me";

/// The `vars:` lines a test calling `api` (`users#deactivate`) needs, each after a line break, or
/// nothing when its path holds no var the config leaves unset.
pub fn path_vars(project: &Authoring, api: &str) -> Result<String, EngineError> {
    let named = Named::parse(api);
    let Some(found) = project.find(FileKind::Api, named.file) else { return Ok(String::new()) };
    let opened = Authoring::open(found)?;
    let Some(node) = item(opened.document.root(), "apis", named.item.unwrap_or_default()) else {
        return Ok(String::new());
    };
    let set = config_vars(project)?;
    let path = node.get("path").and_then(Node::as_str).unwrap_or_default();
    let mut lines = String::new();
    for name in vars_in(path).into_iter().filter(|name| !set.contains(*name)) {
        let own = node.get("vars").and_then(|vars| vars.get(name));
        lines.push_str(&match own.and_then(|value| written(&opened.document, value)) {
            Some(sample) => format!("\n  {name}: {sample}  # replace: the api's sample value"),
            None => format!("\n  {name}: {PLACEHOLDER}  # replace with a real {name}"),
        });
    }
    Ok(if lines.is_empty() { lines } else { format!("\nvars:{lines}") })
}

/// The names of the `${var.…}` references in `path`, each once, in order.
fn vars_in(path: &str) -> Vec<&str> {
    let mut names = Vec::new();
    for (_, rest) in path.match_indices("${var.").filter_map(|(at, _)| path.split_at_checked(at + 6)) {
        let end = rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_').unwrap_or(rest.len());
        let name = rest.get(..end).unwrap_or_default();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// The vars the config sets, at its root or in any profile.
fn config_vars(project: &Authoring) -> Result<BTreeSet<String>, EngineError> {
    let Some(found) = project.entries().find(|(_, file)| file.kind() == Some(FileKind::Config)) else {
        return Ok(BTreeSet::new());
    };
    let opened = Authoring::open(found)?;
    let root = opened.document.root();
    let profiles = root.get("profiles").map(Node::items).unwrap_or_default();
    let maps = std::iter::once(root).chain(profiles).filter_map(|holder| holder.get("vars"));
    Ok(maps.flat_map(Node::entries).map(|entry| entry.key.clone()).collect())
}

/// The YAML of the scalar `value` as written, when it fits on one line.
fn written<'d>(document: &'d Document, value: &Node) -> Option<&'d str> {
    value.as_scalar()?;
    let text = document.text().get(value.span.start.offset..value.span.end.offset)?;
    (!text.is_empty() && !text.contains('\n')).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::vars_in;

    #[test]
    fn a_path_names_each_var_once_in_order() {
        assert_eq!(vars_in("/users/${var.userId}/posts/${var.postId}"), ["userId", "postId"]);
        assert_eq!(vars_in("/a/${var.id}/b/${var.id}"), ["id"]);
        assert_eq!(vars_in("/users/${var.id | upper}"), ["id"]);
        assert!(vars_in("/users/${env.HOME}/${var.}").is_empty());
    }
}
