//! The demo service of the `vc init` sample: `/users`, a small bounded store, and `/echo`; the
//! test routes `/slow`, `/flaky`, `/status/{code}` and `/poll/{id}` (D329), and the event stream
//! `/events` (D426), and in the binary the WebSocket `/ws` (D465); sign-in at `/auth/token`, `/auth/me` and `/oauth/token`, and the users API's
//! specs under `/openapi` (D331). The `vc-demo` binary serves it on localhost; the docs
//! playground calls [`router`] in process, with no socket (D293).

mod auth;
mod events;
mod openapi;
mod routes;
mod scripted;
mod users;
#[cfg(not(target_family = "wasm"))]
mod ws;
#[cfg(not(target_family = "wasm"))]
mod ws_chat;

#[cfg(test)]
mod testing;

pub use routes::router;
