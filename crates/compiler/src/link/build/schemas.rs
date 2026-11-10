//! The JSON Schemas `body.schema` checks name (D201), by the key each check holds: those written
//! inline, and the documents of the schema files the compile was given. A file not given is
//! left out, and its check fails at run time.

use super::Build;
use crate::link::body_schema;
use crate::schema;

impl Build<'_, '_> {
    /// Adds every schema a `body.schema` names that the program can hold.
    pub fn schemas(&mut self) {
        let (project, data) = (self.project, self.data);
        for source in &project.sources {
            for node in body_schema::nodes(source.root) {
                if body_schema::is_inline(node) {
                    self.builder.schema(body_schema::inline(source, node), schema::to_json(node));
                    continue;
                }
                let Some(path) = body_schema::path(source, node) else { continue };
                if let Some(document) = data.file(&path) {
                    self.builder.schema(path, schema::to_json(document));
                }
            }
        }
    }
}
