//! The demo service of the `vc init` sample: `/users` and `/echo`. The `vc-demo` binary serves it
//! on localhost; the docs playground calls [`router`] in process, with no socket (D293).

mod routes;

pub use routes::router;
