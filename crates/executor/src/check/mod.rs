//! Checks: a call's merged `expect`, evaluated against its response (syntax reference "Checks
//! and matchers"). Every matcher is an entry in the [`MatcherRegistry`].

mod compare;
mod compose;
mod evaluate;
mod registry;
mod schema;
mod shape;
mod text;

pub use evaluate::{Subject, diff, evaluate, verdict};
pub use registry::{Arguments, Judge, MatcherFn, MatcherRegistry};
pub use schema::Schemas;

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
