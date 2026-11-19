//! GraphQL schemas (D347, D367): SDL type definitions validated with `apollo-compiler`, and the
//! documents and literal variable values checked against one. No other crate names its types.

use std::ops::Range;
use std::path::Path;

use apollo_compiler::diagnostic::{Diagnostic, ToCliReport};
use apollo_compiler::executable::{DirectiveList, Operation, Selection, SelectionSet, VariableDefinition};
use apollo_compiler::parser::SourceSpan;
use apollo_compiler::request::coerce_variable_values;
use apollo_compiler::response::serde_json_bytes::serde_json::Number;
use apollo_compiler::response::{JsonMap, JsonValue};
use apollo_compiler::schema::{DirectiveDefinition, DirectiveLocation, ExtendedType};
use apollo_compiler::validation::{DiagnosticData, DiagnosticList, Valid};
use apollo_compiler::{ExecutableDocument, Schema, name};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::graphql::GraphqlProblem;
use crate::node::{Node, Value};
use crate::scalar::{Scalar, ScalarKind};

/// The name `apollo-compiler` is given for a schema's text, which tells its spots from others.
const SCHEMA_PATH: &str = "schema.graphql";

/// The name `apollo-compiler` is given for a document's text.
const DOCUMENT_PATH: &str = "document.graphql";

/// How deep selections are searched for the type an unknown field was asked of; past it, no
/// close name is given.
pub const MAX_DEPTH: usize = 64;

/// What `apollo-compiler` calls a field its type does not have.
const UNDEFINED_FIELD: &str = "UndefinedField";

/// What `apollo-compiler` calls two operations of one name, which V2009 always reports.
const NAME_COLLISION: &str = "OperationNameCollision";

/// What `apollo-compiler` calls an anonymous operation beside others, which V2009 reports when
/// no api's `operationName` picks an operation of the document.
const ANONYMOUS: &str = "AmbiguousAnonymousOperation";

/// What `apollo-compiler` calls a fragment no operation uses, which V2009 reports as a document
/// with no operation.
const UNUSED_FRAGMENT: &str = "UnusedFragment";

/// The largest magnitude past which a float is not an `i64`: 2^63.
const I64_BOUND: f64 = 9_223_372_036_854_775_808.0;

/// What `apollo-compiler` calls a field of an object type with no selections.
const NO_SUBSELECTION: &str = "MissingSubselection";

/// A schema that reads and validates: SDL type definitions, or an introspection result written
/// out as SDL ([`schema_from_introspection`](crate::schema_from_introspection)).
#[derive(Clone, Debug)]
pub struct GraphqlSchema {
    sdl: String,
    valid: Valid<Schema>,
}

impl PartialEq for GraphqlSchema {
    fn eq(&self, other: &Self) -> bool {
        self.sdl == other.sdl
    }
}

impl Eq for GraphqlSchema {}

/// Written as its SDL, which is built again when it is read back, and trusted: the compile cache
/// that stores it is sealed and keyed by the build id.
impl Serialize for GraphqlSchema {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.sdl)
    }
}

/// Read back without validating it again: only a schema that validated is written, and a cache
/// another build wrote is not read (D333).
impl<'de> Deserialize<'de> for GraphqlSchema {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let sdl = String::deserialize(deserializer)?;
        let schema = build(&sdl).map_err(|_| D::Error::custom("the SDL is not a schema"))?;
        Ok(Self { sdl, valid: Valid::assume_valid(schema) })
    }
}

/// A document read once against a schema: its operations, and where it does not validate.
#[derive(Debug)]
pub struct GraphqlOperations {
    document: ExecutableDocument,
    errors: Option<DiagnosticList>,
}

/// Where a document does not fit a schema, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GraphqlMismatch {
    /// What `apollo-compiler` says.
    pub message: String,
    /// The bytes of the document at fault; empty when the problem has no place in it.
    pub at: Range<usize>,
    /// For a field its type does not have, the fields the type has; else empty.
    pub fields: Vec<String>,
}

/// Reads `text` as a schema, in one parse: type system definitions only, which validate.
///
/// # Errors
///
/// Each spot where the text does not parse, holds an operation or fragment, or does not
/// validate; a spot is the first line of the part at fault.
pub fn parse_schema(text: &str) -> Result<GraphqlSchema, Vec<GraphqlProblem>> {
    let valid = build(text)?.validate().map_err(|invalid| problems(&invalid.errors, text))?;
    Ok(GraphqlSchema { sdl: text.to_owned(), valid })
}

/// The schema `text` holds, with `@oneOf`, not yet validated.
fn build(text: &str) -> Result<Schema, Vec<GraphqlProblem>> {
    let built = Schema::builder().parse(text, SCHEMA_PATH).build();
    let mut schema = built.map_err(|invalid| problems(&invalid.errors, text))?;
    schema.directive_definitions.entry(name!("oneOf")).or_insert_with(one_of);
    Ok(schema)
}

/// The `@oneOf` directive (GraphQL spec, 2025): built into servers, so SDL printers leave it
/// out, but not into `apollo-compiler` 1.33.
fn one_of() -> apollo_compiler::Node<DirectiveDefinition> {
    let locations = vec![DirectiveLocation::InputObject];
    let definition = DirectiveDefinition {
        description: None,
        name: name!("oneOf"),
        arguments: Vec::new(),
        repeatable: false,
        locations,
    };
    apollo_compiler::Node::new(definition)
}

/// Each of `errors` as a problem in the schema `text`, at the first line of its spot.
fn problems(errors: &DiagnosticList, text: &str) -> Vec<GraphqlProblem> {
    let problem = |found: Diagnostic<'_, DiagnosticData>| {
        let at = spot(&found, SCHEMA_PATH);
        let line = text.get(at.clone()).and_then(|part| part.find('\n')).map_or(at.end, |newline| at.start + newline);
        GraphqlProblem { message: found.error.to_string(), at: at.start..line }
    };
    errors.iter().map(problem).collect()
}

impl GraphqlSchema {
    /// Its SDL.
    #[must_use]
    pub fn sdl(&self) -> &str {
        &self.sdl
    }

    /// Where the document of `operations` does not fit the schema, by the GraphQL spec's
    /// validation rules, less what V2009 reports (D368): two operations of one name; an
    /// anonymous operation beside others when V2009 has `reported` the document; a fragment no
    /// operation uses when the document has no operation. A selection left empty because each
    /// field in it is unknown is reported only at those fields.
    #[must_use]
    pub fn check_document(&self, operations: &GraphqlOperations, reported: bool) -> Vec<GraphqlMismatch> {
        let Some(errors) = &operations.errors else { return Vec::new() };
        let empty = operations.document.operations.is_empty();
        let left_out = |found: &Diagnostic<'_, DiagnosticData>| {
            named(found, NAME_COLLISION)
                || (reported && named(found, ANONYMOUS))
                || (empty && named(found, UNUSED_FRAGMENT))
        };
        let found: Vec<Diagnostic<'_, DiagnosticData>> = errors.iter().filter(|found| !left_out(found)).collect();
        let unknown: Vec<Range<usize>> = found
            .iter()
            .filter(|found| named(found, UNDEFINED_FIELD))
            .map(|found| spot(found, DOCUMENT_PATH))
            .collect();
        let emptied = |found: &Diagnostic<'_, DiagnosticData>| {
            let at = spot(found, DOCUMENT_PATH);
            named(found, NO_SUBSELECTION) && unknown.iter().any(|field| at.start <= field.start && field.end <= at.end)
        };
        found.iter().filter(|found| !emptied(found)).map(|found| self.mismatch(&operations.document, found)).collect()
    }

    /// The document `text`, which parses, read and validated against the schema once, for
    /// [`check_document`](Self::check_document) and [`check_variables`](Self::check_variables).
    #[must_use]
    pub fn operations(&self, text: &str) -> GraphqlOperations {
        match ExecutableDocument::parse_and_validate(&self.valid, text, DOCUMENT_PATH) {
            Ok(valid) => GraphqlOperations { document: valid.into_inner(), errors: None },
            Err(invalid) => GraphqlOperations { document: invalid.partial, errors: Some(invalid.errors) },
        }
    }

    /// The message for each value of `given`, by its index, that does not fit the type the
    /// operation `name` of `operations` declares for its variable. A variable the operation does
    /// not declare, or declares of a type that is no input type of the schema (V2314), and a
    /// value with no JSON form (`.inf`), are left alone.
    #[must_use]
    pub fn check_variables(
        &self,
        (operations, name): (&GraphqlOperations, Option<&str>),
        given: &[(&str, &Node)],
    ) -> Vec<(usize, String)> {
        let Ok(operation) = operations.document.operations.get(name) else { return Vec::new() };
        let mut alone = bare(operation);
        let mut misfits = Vec::new();
        for (index, (key, value)) in given.iter().enumerate() {
            let declared = operation.variables.iter().find(|variable| variable.name == *key);
            misfits.extend(
                declared.and_then(|declared| self.misfit(&mut alone, (declared, value))).map(|why| (index, why)),
            );
        }
        misfits
    }

    /// The mismatch `found` in `document`, with the fields of the type an unknown field was asked of.
    fn mismatch(&self, document: &ExecutableDocument, found: &Diagnostic<'_, DiagnosticData>) -> GraphqlMismatch {
        let at = spot(found, DOCUMENT_PATH);
        let fields =
            if named(found, UNDEFINED_FIELD) { self.fields(enclosing(document, at.start)) } else { Vec::new() };
        GraphqlMismatch { message: found.error.to_string(), at, fields }
    }

    /// The fields of the type `set` selects from, when it has fields.
    fn fields(&self, set: Option<&SelectionSet>) -> Vec<String> {
        let fields = match set.and_then(|set| self.valid.types.get(&set.ty)) {
            Some(ExtendedType::Object(object)) => object.fields.keys(),
            Some(ExtendedType::Interface(interface)) => interface.fields.keys(),
            _ => return Vec::new(),
        };
        fields.map(ToString::to_string).collect()
    }

    /// Why `value` does not fit the type `declared` gives its variable, coerced as the only
    /// variable of `alone`.
    fn misfit(
        &self,
        alone: &mut Operation,
        (declared, value): (&apollo_compiler::Node<VariableDefinition>, &Node),
    ) -> Option<String> {
        self.valid.types.get(declared.ty.inner_named_type()).filter(|ty| ty.is_input_type())?;
        let mut values = JsonMap::new();
        values.insert(declared.name.as_str(), json(value)?);
        alone.variables = vec![declared.clone()];
        coerce_variable_values(&self.valid, alone, &values).err().map(|error| error.message().to_string())
    }
}

/// `operation` with no variables, directives or selections, to coerce one variable against.
fn bare(operation: &Operation) -> Operation {
    Operation {
        operation_type: operation.operation_type,
        name: None,
        variables: Vec::new(),
        directives: DirectiveList::new(),
        selection_set: SelectionSet::new(operation.selection_set.ty.clone()),
    }
}

/// Whether `apollo-compiler` calls `found` `name`.
fn named(found: &Diagnostic<'_, DiagnosticData>, name: &str) -> bool {
    found.error.unstable_error_name() == Some(name)
}

/// The bytes `found` points at in the text apollo was given as `path`; empty when it points
/// elsewhere, or nowhere.
fn spot(found: &Diagnostic<'_, DiagnosticData>, path: &str) -> Range<usize> {
    let ours =
        |span: &SourceSpan| found.sources.get(&span.file_id()).is_some_and(|file| file.path() == Path::new(path));
    found.error.location().filter(ours).map_or(0..0, |span| span.offset()..span.end_offset())
}

/// The selections of `document` that hold byte `at`: the innermost operation, fragment, field
/// or inline fragment around it.
fn enclosing(document: &ExecutableDocument, at: usize) -> Option<&SelectionSet> {
    let operations = document.operations.iter().map(|operation| (operation.location(), &operation.selection_set));
    let fragments = document.fragments.values().map(|fragment| (fragment.location(), &fragment.selection_set));
    let mut set = operations.chain(fragments).find(|(span, _)| covers(*span, at))?.1;
    for _ in 0..MAX_DEPTH {
        let Some(inner) = set.selections.iter().find_map(|selection| inside(selection, at)) else { return Some(set) };
        set = inner;
    }
    None
}

/// The selections of `selection` when it holds byte `at`.
fn inside(selection: &Selection, at: usize) -> Option<&SelectionSet> {
    let (span, set) = match selection {
        Selection::Field(field) => (field.location(), &field.selection_set),
        Selection::InlineFragment(fragment) => (fragment.location(), &fragment.selection_set),
        Selection::FragmentSpread(_) => return None,
    };
    covers(span, at).then_some(set)
}

/// Whether `span` holds byte `at`.
fn covers(span: Option<SourceSpan>, at: usize) -> bool {
    span.is_some_and(|span| span.offset() <= at && at < span.end_offset())
}

/// `node` as JSON; `None` when part of it has no JSON form.
fn json(node: &Node) -> Option<JsonValue> {
    match &node.value {
        Value::Scalar(scalar) => scalar_json(scalar),
        Value::Seq(items) => items.iter().map(json).collect::<Option<Vec<_>>>().map(JsonValue::Array),
        Value::Map(entries) => {
            let fields = entries.iter().map(|entry| Some((entry.key.as_str().into(), json(&entry.value)?)));
            fields.collect::<Option<JsonMap>>().map(JsonValue::Object)
        }
    }
}

/// `scalar` as JSON, by the kind YAML reads it as.
fn scalar_json(scalar: &Scalar) -> Option<JsonValue> {
    let text = scalar.text.as_str();
    match scalar.kind {
        ScalarKind::Null => Some(JsonValue::Null),
        ScalarKind::Bool => Some(JsonValue::Bool(text.eq_ignore_ascii_case("true"))),
        ScalarKind::Int => text.parse::<i64>().ok().map(JsonValue::from),
        ScalarKind::Float => text.parse::<f64>().ok().and_then(float_json),
        ScalarKind::String => Some(JsonValue::from(text)),
    }
}

/// `value` as JSON: an integral one an `i64` holds as an integer, as a JSON reader gives `10.0`
/// to an `Int` (D370); `None` for one with no JSON form.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the float is integral and inside the i64 range, so the cast is exact"
)]
fn float_json(value: f64) -> Option<JsonValue> {
    if value.fract() == 0.0 && (-I64_BOUND..I64_BOUND).contains(&value) {
        return Some(JsonValue::from(value as i64));
    }
    Number::from_f64(value).map(JsonValue::Number)
}

#[cfg(test)]
#[path = "graphql_schema_tests.rs"]
mod tests;
