//! Load mode: N virtual users loop over create -> get for a fixed duration.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::executor::ApiCaller;
use crate::model::{Api, Program};

/// Log-linear latency histogram in microseconds (~1.5% precision), identical in all three implementations.
#[derive(Clone)]
pub struct Histogram {
    counts: Vec<u64>,
    pub count: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self { counts: vec![0; 128 + 40 * 64], count: 0 }
    }
}

impl Histogram {
    pub fn record(&mut self, us: f64) {
        let v = us.max(0.0) as u64;
        let idx = if v < 128 {
            v as usize
        } else {
            let e = (63 - v.leading_zeros()) as usize - 6;
            128 + (e - 1) * 64 + ((v >> e) as usize - 64)
        };
        self.counts[idx] += 1;
        self.count += 1;
    }

    pub fn merge(&mut self, o: &Histogram) {
        self.counts.iter_mut().zip(&o.counts).for_each(|(a, b)| *a += b);
        self.count += o.count;
    }

    pub fn percentile(&self, p: f64) -> f64 {
        let target = (p / 100.0 * self.count as f64).ceil() as u64;
        let mut seen = 0;
        for (i, &c) in self.counts.iter().enumerate() {
            seen += c;
            if seen >= target && c > 0 {
                if i < 128 {
                    return i as f64;
                }
                let e = (i - 128) / 64 + 1;
                return ((64 + (i - 128) % 64) << e) as f64;
            }
        }
        0.0
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadResult {
    pub vus: usize,
    pub duration_s: f64,
    pub requests: u64,
    pub errors: u64,
    pub rps: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
}

pub async fn run(program: Arc<Program>, caller: Arc<ApiCaller>, vus: usize, duration: Duration) -> Result<LoadResult, String> {
    let api = |id: &str| program.apis.get(id).map(|a| a.def.clone()).ok_or_else(|| format!("load flow needs api {id}"));
    let (create, get) = (Arc::new(api("load.create")?), Arc::new(api("load.get")?));
    let start = Instant::now();
    let deadline = start + duration;

    let handles: Vec<_> = (0..vus)
        .map(|n| {
            let (caller, create, get) = (caller.clone(), create.clone(), get.clone());
            tokio::spawn(async move {
                let mut hist = Histogram::default(); // per-VU, merged at the end
                let mut errors = 0u64;
                let mut i = 0u64;
                while Instant::now() < deadline {
                    let out = timed(&caller, &create, json!({ "name": format!("vu{n}-{i}") }), &mut hist, &mut errors).await;
                    i += 1;
                    if let Some(out) = out
                        && Instant::now() < deadline
                    {
                        let id = out.get("id").cloned().unwrap_or(Value::Null);
                        timed(&caller, &get, json!({ "id": id }), &mut hist, &mut errors).await;
                    }
                }
                (hist, errors)
            })
        })
        .collect();

    let mut total = Histogram::default();
    let mut errors = 0;
    for h in handles {
        let (hist, e) = h.await.map_err(|e| e.to_string())?;
        total.merge(&hist);
        errors += e;
    }
    let elapsed = start.elapsed().as_secs_f64();
    Ok(LoadResult {
        vus,
        duration_s: elapsed,
        requests: total.count,
        errors,
        rps: total.count as f64 / elapsed,
        p50_ms: total.percentile(50.0) / 1000.0,
        p99_ms: total.percentile(99.0) / 1000.0,
    })
}

async fn timed(
    caller: &ApiCaller,
    api: &Api,
    inputs: Value,
    hist: &mut Histogram,
    errors: &mut u64,
) -> Option<std::collections::BTreeMap<String, Value>> {
    let inputs: Map<String, Value> = match inputs {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let t0 = Instant::now();
    let r = caller.call(api, &inputs).await;
    hist.record(t0.elapsed().as_secs_f64() * 1e6);
    match r {
        Ok(r) if api.guards.is_none_or(|g| g.status.matches(r.res.status)) => Some(r.outputs),
        _ => {
            *errors += 1;
            None
        }
    }
}
