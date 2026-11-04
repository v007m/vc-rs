//! `*.load.yaml`.

use super::types::{Shape, Ty, Unit, key};

/// Metrics a load threshold or `abortOn` names.
const METRICS: &[&str] = &["p50", "p90", "p95", "p99", "max", "errorRate", "rps"];

shape! {
    load_file = Shape::new("load", "A load file: existing tests as a traffic mix", &[
        key("id", Ty::Text).default("path-derived").doc("Scenario ID; must match the path (D139)"),
        key("name", Ty::Text).doc("Shown in the load report"),
        key("mix", Ty::List(&Ty::Obj(mix))).required().doc("The tests and their weights"),
        key("stages", Ty::List(&Ty::Obj(stage))).required().doc("Users ramp from the previous stage's count"),
        key("thinkTime", Ty::Either(&[Ty::Duration, Ty::Obj(think_time)])).default("0s")
            .doc("Pause between iterations"),
        key("checks", Ty::Enum(&["api", "full"])).default("api").doc("`full` evaluates every test's `expect`"),
        key("thresholds", Ty::Obj(thresholds)).doc("Matchers on load metrics"),
        key("abortOn", Ty::List(&Ty::Enum(METRICS))).default("[]").doc("Stop early when one of these is breached"),
    ])
}

shape! {
    mix = Shape::new("mixEntry", "One test in the traffic mix", &[
        key("test", Ty::Ref).required().doc("The test's full ID").example("orders.checkout#placesOrder"),
        key("weight", Ty::Int).required().doc("Relative share of iterations").example("3"),
        key("thresholds", Ty::Obj(thresholds)).doc("Thresholds for this test only"),
    ])
}

shape! {
    stage = Shape::new("stage", "One load stage", &[
        key("duration", Ty::Duration).required().doc("How long the stage lasts").example("2m"),
        key("users", Ty::Int).required().doc("Users at the end of the stage").example("50"),
    ])
}

shape! {
    think_time = Shape::new("thinkTime", "A random pause", &[
        key("min", Ty::Duration).required().doc("Shortest pause"),
        key("max", Ty::Duration).required().doc("Longest pause"),
    ])
}

shape! {
    thresholds = Shape::new("thresholds", "Matchers on load metrics", &[
        key("p50", Ty::Matcher(Unit::Duration)).doc("Median latency").example("{lt: 200ms}"),
        key("p90", Ty::Matcher(Unit::Duration)).doc("90th percentile latency"),
        key("p95", Ty::Matcher(Unit::Duration)).doc("95th percentile latency"),
        key("p99", Ty::Matcher(Unit::Duration)).doc("99th percentile latency"),
        key("max", Ty::Matcher(Unit::Duration)).doc("Slowest request"),
        key("errorRate", Ty::Matcher(Unit::Plain)).doc("Share of failed requests").example("{lt: 0.01}"),
        key("rps", Ty::Matcher(Unit::Plain)).doc("Requests per second").example("{gte: 100}"),
    ])
}
