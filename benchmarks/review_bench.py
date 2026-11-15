#!/usr/bin/env python3
"""Bounded review benchmark: modes x {1,8} producers x 3 repetitions, median per cell.

Usage: review_bench.py REPO OUT_JSON
Builds the benchmark example per feature set, runs each case in its own process
under a 30 s timeout, and writes raw runs + medians.
"""
import json, statistics, subprocess, sys, datetime, platform
from pathlib import Path

repo = Path(sys.argv[1]); out = Path(sys.argv[2])
REPS = 3
# (mode, traces_per_producer, cargo feature args)
CASES = [
    ("filtered", 2000, []), ("simple", 2000, []), ("formatted", 2000, []),
    ("spans3", 2000, []), ("span_per_trace", 2000, []), ("backtrace", 40, []),
    ("render", 2000, []), ("file", 2000, []),
    ("log", 2000, ["--features", "log_consumer"]),
    ("tracing", 2000, ["--features", "tracing_consumer"]),
]
PRODUCERS = [1, 8]
KEYS = ("p50_ns", "p95_ns", "p99_ns", "emission_calls_s", "drained_traces_s", "drain_ns")

def run(cmd, **kw):
    return subprocess.run(cmd, cwd=repo, capture_output=True, text=True, **kw)

results = {"schema": 1, "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
           "git_head": run(["git", "rev-parse", "HEAD"]).stdout.strip(),
           "rustc": run(["rustc", "-V"]).stdout.strip(), "machine": platform.machine(),
           "reps": REPS, "cases": []}
built = set()
for mode, per, feats in CASES:
    key = tuple(feats)
    if key not in built:
        b = run(["cargo", "build", "--locked", "--release", "--example", "benchmark", *feats])
        if b.returncode != 0:
            print(b.stderr[-2000:]); sys.exit(1)
        built.add(key)
    for prod in PRODUCERS:
        # keep total calls within harness caps: ordinary 20000, backtrace 320
        if mode == "backtrace":
            n = min(per, 320 // prod)
        else:
            n = min(per, 20000 // prod)
        runs = []
        for _ in range(REPS):
            r = run(["timeout", "30s", "cargo", "run", "-q", "--locked", "--release", "--example", "benchmark", *feats,
                     "--", mode, str(prod), str(n)])
            try:
                data = json.loads(r.stdout.strip().splitlines()[-1])
            except Exception:
                data = {"error": r.stderr[-500:]}
            data["exit"] = r.returncode
            runs.append(data)
        ok = [d for d in runs if d.get("exit") == 0 and "error" not in d]
        med = {k: statistics.median(d[k] for d in ok) for k in KEYS if ok and all(k in d for d in ok)}
        results["cases"].append({"mode": mode, "producers": prod, "traces_per_producer": n,
                                 "valid_runs": len(ok), "median": med, "runs": runs})
        print(f"{mode:15} p={prod} n={n} valid={len(ok)}/{REPS} p50={med.get('p50_ns')} calls/s={med.get('emission_calls_s')} drained/s={med.get('drained_traces_s')}", flush=True)
out.write_text(json.dumps(results, indent=2) + "\n")
