//! Reporters: console, CTRF and `JUnit` XML.
//!
//! Every reporter implements `vc_interfaces::Reporter` and is built by name from the
//! [`ReporterRegistry`] (D43): `spec` and `json` print to the console, `ctrf` writes the
//! canonical `ctrf.json` and `junit` writes `junit.xml`. A reporter gets the program, the plan,
//! where to write ([`Destination`], [`FsDestination`] on disk), whether to colour and a
//! [`Stopwatch`] through one [`ReporterContext`]; [`NoopReporter`] reports nothing.

mod collect;
mod context;
mod ctrf;
mod destination;
mod encode;
mod failure;
mod json;
mod junit;
mod names;
mod noop;
mod registry;
mod spec;
#[cfg(test)]
mod testing;
mod text;
mod xml;

pub use context::{Color, ReporterContext, Stopwatch, SystemStopwatch};
pub use destination::{Destination, FsDestination};
pub use noop::NoopReporter;
pub use registry::{ReporterFactory, ReporterRegistry};
