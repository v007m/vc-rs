//! Run-time values: the run's var definitions with profiles applied, the frame each action reads,
//! the scope a `${var.x}` lookup walks and the renderer that answers references.

mod bodies;
mod draw;
mod frame;
mod labels;
mod lookup;
mod reads;
mod render;
mod rows;
mod run;
mod scope;
mod unread;

pub use bodies::{BodyFiles, Unread};
pub use draw::{RandomFor, draw_rows, row_seed};
pub use frame::{Frame, Iteration};
pub use labels::row_names;
pub use render::{Around, Render};
pub use rows::example_rows;
pub use run::{Defined, ExampleRows, RunInputs, RunValues};
pub use unread::{UnreadVar, unread_var};

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
