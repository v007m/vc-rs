"""Rerun of the load, compile and CEL benchmarks with equal connection limits and Node 26.

Every client is capped at CONNS concurrent connections/in-flight requests:
  Bun: BUN_CONFIG_MAX_HTTP_REQUESTS env var; Node: undici Agent connections;
  Go: http.Transport MaxConnsPerHost; Rust: semaphore around reqwest.
Usage: rerun_fair.py NODE26_BIN [runs]
"""
import json
import os
import statistics
import sys

sys.path.insert(0, os.path.dirname(__file__))
from run_bench import FIX, IMPLS, MOCK_URL, CLIENT_CPU, Mock, result_line, run  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(ROOT, "results", "rerun_fair.json")
CONNS = 256
DURATION = 30


def impls(node26):
    return {
        "ts-bun": IMPLS["ts-bun"],
        "ts-node22": ["node", os.path.join(ROOT, "ts/dist/cli.js")],
        "ts-node26": [node26, os.path.join(ROOT, "ts/dist/cli.js")],
        "go": IMPLS["go"],
        "rust": IMPLS["rust"],
    }


def env_for(name):
    env = dict(os.environ)
    if name == "ts-bun":
        env["BUN_CONFIG_MAX_HTTP_REQUESTS"] = str(CONNS)
    return env


def load(cmds, runs):
    out = {}
    with Mock():
        for vus in (1000, 5000):
            for name, cmd in cmds.items():
                samples = []
                for _ in range(runs):
                    r = run(cmd + ["load", f"{FIX}/load", "--vus", str(vus), "--duration", str(DURATION),
                                   "--conns", str(CONNS), "--base-url", MOCK_URL],
                            cpus=CLIENT_CPU, timeout=DURATION + 120, env=env_for(name))
                    res = result_line(r["stdout"])
                    if r["rc"] != 0 or not res:
                        print(f"load {name}@{vus} FAILED rc={r['rc']} {r['stderr'][-300:]}", flush=True)
                        continue
                    res["cpu_pct"] = 100 * r["cpu_s"] / (r["wall_ms"] / 1000)
                    res["max_rss_mb"] = r["max_rss_kb"] / 1024
                    res["cpu_ms_per_1k"] = 1000 * r["cpu_s"] / max(res["requests"], 1) * 1000
                    samples.append(res)
                    print(f"load {name:10} vus={vus} rps={res['rps']:.0f} p50={res['p50Ms']:.1f} "
                          f"p99={res['p99Ms']:.1f} err={res['errors']} rss={res['max_rss_mb']:.0f}MB", flush=True)
                if samples:
                    out[f"{name}@{vus}"] = {k: statistics.median(s[k] for s in samples)
                                            for k in ("rps", "p50Ms", "p99Ms", "errors", "cpu_pct", "max_rss_mb", "cpu_ms_per_1k")}
                    out[f"{name}@{vus}"]["runs"] = len(samples)
    return out


def compile_and_cel(cmds, runs):
    out = {}
    for name, cmd in cmds.items():
        walls, rss = [], []
        for _ in range(runs):
            extra = ["--yaml", "lazy"] if name.startswith("ts") else []
            r = run(cmd + ["compile", f"{FIX}/ok"] + extra, cpus=CLIENT_CPU)
            if r["rc"] == 0:
                walls.append(r["wall_ms"])
                rss.append(r["max_rss_kb"] / 1024)
        cels = []
        for _ in range(runs):
            r = run(cmd + ["bench-cel", f"{FIX}/bench-body.json", "--iterations", "1000000"], cpus=CLIENT_CPU, timeout=900)
            res = result_line(r["stdout"])
            if res:
                cels.append(res.get("nsPerOp") or res.get("ns_per_op"))
        out[name] = {"compile_wall_ms": statistics.median(walls) if walls else None,
                     "compile_rss_mb": statistics.median(rss) if rss else None,
                     "cel_ns_per_op": statistics.median(cels) if cels else None}
        print(f"{name:10} compile={out[name]['compile_wall_ms']} cel={out[name]['cel_ns_per_op']}", flush=True)
    return out


if __name__ == "__main__":
    node26 = sys.argv[1]
    runs = int(sys.argv[2]) if len(sys.argv) > 2 else 3
    cmds = impls(node26)
    results = {"conns": CONNS, "duration_s": DURATION, "runs": runs,
               "compile_cel": compile_and_cel({k: v for k, v in cmds.items() if k.startswith("ts-node")}, 5),
               "load": load(cmds, runs)}
    with open(OUT, "w") as f:
        json.dump(results, f, indent=2, sort_keys=True)
    print("wrote", OUT)
