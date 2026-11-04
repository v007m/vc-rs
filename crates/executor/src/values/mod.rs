//! Run-time values: the run's var definitions with profiles applied, the frame each action reads,
//! the scope a `${var.x}` lookup walks and the renderer that answers references.

mod fake;
mod frame;
mod render;
mod run;
mod scope;

pub use fake::{FakeData, FakeFn};
pub use frame::{Frame, Iteration};
pub use render::Render;
pub use run::{Defined, RunInputs, RunValues};

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
