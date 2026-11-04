//! The keys tables of the Syntax Reference, generated from the shape declarations, so the docs
//! describe exactly what the shape-check accepts.

use vc_model::FileKind;

use crate::shapes::{self, Key, Shape};

/// The keys tables of a file kind as markdown: one section per shape it reaches, the file first.
#[must_use]
pub fn key_tables(kind: FileKind) -> String {
    shapes::reachable(shapes::root(kind)).into_iter().map(section).collect::<Vec<_>>().join("\n")
}

fn section(shape: &Shape) -> String {
    format!("### `{}`\n\n{}.\n\n{}", shape.title, shape.doc, table(shape))
}

/// One shape's keys: `| Key | Type | Default | Purpose |`.
fn table(shape: &Shape) -> String {
    let rows: String = shape.all_keys().map(row).collect();
    format!("| Key | Type | Default | Purpose |\n| --- | --- | --- | --- |\n{rows}")
}

fn row(key: &Key) -> String {
    let ty = key.ty.describe();
    let ty = if key.required { format!("{ty}, required") } else { ty };
    let default = key.default.unwrap_or_default();
    format!("| `{}` | {} | {} | {} |\n", key.name, cell(&ty), cell(default), cell(key.doc))
}

/// Text safe inside a table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use vc_model::FileKind;

    use super::{cell, key_tables, row, table};
    use crate::shapes::{Shape, Ty, key};

    #[test]
    fn renders_one_row_per_key() {
        const KEYS: &[crate::shapes::Key] = &[
            key("id", Ty::Id).required().doc("camelCase"),
            key("method", Ty::Enum(&["GET", "POST"])).default("GET").doc("HTTP method"),
        ];
        let shape = Shape::new("entry", "An entry", KEYS);
        let expected = "| Key | Type | Default | Purpose |\n| --- | --- | --- | --- |\n\
            | `id` | ID, required |  | camelCase |\n\
            | `method` | one of GET, POST | GET | HTTP method |\n";
        assert_eq!(table(&shape), expected);
    }

    #[test]
    fn escapes_pipes_in_cells() {
        assert_eq!(cell("a | b"), "a \\| b");
        let row = row(&key("x", Ty::Text).doc("`a|b`"));
        assert_eq!(row, "| `x` | text |  | `a\\|b` |\n");
    }

    #[test]
    fn covers_every_shape_a_file_reaches() {
        let api = key_tables(FileKind::Api);
        assert!(api.starts_with("### `api`\n\nAn api file, `services/<service>/<service>.api.yaml`.\n\n| Key |"));
        assert!(api.contains("### `apiEntry`") && api.contains("### `expect`"));
        assert!(api.contains("| `path` | template, required |  | Appended to `baseUrl` |\n"));
    }
}
