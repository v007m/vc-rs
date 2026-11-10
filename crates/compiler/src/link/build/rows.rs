//! Dataset rows in the program (D173): the `datasets` table, and the `examples` or `forEach`
//! that name a dataset or `$file` the compile was given, lowered to literal rows.

use serde_json::Value as Json;
use vc_model::{Dataset, DatasetId, Loop, Part, Template, TemplateValue};
use vc_parser::Node;

use super::{Build, Built, read};
use crate::link::project::Source;
use crate::schema;

impl Build<'_, '_> {
    /// Every dataset whose file was given, with its rows as JSON.
    pub fn datasets(&mut self) -> Built<()> {
        let data = self.data;
        for (id, file, rows) in data.loaded() {
            let id = DatasetId::new(self.sym(id)?);
            let rows = rows.items().iter().map(schema::to_json).collect();
            self.builder.dataset(Dataset { id, file: file.to_owned(), rows });
        }
        Ok(())
    }

    /// The rows `node` names, as literal values, when the compile was given them.
    pub fn rows(&self, source: &Source<'_>, node: Option<&Node>) -> Option<Vec<TemplateValue>> {
        let rows = self.data.rows_at(source, node?)?;
        Some(rows.items().iter().map(|row| literal(&schema::to_json(row))).collect())
    }

    /// `forEach` or `repeat`; a `forEach` over known rows loops over those rows.
    pub fn looping(&self, source: &Source<'_>, node: &Node) -> Option<Loop> {
        match (read::looping(source, node)?, self.rows(source, node.get("forEach"))) {
            (Loop::ForEach { parallel, .. }, Some(rows)) => {
                Some(Loop::ForEach { items: TemplateValue::List(rows), parallel })
            }
            (looped, _) => Some(looped),
        }
    }
}

/// A JSON value as a template value with no references: a data file's text is never a template.
fn literal(value: &Json) -> TemplateValue {
    match value {
        Json::Null => TemplateValue::Null,
        Json::Bool(flag) => TemplateValue::Bool(*flag),
        Json::Number(number) => TemplateValue::Number(number.clone()),
        Json::String(text) if text.is_empty() => TemplateValue::String(Template { parts: Vec::new() }),
        Json::String(text) => TemplateValue::String(Template { parts: vec![Part::Literal(text.clone())] }),
        Json::Array(items) => TemplateValue::List(items.iter().map(literal).collect()),
        Json::Object(map) => TemplateValue::Map(map.iter().map(|(key, item)| (key.clone(), literal(item))).collect()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_model::{Part, Template, TemplateValue};

    use super::literal;

    #[test]
    fn data_becomes_literal_values_of_the_same_shape() {
        let text = |text: &str| TemplateValue::String(Template { parts: vec![Part::Literal(text.to_owned())] });
        let row = literal(&json!({"city": "${var.x}", "n": 2, "ok": true, "none": null, "tags": ["a", ""]}));
        let TemplateValue::Map(row) = row else { panic!("{row:?}") };
        assert_eq!(row["city"], text("${var.x}"));
        assert_eq!(row["n"], TemplateValue::Number(2.into()));
        assert_eq!((&row["ok"], &row["none"]), (&TemplateValue::Bool(true), &TemplateValue::Null));
        let empty = TemplateValue::String(Template { parts: Vec::new() });
        assert_eq!(row["tags"], TemplateValue::List(vec![text("a"), empty]));
    }
}
