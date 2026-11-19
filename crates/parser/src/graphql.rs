//! GraphQL documents (D346, D356): read with `apollo-parser` for their syntax and operations only.
//! No other crate names its types; validation against a schema comes with D347.

use std::ops::Range;

use apollo_parser::Parser;
use apollo_parser::cst::{Definition, OperationDefinition};

/// What an operation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GraphqlOperation {
    /// Its name; `None` for an anonymous one.
    pub name: Option<String>,
    /// What it does.
    pub kind: OperationKind,
}

/// A document that parses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GraphqlDocument {
    /// Its operations, in order.
    pub operations: Vec<GraphqlOperation>,
}

/// Where a document does not parse, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    let problems: Vec<GraphqlProblem> = tree
        .errors()
        .map(|error| GraphqlProblem {
            message: error.message().to_owned(),
            at: error.index()..error.index().saturating_add(error.data().len()).min(text.len()),
        })
        .collect();
    if !problems.is_empty() {
        return Err(problems);
    }
    let operations = tree.document().definitions().filter_map(|definition| match definition {
        Definition::OperationDefinition(operation) => Some(read(&operation)),
        _ => None,
    });
    Ok(GraphqlDocument { operations: operations.collect() })
}

/// The name and kind of `operation`.
fn read(operation: &OperationDefinition) -> GraphqlOperation {
    let kind = operation.operation_type().map_or(OperationKind::Query, |kind| {
        if kind.mutation_token().is_some() {
            OperationKind::Mutation
        } else if kind.subscription_token().is_some() {
            OperationKind::Subscription
        } else {
            OperationKind::Query
        }
    });
    GraphqlOperation { name: operation.name().map(|name| name.text().to_string()), kind }
}

#[cfg(test)]
mod tests {
    use super::{GraphqlOperation, OperationKind, parse_document};

    fn operation(name: Option<&str>, kind: OperationKind) -> GraphqlOperation {
        GraphqlOperation { name: name.map(str::to_owned), kind }
    }

    #[test]
    fn lists_each_operation_with_its_name_and_kind() {
        let text = "query GetUser { me { id } }\nmutation Rename { rename }\nsubscription OnUser { user }\n{ me }\n\
                    fragment F on User { id }";
        let document = parse_document(text).unwrap();
        let expected = [
            operation(Some("GetUser"), OperationKind::Query),
            operation(Some("Rename"), OperationKind::Mutation),
            operation(Some("OnUser"), OperationKind::Subscription),
            operation(None, OperationKind::Query),
        ];
        assert_eq!(document.operations, expected);
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
