//! V2315: a literal value in the `body.variables` of a `graphql` api does not fit the type its
//! operation declares for the variable, by the schema its file's `graphql.schema` names (D347):
//! `limit: "3"` for an `Int`. A value with a template, a `$cel` value or an `$extend` in it is
//! known only at run time, and is left alone. Each document is parsed once against each schema
//! ([`crate::graphql::Documents`]).

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Entry, Node, Value};

use super::graphql::{self, Document};
use super::graphql_schemas::{Checked, schema};
use super::graphql_variables::given;
use super::{Lint, LintCx};
use crate::value::{CEL_KEY, EXTEND_KEY};

pub struct GraphqlValues;

impl Lint for GraphqlValues {
    fn code(&self) -> Code {
        Code::V2315
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        graphql::documents(cx).iter().flat_map(|document| misfits(cx, document)).collect()
    }
}

/// V2315 for each literal value of `document`'s `body.variables` that does not fit its type.
fn misfits(cx: &LintCx<'_, '_>, document: &Document<'_>) -> Vec<Diagnostic> {
    let (Some((operation, given)), Some(Checked { schema, operations, .. })) = (given(document), schema(cx, document))
    else {
        return Vec::new();
    };
    let literal: Vec<&Entry> = given.iter().filter(|entry| !dynamic(&entry.value)).collect();
    let values: Vec<(&str, &Node)> = literal.iter().map(|entry| (entry.key.as_str(), &entry.value)).collect();
    let found = schema.check_variables((operations, operation.name.as_deref()), &values);
    found.into_iter().filter_map(|(index, why)| Some(misfit(literal.get(index)?, &why))).collect()
}

/// Whether `node` is known only at run time, as the value lowering reads it: a template, a map
/// whose only key is `$cel`, or a map with an `$extend` key, anywhere in it. A `$cel` beside
/// other keys is a key like any other.
fn dynamic(node: &Node) -> bool {
    match &node.value {
        Value::Scalar(scalar) => scalar.text.contains("${"),
        Value::Seq(items) => items.iter().any(dynamic),
        Value::Map(entries) => {
            let cel = matches!(entries.as_slice(), [only] if only.key == CEL_KEY);
            cel || entries
                .iter()
                .any(|entry| entry.key.contains("${") || entry.key == EXTEND_KEY || dynamic(&entry.value))
        }
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
