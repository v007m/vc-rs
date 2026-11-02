//! Architecture rules as data, checked by `tests/rules.rs`.
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use syn::visit::Visit;

/// Allowed *internal* dependencies per workspace crate. A crate missing from
/// this table is itself a violation, so adding a crate forces a rule decision.
pub const ALLOWED_INTERNAL_DEPS: &[(&str, &[&str])] = &[
    ("ir", &[]),
    ("tsstrip", &[]),
    ("ports", &["ir"]),
    ("compiler", &["ir"]),
    ("engine", &["ir", "ports"]),
    ("adapters-http", &["ir", "ports"]),
    ("adapters-store", &["ports"]),
    ("adapters-hooks", &["ir", "ports", "tsstrip"]),
    ("cli", &["ir", "ports", "engine", "compiler", "adapters-http", "adapters-store", "adapters-hooks"]),
    ("arch-tests", &[]),
];

/// Internal dependency graph: crate name -> internal crates it depends on.
pub type DepGraph = BTreeMap<String, BTreeSet<String>>;

/// Every edge not allowed by [`ALLOWED_INTERNAL_DEPS`].
#[must_use]
pub fn dependency_violations(graph: &DepGraph) -> Vec<String> {
    let allowed: BTreeMap<&str, &[&str]> = ALLOWED_INTERNAL_DEPS.iter().copied().collect();
    let mut out = Vec::new();
    for (krate, deps) in graph {
        let Some(ok) = allowed.get(krate.as_str()) else {
            out.push(format!("crate `{krate}` has no entry in ALLOWED_INTERNAL_DEPS"));
            continue;
        };
        out.extend(
            deps.iter().filter(|d| !ok.contains(&d.as_str())).map(|d| format!("`{krate}` must not depend on `{d}`")),
        );
    }
    out
}

/// Build the internal dependency graph (normal + build deps; dev-deps excluded).
///
/// # Errors
/// If `cargo metadata` fails.
pub fn workspace_graph(manifest: &Path) -> Result<DepGraph, cargo_metadata::Error> {
    let meta = cargo_metadata::MetadataCommand::new().manifest_path(manifest).no_deps().exec()?;
    let members: BTreeSet<String> = meta.workspace_packages().iter().map(|p| p.name.to_string()).collect();
    Ok(meta
        .workspace_packages()
        .iter()
        .map(|p| {
            let deps = p
                .dependencies
                .iter()
                .filter(|d| d.kind != cargo_metadata::DependencyKind::Development && members.contains(&d.name))
                .map(|d| d.name.clone())
                .collect();
            (p.name.to_string(), deps)
        })
        .collect())
}

struct Statics {
    found: Vec<String>,
}

impl Visit<'_> for Statics {
    fn visit_item_static(&mut self, item: &syn::ItemStatic) {
        self.found.push(item.ident.to_string());
    }
}

/// Global data: every `static` item (mutable, atomic, lazy or locked) is reported.
///
/// # Errors
/// If the source does not parse.
pub fn static_items(source: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut v = Statics { found: Vec::new() };
    v.visit_file(&file);
    Ok(v.found)
}

const PRIMITIVES: &[&str] = &["String", "str", "u16", "u32", "u64", "usize", "i32", "i64"];

fn smells_like_domain_value(field: &str) -> bool {
    let f = field.to_ascii_lowercase();
    f == "id"
        || f.ends_with("_id")
        || ["status", "method", "url", "path", "token"].iter().any(|k| f == *k || f.ends_with(&format!("_{k}")))
}

struct PrimitiveFields {
    found: Vec<String>,
}

impl Visit<'_> for PrimitiveFields {
    fn visit_item_struct(&mut self, item: &syn::ItemStruct) {
        if !matches!(item.vis, syn::Visibility::Public(_)) {
            return;
        }
        for field in &item.fields {
            let (Some(name), syn::Type::Path(ty)) = (&field.ident, &field.ty) else { continue };
            let last = ty.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
            if smells_like_domain_value(&name.to_string()) && PRIMITIVES.contains(&last.as_str()) {
                self.found.push(format!("{}.{name}: {last}", item.ident));
            }
        }
    }
}

/// Primitive-obsession heuristic: public struct fields that look like domain
/// values (ids, status, method, url, path, token) but are typed as primitives.
///
/// # Errors
/// If the source does not parse.
pub fn primitive_obsession(source: &str) -> Result<Vec<String>, syn::Error> {
    let file = syn::parse_file(source)?;
    let mut v = PrimitiveFields { found: Vec::new() };
    v.visit_file(&file);
    Ok(v.found)
}
