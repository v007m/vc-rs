#!/usr/bin/env python3
"""Print markdown tables from results/raw.json (pasted into REPORT.md)."""
import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
raw = json.load(open(os.path.join(ROOT, "results", "raw.json")))


def table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    out += ["| " + " | ".join(str(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


print("### Compile (1,000 files)\n")
rows = []
for name, v in raw["compile"].items():
    rows.append([name, f"{v['median_wall_ms']:.0f}", f"{v['median_internal_ms']:.0f}", f"{v['median_max_rss_mb']:.0f}", f"{v['median_cpu_s']:.2f}",
                 f"{min(r['wall_ms'] for r in v['runs']):.0f}–{max(r['wall_ms'] for r in v['runs']):.0f}"])
for name, v in raw.get("compile_variants_extra", {}).items():
    rows.append([name, f"{v['median_wall_ms']:.0f}", f"{v['median_internal_ms']:.0f}", f"{v['median_max_rss_mb']:.0f}", f"{v['median_cpu_s']:.2f}",
                 f"{min(r['wall_ms'] for r in v['runs']):.0f}–{max(r['wall_ms'] for r in v['runs']):.0f}"])
print(table(["variant", "wall ms (median)", "in-process ms", "peak RSS MB", "CPU s", "wall range"], rows))

print("\n### CEL (1,000,000 evals, compiled once)\n")
print(table(["impl", "median ns/op", "runs"], [[k, f"{v['median_ns_per_op']:.0f}", ", ".join(f"{x:.0f}" for x in v["runs_ns_per_op"])] for k, v in raw["cel"].items()]))

print("\n### Load\n")
rows = []
for k, v in raw.get("load", {}).items():
    if "rps" not in v:
        rows.append([k, "FAILED", "", "", "", "", "", ""])
        continue
    n = len([r for r in v["runs"] if not r.get("failed")])
    rows.append([k, f"{v['rps']:.0f}", f"{v['p50Ms']:.1f}", f"{v['p99Ms']:.1f}", f"{v['errors']:.0f}", f"{v['cpu_pct']:.0f}", f"{v['max_rss_mb']:.0f}",
                 f"{v['cpu_ms_per_1k_req']:.1f}", n])
print(table(["impl@VUs", "req/s", "p50 ms", "p99 ms", "errors", "CPU %", "peak RSS MB", "CPU ms / 1k req", "runs"], rows))

print("\n### Startup (--version)\n")
print(table(["impl", "median ms", "stddev ms", "peak RSS MB", "artifact MB"],
            [[k, f"{v['median_ms']:.1f}", f"{v['stddev_ms']:.1f}", f"{v.get('median_peak_rss_mb', 0):.0f}", f"{v['artifact_bytes'] / 2**20:.1f}"] for k, v in raw["startup"].items()]))

print("\n### LOC\n")
print(table(["impl", "code", "comments", "blank", "files"], [[k, v["code"], v["comments"], v["blanks"], v["files"]] for k, v in raw["loc"].items()]))

if "build" in raw:
    print("\n### Build\n")
    for lang, d in raw["build"].items():
        for k, v in d.items():
            print(f"- {lang}: {k} = {v:.2f}")

print("\n### Proof\n")
print(table(["impl", "pass", "file-scope creates (50 tests)", "run-scope creates (20 tests)", "test-scope creates (repeat 5)", "repeat requests", "full suite"],
            [[k, v["pass"], v["file_scoped_given_creates"], v["run_scoped_given_creates"], v["test_scoped_given_creates_for_repeat5"], v["repeat5_requests"],
              f"{v['full_suite']['summary']} ({v['full_suite']['requests']} req)"] for k, v in raw["proof"].items()]))
