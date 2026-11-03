//! Runs a Plan through the interfaces: frames, lifecycle, events and the redactor.
//!
//! So far: the request pipeline (D141), `tower` services around the [`vc_interfaces::Transport`]
//! built by [`pipeline`]: timing, retry ([`HttpRetryPolicy`], D143), auth and `beforeRequest` hooks
//! ([`RequestStage`], null objects until M3 and M4), the per-host limit ([`HostLimits`]) and the
//! per-attempt timeout.

mod pipeline;

pub use pipeline::{
    Attempted, BoxFuture, Call, Delivery, Hop, HostLimit, HostLimits, HttpRetryPolicy, NoRetry, Pipeline,
    PipelineParts, RequestStage, Retried, Retry, RetryOn, RetryPolicy, Stage, Timeout, Timing, Transmit, Unchanged,
    pipeline,
};
