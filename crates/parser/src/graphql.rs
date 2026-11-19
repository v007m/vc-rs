//! GraphQL documents (D346, D356): read with `apollo-parser` for their syntax and operations only.
//! No other crate names its types. Schemas (D347) are in [`crate::graphql_schema`].

use std::ops::Range;

use apollo_parser::cst::{CstNode, Definition, OperationDefinition, Type, VariableDefinition};
use apollo_parser::{Parser, SyntaxNode, SyntaxTree};
use serde::{Deserialize, Serialize};

/// What an operation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationKind {
    /// `query`, or a `{ … }` shorthand.
    Query,
    /// `mutation`.
    Mutation,
    /// `subscription`.
    Subscription,
}

/// One operation a document defines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphqlOperation {
    /// Its name; `None` for an anonymous one.
    pub name: Option<String>,
    /// What it does.
    pub kind: OperationKind,
    /// The variables it declares, in order.
    pub variables: Vec<GraphqlVariable>,
    /// The bytes of its keyword (`query`), or of the `{` that opens a shorthand query.
    pub head: Range<usize>,
}

/// One variable an operation declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphqlVariable {
    /// Its name, without the `$`.
    pub name: String,
    /// Whether a value must be given: its type is non-null and it has no default.
    pub required: bool,
}

/// A document that parses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphqlDocument {
    /// Its operations, in order.
    pub operations: Vec<GraphqlOperation>,
    /// Whether it holds a type system definition or extension, so it may be a schema (D347).
    #[serde(default)]
    pub defines_types: bool,
}

/// Where a document does not parse, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct GraphqlProblem {
    /// What the parser expected, as it says it.
    pub message: String,
    /// The bytes of the document it stopped at; empty at the end of the text.
    pub at: Range<usize>,
}

/// Reads `text` as a GraphQL document.
///
/// # Errors
///
/// Every spot the text does not parse, in order.
pub fn parse_document(text: &str) -> Result<GraphqlDocument, Vec<GraphqlProblem>> {
    let tree = Parser::new(text).parse();
    syntax(&tree, text)?;
    let definitions: Vec<Definition> = tree.document().definitions().collect();
    let defines_types = definitions.iter().any(|definition| {
        !matches!(definition, Definition::OperationDefinition(_) | Definition::FragmentDefinition(_))
    });
    let operations = definitions.iter().filter_map(|definition| match definition {
        Definition::OperationDefinition(operation) => Some(read(operation)),
        _ => None,
    });
    Ok(GraphqlDocument { operations: operations.collect(), defines_types })
}

/// Every spot of `text` that `tree` does not parse.
fn syntax(tree: &SyntaxTree, text: &str) -> Result<(), Vec<GraphqlProblem>> {
    let problems: Vec<GraphqlProblem> = tree
        .errors()
        .map(|error| GraphqlProblem {
            message: error.message().to_owned(),
            at: error.index()..error.index().saturating_add(error.data().len()).min(text.len()),
        })
        .collect();
    if problems.is_empty() { Ok(()) } else { Err(problems) }
}

/// The name, kind, variables and head of `operation`.
fn read(operation: &OperationDefinition) -> GraphqlOperation {
    let keyword = operation.operation_type();
    let kind = keyword.as_ref().map_or(OperationKind::Query, |kind| {
        if kind.mutation_token().is_some() {
            OperationKind::Mutation
        } else if kind.subscription_token().is_some() {
            OperationKind::Subscription
        } else {
            OperationKind::Query
        }
    });
    let head = match (&keyword, operation.selection_set()) {
        (Some(keyword), _) => bytes(keyword.syntax()),
        (None, Some(selections)) => bytes(selections.syntax()).start..bytes(selections.syntax()).start + 1,
        (None, None) => bytes(operation.syntax()),
    };
    let declared = operation.variable_definitions().into_iter().flat_map(|list| list.variable_definitions());
    let variables = declared.filter_map(|definition| variable(&definition)).collect();
    GraphqlOperation { name: operation.name().map(|name| name.text().to_string()), kind, variables, head }
}

/// The name of the variable `definition` declares, and whether it needs a value.
fn variable(definition: &VariableDefinition) -> Option<GraphqlVariable> {
    let name = definition.variable()?.name()?.text().to_string();
    let required = matches!(definition.ty(), Some(Type::NonNullType(_))) && definition.default_value().is_none();
    Some(GraphqlVariable { name, required })
}

/// The bytes `node` spans.
fn bytes(node: &SyntaxNode) -> Range<usize> {
    let range = node.text_range();
    usize::from(range.start())..usize::from(range.end())
}

#[cfg(test)]
mod tests {
    use super::{OperationKind, parse_document};

    #[test]
    fn tells_a_document_that_defines_types() {
        for text in
            ["type T { a: Int }", "query Q { a }\nextend type T { b: Int }", "schema { query: Q }", "scalar Date"]
        {
            assert!(parse_document(text).unwrap().defines_types, "{text}");
        }
        for text in ["query Q { a }", "fragment F on T { a }", "{ a }\nfragment F on T { a }"] {
            assert!(!parse_document(text).unwrap().defines_types, "{text}");
        }
    }

    #[test]
    fn lists_each_operation_with_its_name_and_kind() {
        let text = "query GetUser { me { id } }\nmutation Rename { rename }\nsubscription OnUser { user }\n{ me }\n\
                    fragment F on User { id }";
        let document = parse_document(text).unwrap();
        let found: Vec<_> =
            document.operations.iter().map(|op| (op.name.as_deref(), op.kind, &text[op.head.clone()])).collect();
        let expected = [
            (Some("GetUser"), OperationKind::Query, "query"),
            (Some("Rename"), OperationKind::Mutation, "mutation"),
            (Some("OnUser"), OperationKind::Subscription, "subscription"),
            (None, OperationKind::Query, "{"),
        ];
        assert_eq!(found, expected);
    }

    #[test]
    fn a_variable_needs_a_value_when_it_is_non_null_without_a_default() {
        let text = "query Q($id: ID!, $limit: Int! = 10, $after: String, $tags: [String!]!) { a }";
        let document = parse_document(text).unwrap();
        let found: Vec<_> = document.operations[0].variables.iter().map(|v| (v.name.as_str(), v.required)).collect();
        assert_eq!(found, [("id", true), ("limit", false), ("after", false), ("tags", true)]);
    }

    #[test]
    fn points_at_the_token_the_parser_stopped_at() {
        let problems = parse_document("query { me { id }").unwrap_err();
        assert_eq!(problems.len(), 1);
        assert_eq!((problems[0].message.as_str(), problems[0].at.clone()), ("expected R_CURLY, got EOF", 17..17));
        let problems = parse_document("query { user(id: ) { id } }").unwrap_err();
        assert_eq!(problems[0].at, 17..18);
        assert!(problems[0].message.contains("expected"), "{}", problems[0].message);
    }
}
