#!/usr/bin/env python3
"""Benchmark harness for the Vibranium v2 language spike.

Every child process is launched with os.posix_spawn and reaped with os.wait4,
so peak RSS (ru_maxrss) and CPU time (utime+stime) come from the kernel for that
process only. Wall time uses time.monotonic_ns(). Medians over N runs.

Usage: run_bench.py [section ...]   sections: env compile diag proof cel load startup loc build
"""
import json
import os
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time
import urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
FIX = os.path.join(ROOT, "fixtures")
RESULTS = os.path.join(ROOT, "results", "raw.json")
MOCK_ADDR = "127.0.0.1:18080"
MOCK_URL = f"http://{MOCK_ADDR}"
CLIENT_CPU = "0"
MOCK_CPU = "1"

IMPLS = {
    "ts-bun": [os.path.join(ROOT, "ts/dist/vib")],
    "ts-node": ["node", os.path.join(ROOT, "ts/dist/cli.js")],
    "go": [os.path.join(ROOT, "go/bin/vib")],
    "rust": [os.path.join(ROOT, "rust/target/release/vibranium")],
}


def run(cmd, cpus=None, timeout=600, env=None):
    """Run cmd; return dict(wall_ms, cpu_s, max_rss_kb, rc, stdout, stderr)."""
    if cpus:
        cmd = ["taskset", "-c", cpus] + cmd
    out_r, out_w = os.pipe()
    err_r, err_w = os.pipe()
    fa = os.posix_spawn_file_actions if False else None  # noqa: F841 (kept simple below)
    t0 = time.monotonic_ns()
    pid = os.posix_spawnp(cmd[0], cmd, env or os.environ, file_actions=[
        (os.POSIX_SPAWN_DUP2, out_w, 1), (os.POSIX_SPAWN_DUP2, err_w, 2),
        (os.POSIX_SPAWN_CLOSE, out_r), (os.POSIX_SPAWN_CLOSE, err_r)])
    os.close(out_w)
    os.close(err_w)
    out_chunks, err_chunks = [], []
    import selectors
    sel = selectors.DefaultSelector()
    sel.register(out_r, selectors.EVENT_READ, out_chunks)
    sel.register(err_r, selectors.EVENT_READ, err_chunks)
    open_fds = 2
    deadline = time.monotonic() + timeout
    while open_fds:
        for key, _ in sel.select(timeout=1):
            data = os.read(key.fd, 65536)
            if not data:
                sel.unregister(key.fd)
                os.close(key.fd)
                open_fds -= 1
            else:
                key.data.append(data)
        if time.monotonic() > deadline:
            os.kill(pid, signal.SIGKILL)
    _, status, ru = os.wait4(pid, 0)
    wall_ms = (time.monotonic_ns() - t0) / 1e6
    return {
        "wall_ms": wall_ms,
        "cpu_s": ru.ru_utime + ru.ru_stime,
        "max_rss_kb": ru.ru_maxrss,
        "rc": os.waitstatus_to_exitcode(status),
        "stdout": b"".join(out_chunks).decode(errors="replace"),
        "stderr": b"".join(err_chunks).decode(errors="replace"),
    }


def result_line(out):
    m = re.search(r"^RESULT (.*)$", out, re.M)
    return json.loads(m.group(1)) if m else None


def median(xs):
    return statistics.median(xs) if xs else None


def load_raw():
    if os.path.exists(RESULTS):
        with open(RESULTS) as f:
            return json.load(f)
    return {}


def save_raw(raw):
    os.makedirs(os.path.dirname(RESULTS), exist_ok=True)
    with open(RESULTS, "w") as f:
        json.dump(raw, f, indent=2, sort_keys=True)


class Mock:
    def __init__(self):
        self.proc = None

    def __enter__(self):
        subprocess.run(["pkill", "-f", "mock-server/mock"], check=False)
        time.sleep(0.3)
        self.proc = subprocess.Popen(["taskset", "-c", MOCK_CPU, os.path.join(ROOT, "mock-server/mock"), "-addr", MOCK_ADDR],
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(50):
            try:
                urllib.request.urlopen(MOCK_URL + "/__stats", timeout=1).read()
                return self
            except Exception:
                time.sleep(0.1)
        raise RuntimeError("mock did not start")

    def reset(self):
        urllib.request.urlopen(urllib.request.Request(MOCK_URL + "/__reset", method="POST"), timeout=5).read()

    def stats(self):
        return json.loads(urllib.request.urlopen(MOCK_URL + "/__stats", timeout=5).read())

    def __exit__(self, *a):
        self.proc.terminate()
        self.proc.wait()


def sh(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()


def section_env(raw):
    mem_kb = int(re.search(r"MemTotal:\s+(\d+)", open("/proc/meminfo").read()).group(1))
    raw["environment"] = {
        "nproc": int(sh("nproc")),
        "cpu_model": sh("grep -m1 'model name' /proc/cpuinfo | cut -d: -f2").strip(),
        "ram_mb": mem_kb // 1024,
        "kernel": sh("uname -r"),
        "os": sh(". /etc/os-release && echo $PRETTY_NAME"),
        "versions": {
            "node": sh("node --version"),
            "bun": sh("bun --version"),
            "typescript": sh(f"cd {ROOT}/ts && npx tsc --version"),
            "go": sh("go version"),
            "rustc": sh("rustc --version"),
            "cargo": sh("cargo --version"),
            "python": sh("python3 --version"),
        },
        "libraries": {
            "ts": json.load(open(f"{ROOT}/ts/package.json"))["dependencies"],
            "go": sh(f"cd {ROOT}/go && go list -m -f '{{{{.Path}}}} {{{{.Version}}}}' github.com/google/cel-go github.com/goccy/go-yaml github.com/go-playground/validator/v10 github.com/santhosh-tekuri/jsonschema/v6 golang.org/x/sync 2>/dev/null").splitlines(),
            "rust": sh(f"cd {ROOT}/rust && cargo tree --depth 1 --prefix none 2>/dev/null | sort -u").splitlines(),
        },
        "fixture": {
            "files": int(sh(f"find {FIX}/ok -name '*.yaml' | wc -l")),
            "yaml_lines": int(sh(f"find {FIX}/ok -name '*.yaml' -print0 | xargs -0 cat | wc -l")),
            "bytes": int(sh(f"du -sb {FIX}/ok | cut -f1")),
        },
        "pinning": {"client_cpu": CLIENT_CPU, "mock_cpu": MOCK_CPU, "compile_and_cel": "unpinned (both cores, no mock running)"},
        "date": sh("date -u +%FT%TZ"),
    }


COMPILE_VARIANTS = {
    "ts-bun (lazy spans)": IMPLS["ts-bun"] + ["compile", f"{FIX}/ok", "--yaml", "lazy"],
    "ts-bun (eager spans)": IMPLS["ts-bun"] + ["compile", f"{FIX}/ok", "--yaml", "eager"],
    "ts-node (lazy spans)": IMPLS["ts-node"] + ["compile", f"{FIX}/ok", "--yaml", "lazy"],
    "ts-node (eager spans)": IMPLS["ts-node"] + ["compile", f"{FIX}/ok", "--yaml", "eager"],
    "go": IMPLS["go"] + ["compile", f"{FIX}/ok"],
    "go (GOMAXPROCS=1)": ["env", "GOMAXPROCS=1"] + IMPLS["go"] + ["compile", f"{FIX}/ok"],
    "rust": IMPLS["rust"] + ["compile", f"{FIX}/ok"],
    "rust (1 thread)": ["env", "RAYON_NUM_THREADS=1"] + IMPLS["rust"] + ["compile", f"{FIX}/ok"],
}


def section_compile(raw, n=7):
    out = {}
    for name, cmd in COMPILE_VARIANTS.items():
        run(cmd)  # warm page cache
        runs = []
        for _ in range(n):
            r = run(cmd)
            assert r["rc"] == 0, (name, r["stdout"][-2000:], r["stderr"][-2000:])
            res = result_line(r["stdout"])
            runs.append({"wall_ms": r["wall_ms"], "internal_ms": res["compileMs"], "max_rss_kb": r["max_rss_kb"], "cpu_s": r["cpu_s"]})
        out[name] = {
            "runs": runs,
            "median_wall_ms": median([x["wall_ms"] for x in runs]),
            "median_internal_ms": median([x["internal_ms"] for x in runs]),
            "median_max_rss_mb": median([x["max_rss_kb"] for x in runs]) / 1024,
            "median_cpu_s": median([x["cpu_s"] for x in runs]),
            "counts": {k: v for k, v in res.items() if k != "compileMs"},
        }
        print(f"compile {name:24} wall={out[name]['median_wall_ms']:.0f}ms internal={out[name]['median_internal_ms']:.0f}ms rss={out[name]['median_max_rss_mb']:.0f}MB", flush=True)
    raw["compile"] = out


def section_diag(raw):
    out = {}
    for name in ["ts-bun", "go", "rust"]:
        extra = ["--yaml", "lazy"] if name.startswith("ts") else []
        r = run(IMPLS[name] + ["compile", "../fixtures/broken"] + extra)
        out[name] = {"rc": r["rc"], "output": r["stdout"].strip(), "wall_ms": r["wall_ms"]}
        os.makedirs(f"{ROOT}/results/diagnostics", exist_ok=True)
        with open(f"{ROOT}/results/diagnostics/{name}.txt", "w") as f:
            f.write(r["stdout"])
    raw["diagnostics"] = out


def section_proof(raw):
    out = {}
    with Mock() as mock:
        for name in ["ts-bun", "ts-node", "go", "rust"]:
            mock.reset()
            r = run(IMPLS[name] + ["run", f"{FIX}/proof", "--verbose"], cpus=CLIENT_CPU)
            stats = mock.stats()
            out[name] = {
                "rc": r["rc"],
                "summary": [l for l in r["stdout"].splitlines() if l.startswith("tests:")],
                "mock_counts": stats,
                "file_scoped_given_creates": stats.get("POST /slow/file-seeds", 0),
                "run_scoped_given_creates": stats.get("POST /slow/run-seeds", 0),
                "test_scoped_given_creates_for_repeat5": stats.get("POST /slow/test-seeds", 0),
                "repeat5_requests": stats.get("POST /proof-repeat", 0),
                "pass": r["rc"] == 0 and stats.get("POST /slow/file-seeds") == 1 and stats.get("POST /slow/run-seeds") == 1
                and stats.get("POST /slow/test-seeds") == 5 and stats.get("POST /proof-repeat") == 5,
            }
            # Full 1,000-file suite end to end (bonus sanity check: identical request counts).
            mock.reset()
            extra = ["--yaml", "lazy"] if name.startswith("ts") else []
            r2 = run(IMPLS[name] + ["run", f"{FIX}/ok", "--concurrency", "64"] + extra, cpus=CLIENT_CPU)
            out[name]["full_suite"] = {"rc": r2["rc"], "summary": r2["stdout"].strip().splitlines()[-1], "requests": sum(mock.stats().values()),
                                       "wall_ms": r2["wall_ms"], "max_rss_mb": r2["max_rss_kb"] / 1024}
            print(f"proof {name:8} pass={out[name]['pass']} counts={stats} full={out[name]['full_suite']['summary']}", flush=True)
    raw["proof"] = out


def section_cel(raw, n=5):
    out = {}
    for name in ["ts-bun", "ts-node", "go", "rust"]:
        runs = []
        for _ in range(n):
            r = run(IMPLS[name] + ["bench-cel", f"{FIX}/bench-body.json", "--iterations", "1000000"], cpus=CLIENT_CPU)
            res = result_line(r["stdout"])
            assert res and res["allTrue"], (name, r)
            runs.append(res["nsPerOp"])
        out[name] = {"runs_ns_per_op": runs, "median_ns_per_op": median(runs)}
        print(f"cel {name:8} {median(runs):.0f} ns/op", flush=True)
    raw["cel"] = out


def section_load(raw, n=3, duration=30):
    out = raw.get("load", {})
    plan = [("ts-bun", n), ("go", n), ("rust", n), ("ts-node", 1)]
    for vus in (1000, 5000):
        for name, reps in plan:
            runs = []
            for _ in range(reps):
                with Mock():
                    time.sleep(0.5)
                    r = run(IMPLS[name] + ["load", f"{FIX}/load", "--vus", str(vus), "--duration", str(duration)], cpus=CLIENT_CPU, timeout=duration + 120)
                res = result_line(r["stdout"])
                if not res:
                    runs.append({"failed": True, "rc": r["rc"], "stderr": r["stderr"][-1500:]})
                    print(f"load {name} {vus} FAILED rc={r['rc']} {r['stderr'][-300:]}", flush=True)
                    continue
                res.update({"cpu_pct": 100 * r["cpu_s"] / (r["wall_ms"] / 1000), "max_rss_mb": r["max_rss_kb"] / 1024, "wall_ms": r["wall_ms"],
                            "cpu_ms_per_1k_req": 1000 * r["cpu_s"] / max(1, res["requests"]) * 1000})
                runs.append(res)
                print(f"load {name:8} vus={vus} rps={res['rps']:.0f} p50={res['p50Ms']:.1f} p99={res['p99Ms']:.1f} err={res['errors']} cpu={res['cpu_pct']:.0f}% rss={res['max_rss_mb']:.0f}MB", flush=True)
            ok = [x for x in runs if not x.get("failed")]
            key = f"{name}@{vus}"
            out[key] = {"runs": runs}
            if ok:
                out[key].update({m: median([x[m] for x in ok]) for m in ["rps", "p50Ms", "p99Ms", "errors", "cpu_pct", "max_rss_mb", "cpu_ms_per_1k_req"]})
            raw["load"] = out
            save_raw(raw)
    raw["load"] = out


def section_startup(raw):
    out = {}
    cmds = {k: " ".join(v) + " --version" for k, v in IMPLS.items()}
    exported = "/tmp/claude-0/hyperfine-startup.json"
    args = ["hyperfine", "-N", "--warmup", "5", "--runs", "40", "--export-json", exported] + list(cmds.values())
    subprocess.run(["taskset", "-c", CLIENT_CPU] + args, capture_output=True, text=True, check=True)
    data = json.load(open(exported))
    for (name, _), res in zip(cmds.items(), data["results"]):
        walls = [m["time_wall_clock"]["value"] * 1000 for m in res["measurements"]]
        rss = [m["memory_peak_resident"]["value"] / 2**20 for m in res["measurements"]]
        out[name] = {"median_ms": median(walls), "mean_ms": statistics.mean(walls), "stddev_ms": statistics.stdev(walls), "median_peak_rss_mb": median(rss), "runs": len(walls)}
        print(f"startup {name:8} {out[name]['median_ms']:.1f} ms", flush=True)
    sizes = {"ts-bun": "ts/dist/vib", "ts-node": "ts/dist/cli.js", "go": "go/bin/vib", "rust": "rust/target/release/vibranium"}
    for k, p in sizes.items():
        out[k]["artifact_bytes"] = os.path.getsize(os.path.join(ROOT, p))
    raw["startup"] = out


def section_loc(raw):
    out = {}
    for name, path in {"ts": "ts/src", "go": "go/cmd go/internal", "rust": "rust/src", "generator": "generator", "mock-server": "mock-server/main.go"}.items():
        cmd = f"cd {ROOT} && tokei --output json {path} --exclude '*_test.go'"
        data = json.loads(sh(cmd))
        total = data.get("Total", {})
        out[name] = {"code": total.get("code"), "comments": total.get("comments"), "blanks": total.get("blanks"),
                     "files": sum(len(v.get("reports", [])) for k, v in data.items() if k != "Total")}
    raw["loc"] = out
    print("loc", out, flush=True)


def timed_shell(cmd, cwd):
    t0 = time.monotonic_ns()
    p = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True)
    assert p.returncode == 0, (cmd, p.stdout[-2000:], p.stderr[-2000:])
    return (time.monotonic_ns() - t0) / 1e9


def section_build(raw):
    out = {}
    ts, go, rs = f"{ROOT}/ts", f"{ROOT}/go", f"{ROOT}/rust"
    touch = {"ts": "src/compiler.ts", "go": "internal/compiler/compiler.go", "rust": "src/compiler.rs"}

    def med(fn, k=3):
        return median([fn() for _ in range(k)])

    out["ts"] = {
        "typecheck_s (tsc 7 --noEmit)": med(lambda: timed_shell("npx tsc --noEmit -p .", ts)),
        "bundle_s (bun build --target=node)": med(lambda: timed_shell("bun build src/cli.ts --target=node --outfile dist/cli.js", ts)),
        "compile_binary_s (bun build --compile)": med(lambda: timed_shell("bun build src/cli.ts --compile --bytecode --minify --outfile dist/vib", ts)),
    }
    gocache = "/tmp/claude-0/gocache-cold"
    out["go"] = {
        "cold_build_s (empty GOCACHE, includes deps+std)": timed_shell(f"rm -rf {gocache} && GOCACHE={gocache} go build -o bin/vib ./cmd/vib", go),
        "incremental_build_s (touch compiler.go)": med(lambda: timed_shell(f"touch {touch['go']} && go build -o bin/vib ./cmd/vib", go)),
        "vet_s": med(lambda: timed_shell("go vet ./... 2>/dev/null || true", go)),
    }
    shutil.rmtree(gocache, ignore_errors=True)
    out["rust"] = {
        "cold_release_build_s (cargo clean, includes deps)": timed_shell("cargo clean -q && cargo build --release -q", rs),
        "incremental_release_build_s (touch compiler.rs)": med(lambda: timed_shell(f"touch {touch['rust']} && cargo build --release -q", rs)),
        "incremental_check_s (cargo check, touch)": (timed_shell("cargo check -q", rs), med(lambda: timed_shell(f"touch {touch['rust']} && cargo check -q", rs)))[1],
        "incremental_debug_build_s (touch)": (timed_shell("cargo build -q", rs), med(lambda: timed_shell(f"touch {touch['rust']} && cargo build -q", rs)))[1],
    }
    raw["build"] = out
    print("build", json.dumps(out, indent=1), flush=True)


SECTIONS = {"env": section_env, "compile": section_compile, "diag": section_diag, "proof": section_proof, "cel": section_cel,
            "load": section_load, "startup": section_startup, "loc": section_loc, "build": section_build}

if __name__ == "__main__":
    os.chdir(os.path.join(ROOT, "ts"))  # diagnostics print paths relative to cwd
    raw = load_raw()
    for s in sys.argv[1:] or list(SECTIONS):
        SECTIONS[s](raw)
        save_raw(raw)
