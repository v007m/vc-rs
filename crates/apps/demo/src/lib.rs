//! The demo service of the `vc init` sample: `/users` and `/echo`, and the test routes `/slow`,
//! `/flaky`, `/status/{code}` and `/poll/{id}` (D329). The `vc-demo` binary serves it on localhost;
//! the docs playground calls [`router`] in process, with no socket (D293).

mod routes;
mod scripted;

pub use routes::router;
