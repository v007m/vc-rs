//! Alert channels: `webhook`, `slack`, `teams` and `pagerduty`.
//!
//! The [`NotifierRegistry`] is the `vc_interfaces::NotifierFactory` the engine sends alerts
//! through: one [`Payload`] per channel kind, by name, each posting JSON through the run's
//! `Transport`. `webhook` posts a CTRF summary, `slack` a Block Kit message, `teams` an
//! Adaptive Card and `pagerduty` an Events API v2 event keyed `<project>#<profiles>`, resolved
//! on recovery. No error holds a setting's value: webhook URLs and routing keys are secrets.

mod channels;
mod post;
mod registry;
mod text;

pub use registry::{NotifierRegistry, Payload};
