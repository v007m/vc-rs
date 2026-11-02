"""Collect raw measurements into results/rust_feasibility.json and print the Part D table."""
import csv
import json
import re
import statistics
from pathlib import Path

ROOT = Path("/home/claude/vibranium-spike/rust-feasibility")
RAW = ROOT / "raw"
OUT = Path("/home/claude/vibranium-spike/results/rust_feasibility.json")


def load(name):
    return json.loads((RAW / name).read_text())


def bench(prefix):
    out = {}
    for f in sorted(RAW.glob(f"{prefix}_bench_*.json")):
        d = json.loads(f.read_text())
        r = d["result"]
        out[f.stem.replace(f"{prefix}_bench_", "")] = {
            "marshal": d["marshal"], "iterations": r["iterations"], "runs": r["runs"],
            "before_request_ns_median": round(r["before_request_ns_median"]),
            "before_request_ns_runs": [round(x) for x in r["before_request_ns_runs"]],
            "custom_check_ns_median": round(r["check_ns_median"]),
            "custom_check_ns_runs": [round(x) for x in r["check_ns_runs"]],
        }
    return out


def hyperfine():
    d = load("parta_startup_hyperfine.json")
    return {r["name"]: {"wall_ms_median": round(r["summary"]["time_wall_clock"]["median"] * 1e3, 2),
                        "wall_ms_mean": round(r["summary"]["time_wall_clock"]["mean"] * 1e3, 2),
                        "peak_rss_mb_median": round(r["summary"]["memory_peak_resident"]["median"] / 2**20, 1),
                        "runs": r["summary"]["time_wall_clock"]["count"]} for r in d["results"]}


def cold_builds():
    out = {}
    for line in (RAW / "parta_cold_builds.txt").read_text().splitlines():
        m = re.match(r"(\S+) release_cold_s=([\d.]+) rc=(\d+)", line)
        if m:
            out[m.group(1)] = {"release_cold_s": round(float(m.group(2)), 1), "exit": int(m.group(3))}
    return out


def part_d():
    rows = list(csv.DictReader((RAW / "partd_build_loop.csv").open()))
    steps = {}
    for r in rows:
        steps.setdefault(r["step"], []).append((float(r["seconds"]), int(r["exit"])))
    return {k: {"median_s": round(statistics.median(s for s, _ in v), 2), "runs_s": [round(s, 2) for s, _ in v],
                "exits": [e for _, e in v]} for k, v in steps.items()}


def mutants(dirname):
    p = RAW / dirname / "mutants.out" / "outcomes.json"
    if not p.exists():
        return None
    d = json.loads(p.read_text())
    keys = ["total_mutants", "caught", "missed", "unviable", "timeout", "success"]
    return {k: d.get(k) for k in keys}


sizes = {n: (ROOT / "parta/target/release" / n).stat().st_size for n in ["hello", "tsstrip", "hook-qjs", "hook-deno"]}
result = {
    "machine": {"vcpu": 2, "ram_gb": 7.8, "rustc": "1.97.0", "linker": "LLD 22.1.6 (rustc default)", "cargo_build_jobs": 2},
    "calibration": {"serde_json_clone_of_test_response_us": 8, "serde_json_roundtrip_of_test_response_us": "26-32",
                    "note": "allocation-heavy work is slow on this VM; mimalloc halves it"},
    "part_a": {
        "binary_bytes_stripped_release": sizes,
        "added_over_hello_bytes": {"quickjs": sizes["hook-qjs"] - sizes["hello"], "v8_deno_core": sizes["hook-deno"] - sizes["hello"],
                                    "oxc_only": sizes["tsstrip"] - sizes["hello"]},
        "cold_release_build": cold_builds(),
        "startup_to_first_hook_call": hyperfine(),
        "in_process_first_call": {"quickjs": load("qjs_first_call.json")["result"], "v8_deno_core": load("deno_first_call.json")["result"]},
        "per_call": {"quickjs": bench("qjs"), "v8_deno_core": bench("deno")},
        "peak_rss_mb_20k_call_bench": {"quickjs": 11.0, "v8_deno_core": 46.6},
        "safety": {"quickjs": load("qjs_safety.json")["result"], "v8_deno_core": load("deno_safety.json")["result"]},
        "errors": {"quickjs": load("qjs_errors.json")["result"], "v8_deno_core": load("deno_errors.json")["result"]},
        "deno_core_builds": True,
    },
    "part_b": {
        "singleflight_proof": (RAW / "partb_singleflight_proof.txt").read_text().strip().splitlines(),
        "singleflight_negative_control": (RAW / "partb_singleflight_negative_control.txt").read_text().strip().splitlines(),
        "code_metrics": load("partb_code_metrics.json"),
        "borrow_checker_errors": 0,
        "first_check_compile_errors": 0,
        "first_test_run": "13/13 passed",
    },
    "part_c": {
        "clippy_rounds_to_green": 5,
        "clippy_code_findings_fixed": 15,
        "clippy_config_conflict_findings": 30,
        "smell_injection_findings": (RAW / "partc_smell_injection.txt").read_text().strip().splitlines(),
        "jscpd_partb": json.loads((RAW / "jscpd/jscpd-report.json").read_text())["statistics"]["total"],
        "jscpd_control_parta_plus_partb": json.loads((RAW / "jscpd-control/jscpd-report.json").read_text())["statistics"]["total"],
        "arch_negative_control": (RAW / "partc_arch_negative.txt").read_text().strip().splitlines(),
        "cargo_deny": "advisories ok, bans ok, licenses ok, sources ok; warnings: duplicate hashbrown, syn",
        "cargo_machete": "no unused dependencies",
        "cargo_mutants_engine_before": mutants("."),
        "cargo_mutants_engine_after": mutants("mutants-after"),
    },
    "part_d": part_d(),
}
OUT.write_text(json.dumps(result, indent=2))
print(f"wrote {OUT}")
labels = {
    "cold_debug_build": "cold `cargo build` (debug, empty target dir)",
    "cold_release_build": "cold `cargo build --release` (thin LTO, empty target dir)",
    "incr_check": "incremental `cargo check` after editing `engine/src/scheduler.rs`",
    "incr_debug_build": "incremental debug `cargo build` after the same edit",
    "incr_test_build_and_run": "`cargo test --workspace` after the edit (build + run all 17 tests incl. mock-server e2e)",
    "incr_test_engine_only": "`cargo test -p engine` after the edit",
    "incr_release_build": "incremental `cargo build --release` after the edit",
}
print("| step | median (s) | runs (s) |\n|---|---|---|")
for k, v in result["part_d"].items():
    print(f"| {labels.get(k, k)} | **{v['median_s']}** | {', '.join(str(x) for x in v['runs_s'])} |")
