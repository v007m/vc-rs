//! `*.load.yaml`.

use super::types;
use super::types::{Shape, Ty, Unit};

/// Metrics a load threshold or `abortOn` names.
const METRICS: &[&str] = &["p50", "p90", "p95", "p99", "max", "errorRate", "rps"];

shape! {
    load_file = Shape::new("load", "A load file: existing tests as a traffic mix", &[
        types::key("id", Ty::Text).default("path-derived").doc("Scenario ID; must match the path"),
        types::key("name", Ty::Text).doc("Shown in the load report"),
        types::key("mix", Ty::List(&Ty::Obj(mix))).required().doc("The tests and their weights"),
        types::key("stages", Ty::List(&Ty::Obj(stage))).required().doc("Users ramp from the previous stage's count"),
        types::key("thinkTime", Ty::Either(&[Ty::Duration, Ty::Obj(think_time)])).default("0s")
            .doc("Pause between iterations"),
        types::key("checks", Ty::Enum(&["api", "full"])).default("api").doc("`full` evaluates every test's `expect`"),
        types::key("thresholds", Ty::Obj(thresholds)).doc("Matchers on load metrics"),
        types::key("abortOn", Ty::List(&Ty::Enum(METRICS))).default("[]").doc("Stop early when one of these is breached"),
    ])
}

shape! {
    mix = Shape::new("mixEntry", "One test in the traffic mix", &[
        types::key("test", Ty::Ref).required().doc("The test's full ID").example("orders.checkout#placesOrder"),
        types::key("weight", Ty::Int).required().min(1).doc("Relative share of iterations").example("3"),
        types::key("thresholds", Ty::Obj(thresholds)).doc("Thresholds for this test only"),
    ])
}

shape! {
    stage = Shape::new("stage", "One load stage", &[
        types::key("duration", Ty::Duration).required().doc("How long the stage lasts").example("2m"),
        types::key("users", Ty::Int).required().min(0).doc("Users at the end of the stage").example("50"),
    ])
}

shape! {
    think_time = Shape::new("thinkTime", "A random pause", &[
        types::key("min", Ty::Duration).required().doc("Shortest pause"),
        types::key("max", Ty::Duration).required().doc("Longest pause"),
    ])
}

shape! {
    thresholds = Shape::new("thresholds", "Matchers on load metrics", &[
        types::key("p50", Ty::Matcher(Unit::Duration)).doc("Median latency").example("{lt: 200ms}"),
        types::key("p90", Ty::Matcher(Unit::Duration)).doc("90th percentile latency"),
        types::key("p95", Ty::Matcher(Unit::Duration)).doc("95th percentile latency"),
        types::key("p99", Ty::Matcher(Unit::Duration)).doc("99th percentile latency"),
        types::key("max", Ty::Matcher(Unit::Duration)).doc("Slowest request"),
        types::key("errorRate", Ty::Matcher(Unit::Rate)).doc("Share of failed iterations").example("{lt: 1%}"),
        types::key("rps", Ty::Matcher(Unit::Plain)).doc("Requests per second").example("{gte: 100}"),
    ])
}
