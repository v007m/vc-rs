//! V2315: a literal value in the `body.variables` of a `graphql` api does not fit the type its
//! operation declares for the variable, by the schema its file's `graphql.schema` names (D347):
//! `limit: "3"` for an `Int`. A value with a template, a `$cel` or an `$extend` in it is known
//! only at run time, and is left alone. Each document is parsed once against each schema.

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Entry, GraphqlOperations, Node, Value};

use super::graphql::{self, Document};
use super::graphql_schemas::schema;
use super::graphql_variables::given;
use super::{Lint, LintCx};
use crate::graphql::Origin;
use crate::value::{CEL_KEY, EXTEND_KEY};

/// Each document's operations, parsed against a schema, by the document and the schema's path.
type Parsed<'c> = HashMap<(Origin, &'c str), GraphqlOperations>;

pub struct GraphqlValues;

impl Lint for GraphqlValues {
    fn code(&self) -> Code {
        Code::V2315
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let mut parsed = Parsed::new();
        graphql::documents(cx).iter().flat_map(|document| misfits((cx, &mut parsed), document)).collect()
    }
}

/// V2315 for each literal value of `document`'s `body.variables` that does not fit its type,
/// with the document parsed once against its schema into `parsed`.
fn misfits<'c>((cx, parsed): (&'c LintCx<'_, '_>, &mut Parsed<'c>), document: &Document<'_>) -> Vec<Diagnostic> {
    let (Some((operation, given)), Some((path, schema))) = (given(document), schema(cx, document)) else {
        return Vec::new();
    };
    let literal: Vec<&Entry> = given.iter().filter(|entry| !dynamic(&entry.value)).collect();
    let values: Vec<(&str, &Node)> = literal.iter().map(|entry| (entry.key.as_str(), &entry.value)).collect();
    let key = (document.written.origin, path);
    let operations = parsed.entry(key).or_insert_with(|| schema.operations(document.written.text));
    let found = schema.check_variables((operations, operation.name.as_deref()), &values);
    found.into_iter().filter_map(|(index, why)| Some(misfit(literal.get(index)?, &why))).collect()
}

/// Whether `node` is known only at run time: a template, a `$cel` or an `$extend` anywhere in it.
fn dynamic(node: &Node) -> bool {
    match &node.value {
        Value::Scalar(scalar) => scalar.text.contains("${"),
        Value::Seq(items) => items.iter().any(dynamic),
        Value::Map(entries) => entries.iter().any(|entry| {
            entry.key.contains("${") || entry.key == CEL_KEY || entry.key == EXTEND_KEY || dynamic(&entry.value)
        }),
    }
}

/// V2315 at the value of `entry`, which does not fit its variable's type, for the reason `why`.
fn misfit(entry: &Entry, why: &str) -> Diagnostic {
    let message = format!("the value of `${}` does not fit the type the operation declares: {why}", entry.key);
    let diagnostic = Diagnostic::new(Code::V2315, entry.value.span, message).label("not of the variable's type");
    diagnostic.help(
        "write the value as its type reads it in JSON: a number, `true` or `false` unquoted, an enum value as its name",
    )
}
