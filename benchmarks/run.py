#!/usr/bin/env python3
"""Collect the bounded release protocol and compare its three repetitions."""

import argparse
import csv
import datetime
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shlex
import statistics
import subprocess
import sys
import tomllib


CASES = (("filtered", 1000), ("simple", 1000), ("formatted", 1000), ("backtrace", 20), ("slow", 500))
METRICS = ("p50_ns", "p95_ns", "p99_ns", "emission_calls_s", "drained_traces_s", "drain_ns", "process_peak_rss_kib", "sampled_peak_threads")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_digest(source):
    result = hashlib.sha256()
    for path in sorted((source / "src").rglob("*.rs")):
        result.update(str(path.relative_to(source)).encode())
        result.update(path.read_bytes())
    return result.hexdigest()


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def command_output(command, cwd, env):
    return subprocess.check_output(command, cwd=cwd, env=env, text=True).strip()


def environment(args, source, build, env):
    cpu = "unavailable"
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        cpu = next((line.split(":", 1)[1].strip() for line in cpuinfo.read_text().splitlines() if line.startswith("model name")), cpu)
    baseline_path = args.baseline.with_name(f"{args.baseline.stem}_environment.json")
    baseline = json.loads(baseline_path.read_text()) if baseline_path.exists() else {}
    harness_hash = digest(source / "examples/benchmark.rs")
    return {
        "schema": 1,
        "phase": args.phase,
        "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "source_git_head": command_output(["git", "rev-parse", "HEAD"], source, env),
        "source_note": "Measured working tree, not asserted identical to HEAD; source/manifest/lock digests identify the measured tree.",
        "source_rust_tree_sha256": source_digest(source),
        "manifest_sha256": digest(source / "Cargo.toml"),
        "lock_sha256": digest(source / "Cargo.lock"),
        "harness_sha256": harness_hash,
        "baseline_results": str(args.baseline.resolve()),
        "baseline_harness_sha256": baseline.get("harness_sha256"),
        "harness_exactly_matches_baseline": harness_hash == baseline["harness_sha256"] if "harness_sha256" in baseline else None,
        "harness_note": "Check harness hashes and protocol notes before interpreting comparisons; historical results do not measure later API revisions.",
        "build": shlex.join(build),
        "features": "default",
        "profile": "release; effective settings follow Cargo environment/configuration",
        "rustc": command_output(["rustc", "--version"], source, env),
        "cargo": command_output(["cargo", "--version"], source, env),
        "kernel": platform.release(),
        "architecture": platform.machine(),
        "cpu": cpu,
        "timeout_seconds": args.timeout,
        "warmup_calls": 1,
        "repetitions": 3,
        "instant_overhead_subtracted": False,
        "measurement_note": "Thermal state and background load are not controlled by this script; finish other builds/tests before collecting.",
    }


def lock_difference(source, baseline_source):
    if baseline_source is None or not (baseline_source / "Cargo.lock").exists():
        return {"available": False}
    def packages(path):
        data = tomllib.loads(path.read_text())
        return {(row["name"], row["version"], row.get("source", "local")) for row in data["package"]}
    before = packages(baseline_source / "Cargo.lock")
    after = packages(source / "Cargo.lock")
    return {"available": True, "removed_packages": sorted(before - after), "added_packages": sorted(after - before)}


def collect(args):
    source = args.source.resolve()
    build = ["cargo", "build", "--offline", "--locked", "--release", "--example", "benchmark", "--message-format=json-render-diagnostics"]
    env = os.environ.copy()
    if args.cargo_home is not None:
        env["CARGO_HOME"] = str(args.cargo_home.resolve())
    if args.target_dir is not None:
        env["CARGO_TARGET_DIR"] = str(args.target_dir.resolve())
    metadata = environment(args, source, build, env)
    metadata["lock_difference"] = lock_difference(source, args.baseline_source)
    metadata["cargo_home"] = env.get("CARGO_HOME")
    metadata["cargo_target_dir"] = env.get("CARGO_TARGET_DIR")
    result_path = args.output / f"{args.phase}.json"
    metadata_path = args.output / f"{args.phase}_environment.json"
    save(metadata_path, metadata)
    log_path = args.output / f"{args.phase}_build.log"
    print(f"Building release harness; output: {log_path}", flush=True)
    with log_path.open("w") as log:
        result = subprocess.run(build, cwd=source, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=120)
    if result.returncode:
        raise RuntimeError(f"release build failed: {log_path}")
    binary = None
    for line in log_path.read_text().splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") == "compiler-artifact" and message.get("target", {}).get("name") == "benchmark" and "example" in message["target"].get("kind", []) and message.get("executable"):
            binary = Path(message["executable"])
    if binary is None:
        raise RuntimeError(f"benchmark executable missing from Cargo build output: {log_path}")
    metadata["executable"] = str(binary)
    save(metadata_path, metadata)
    rows = []
    for producers in (1, 4):
        for mode, total in CASES:
            for repeat in range(1, 4):
                command = ["timeout", f"{args.timeout}s", str(binary), mode, str(producers), str(total // producers)]
                row = {"mode": mode, "producers": producers, "repeat": repeat, "phase": args.phase, "command": shlex.join(command), "valid": False}
                try:
                    run = subprocess.run(command, cwd=source, env=env, capture_output=True, text=True, timeout=args.timeout + 5)
                    if run.stdout.strip():
                        row.update(json.loads(run.stdout.strip().splitlines()[-1]))
                    row["exit_code"] = run.returncode
                    if run.stderr:
                        row["stderr"] = run.stderr[-1500:]
                    row["valid"] = bool(row.get("valid")) and run.returncode == 0 and row.get("received") == row.get("expected") and row.get("missing") == 0 and row.get("excess") == 0
                except (subprocess.TimeoutExpired, json.JSONDecodeError) as error:
                    row.update(valid=False, error=str(error))
                rows.append(row)
                save(result_path, rows)
                if not row["valid"]:
                    raise RuntimeError(f"invalid {mode}/{producers} repeat {repeat}; gains must not be inferred")
            print(f"{mode}, {producers} producer(s): 3/3 exact counts", flush=True)
    metadata["completed_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    metadata["source_unchanged_during_measurement"] = (
        metadata["source_rust_tree_sha256"] == source_digest(source)
        and metadata["manifest_sha256"] == digest(source / "Cargo.toml")
        and metadata["lock_sha256"] == digest(source / "Cargo.lock")
        and metadata["harness_sha256"] == digest(source / "examples/benchmark.rs")
    )
    save(metadata_path, metadata)
    if not metadata["source_unchanged_during_measurement"]:
        raise RuntimeError("measured source changed during collection; comparison requires inspection")


def compare(args):
    before = json.loads(args.baseline.read_text())
    after = json.loads(args.after.read_text())
    if len(before) != 30 or len(after) != 30 or not all(row.get("valid") and row.get("exit_code") == 0 and row.get("received") == row.get("expected") and row.get("missing") == 0 and row.get("excess") == 0 for row in before + after):
        raise RuntimeError("comparison requires 30 valid exact-count runs in both phases")
    comparison = []
    for producers in (1, 4):
        for mode, _ in CASES:
            row = {"mode": mode, "producers": producers}
            for phase, records in (("baseline", before), (args.phase, after)):
                group = [record for record in records if record["mode"] == mode and record["producers"] == producers]
                if sorted(record["repeat"] for record in group) != [1, 2, 3]:
                    raise RuntimeError("missing or duplicate repetition")
                for metric in METRICS:
                    values = [record[metric] for record in group]
                    for name, operation in (("min", min), ("median", statistics.median), ("max", max)):
                        row[f"{phase}_{metric}_{name}"] = operation(values)
                row[f"{phase}_received"] = sum(record["received"] for record in group)
                row[f"{phase}_expected"] = sum(record["expected"] for record in group)
            comparison.append(row)
    save(args.output / "comparison.json", comparison)
    stream = io.StringIO()
    writer = csv.DictWriter(stream, fieldnames=list(comparison[0]))
    writer.writeheader()
    writer.writerows(comparison)
    (args.output / "comparison.csv").write_text(stream.getvalue())
    print("Comparison JSON/CSV written; all 60 runs have exact counts.", flush=True)


def main():
    directory = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collect", action="store_true", help="build release and execute; otherwise only compare existing results")
    parser.add_argument("--source", type=Path, default=directory.parent, help="source tree to build (default: repository root)")
    parser.add_argument("--output", type=Path, help="new output directory; must not exist (default: benchmarks/runs/<UTC timestamp>)")
    parser.add_argument("--baseline", type=Path, default=directory / "baseline.json", help="baseline result array (default: historical benchmarks/baseline.json)")
    parser.add_argument("--after", type=Path, help="result array to compare (default: historical benchmarks/after.json; unavailable with --collect)")
    parser.add_argument("--target-dir", type=Path, help="override CARGO_TARGET_DIR; otherwise use Cargo's environment/configuration")
    parser.add_argument("--cargo-home", type=Path, help="override CARGO_HOME; otherwise use Cargo's environment/default")
    parser.add_argument("--baseline-source", type=Path, help="optional baseline source tree for a Cargo.lock package diff")
    parser.add_argument("--phase", default="after", choices=("after",))
    parser.add_argument("--timeout", type=int, default=30, choices=(30,))
    args = parser.parse_args()
    if args.collect and args.after is not None:
        parser.error("--after cannot be used with --collect")
    if args.output is None:
        timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
        args.output = directory / "runs" / timestamp
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    print(f"Output directory: {args.output}", flush=True)
    args.after = args.output / f"{args.phase}.json" if args.collect else args.after or directory / "after.json"
    if args.collect:
        collect(args)
    compare(args)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
        print(f"benchmark failed: {error}", file=sys.stderr)
        sys.exit(1)
