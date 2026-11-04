# Htrace benchmark protocol

The current harness, `examples/benchmark.rs`, uses `HTrace!`,
`HTracer::globalContext_set` and `HTracer::shutdown`, with no additional dependencies.
Each run is a separate process with an in-memory counting module and no per-trace
console or disk output. A shutdown failure invalidates the run.

The recorded November 3–4 results predate the API revision: their harness used
`HTracer::drop`. In the measured implementation after the changes, this wrapper
called shutdown. These measurements and their hashes are historical and were
not rewritten after the wrapper was removed. They also predate the library
target rename to `htrace`; the current harness uses lowercase imports.

```bash
cargo build --offline --locked --release --example benchmark
timeout 30s cargo run --offline --locked --release --example benchmark -- simple 4 250
```

Run commands from the repository root. They respect your Cargo environment and
configuration. Offline builds require dependencies already present in your cache.
The runner below requires Python 3.11 or newer; the process timeout uses the
`timeout` command (GNU coreutils).

Arguments are `MODE PRODUCERS TRACES_PER_PRODUCER`. Available modes:

- `filtered`: DEBUG below an ERROR threshold; no callbacks expected.
- `simple`: a literal NORMAL message.
- `formatted`: producer and index interpolated into the message.
- `backtrace`: an ERROR message with capture/resolution through the historical API.
- `slow`: a literal NORMAL message with a callback delayed by a 100 µs `sleep`.

`formatted` measures message formatting at the call site; the counting module
does not render a line with `FormaterCompile`. The historical runs used the normal
release profile without requested debug information (`debug=0`); new runs respect
your Cargo profile configuration. The actual symbol
count and resolution may vary with the toolchain. The consumer delay is a
request to the scheduler, not a guaranteed exact duration.

The workload is bounded: 1–16 producers, with at most 20,000 ordinary calls,
320 backtrace calls or 2,000 slow calls in total. Always retain the external
30-second timeout. A single warm-up call on the main thread waits for its
callback (except when filtered), with a five-second internal deadline.
Producer threads have no individual warm-up.

## Fields and interpretation

Each process writes a JSON object to stdout, then exits with a nonzero status
if the received count differs from the expected count. Check `missing`, `excess`,
`valid` and the exit status before comparing results. Counts cover callbacks
completed when shutdown returns, minus the warm-up callback. This counter does
not verify trace identity or order; dedicated functional tests assert those.

- `p50_ns`, `p95_ns`, `p99_ns`: sorted call latencies, using the lower rank
  `floor((N-1)*p/100)`. Measured with `Instant`, without subtracting its overhead.
  `instant_pair_p50_ns` separately measures 1,000 `now`/`elapsed` pairs.
- `emission_ns`: wall-clock time from barrier release until producer threads
  have been joined, including coordination and joining. `emission_calls_s` is
  the call count divided by this time, not a guaranteed channel capacity.
- `drain_ns`: shutdown duration (`shutdown` in the current harness). `total_ns`
  covers emission and shutdown. `drained_traces_s` is the received count divided
  by `total_ns`, including emission time; it is not throughput during shutdown alone.
- `received_at_emission`: callbacks completed after joining producers, before
  shutdown. `calls - received_at_emission` describes unfinished work in modes
  that do not filter messages out.
- `sampled_peak_threads`: maximum sampled approximately every 1 ms through
  `/proc/self/status`, including the measurement thread and producers.
  Short-lived threads may escape sampling.
- `process_peak_rss_kib`: Linux `VmHWM` for the entire process, not exact queue
  memory or a bound guaranteed by Htrace. Zero when unavailable.
- `progress`: for `slow`, a series of `[time_us, completed_calls,
  completed_callbacks, VmHWM_KiB]` sampled approximately every 1 ms. Counters are
  read separately and do not form an atomic snapshot; their difference shows
  the progression of unfinished work, not exact channel length. Other modes
  return an empty series.

`Instant` measurements, the atomic counter after each call, the counting callback
and the measurement thread all have a cost. These costs were shared by the
historical before/after harnesses.
The small workloads and 20 backtraces do not establish sustained capacity or
long-queue behavior; high percentiles on these small samples have limited precision.

## Repetitions and provenance

For each version, run three processes for each mode/producer-count pair.
The recorded protocol uses 1 and then 4 producers: 1,000 total calls for
`filtered`, `simple` and `formatted`, 20 for `backtrace`, and 500 for `slow`.
Store JSON objects in an array with `repeat`, `phase`, `command`, `exit_code`
and any errors. Do not aggregate a run with missing traces or a timeout as a
valid result. Compare all three repetitions and their spread, without setting
a target gain beforehand. Median emission throughput describes an observed
rate; the maximum is only the best of these small trials.

The baseline was built on 2026-11-03 from the preserved copy at
`/tmp/htrace-baseline.Q8sxFF`, before replacing the transport. Only the harness
was copied into it; historical sources were preserved. The local lockfile
version had been reconciled before that build. Manifest, lockfile, Rust source
tree and harness hashes, Cargo/Rust versions, CPU, kernel and build command are
recorded in `baseline_environment.json`. `source_git_head` provides the contextual
HEAD; the preserved working tree is not claimed to match that commit. No network
dependency resolution was used. Any dependency differences in the new version
must be reported in the comparison.

`baseline.json` contains 30 runs, all completed with exact counts and no timeout.
For 4 producers, the medians of the three repetitions are:

| Mode | Call p50 (ns) | Emission calls/s | Emission + shutdown traces/s | Shutdown (ms) |
| --- | ---: | ---: | ---: | ---: |
| filtered | 781 | 3 054 703 | 0 | 1.05 |
| simple | 12 473 | 71 900 | 66 795 | 1.06 |
| formatted | 12 143 | 71 396 | 66 372 | 1.06 |
| backtrace | 119 294 | 20 797 | 9 927 | 1.05 |
| slow | 12 743 | 82 923 | 6 596 | 69.77 |

## Historical before/after comparison

The **2026-11-04** collection repeats exactly the 30 baseline cases from
2026-11-03: **30/30 valid after runs, 60/60 before + after runs with exact counts**,
no timeout and no excess detected. Filtered runs expect zero callbacks.
The harness has the same SHA256 hash before and after; the profile remains
release with default features, and workloads/repetitions are identical.
Sources, manifest, lockfile and harness remained unchanged during the final
collection. The build uses `--offline --locked`.

The machine, CPU, kernel, Rust and Cargo match the baseline metadata.
The only package removed from the lockfile is `singletonThread 2.1.0`; no package
was added and no external version changed. Exact hashes and this diff are in
`after_environment.json`. Baseline and after runs were collected on two days,
with a session interruption: thermal state and background load were
**not controlled**. No builds or test suites were launched concurrently by the
orchestration during the final measurements. These comparisons remain descriptive,
with only three repetitions per case.

To collect the current implementation, first wait for other tests/builds to finish:

```bash
python3 benchmarks/run.py --collect
```

The Python standard-library script builds offline in release mode with the
lockfile, runs 30 processes under `timeout 30s`, saves each result immediately,
and fails on missing traces, excess traces, a nonzero exit status or a timeout.
It inherits Cargo's environment and configuration; `--cargo-home` and
`--target-dir` are optional overrides. The executable path comes from Cargo's
build output. Results, metadata and the build log go into a new
`benchmarks/runs/<UTC timestamp>/` directory. `--output PATH` selects another
directory, which must not exist. The retained historical files are never
overwritten by these commands.

By default, the new collection is compared with historical `baseline.json`.
Its metadata records the current harness hash and whether it matches the
baseline; a later harness or API revision is not an identical-harness rerun.
Use `--baseline PATH` to select another compatible 30-run baseline and
optionally `--baseline-source PATH` to record a lockfile package diff. If a
sibling `<baseline stem>_environment.json` exists, its harness hash is recorded.
No preserved baseline source tree is assumed to exist.

To regenerate only the historical comparison into a new directory:

```bash
python3 benchmarks/run.py
```

Use `--baseline PATH --after PATH` to compare other stored arrays without
collecting. `--help` lists all options. `comparison.json` and `comparison.csv`
retain the minimum, median and maximum for each metric, mode and producer
count. A comparison requires three valid exact-count repetitions in each phase.
The checked-in `after.json` and comparison files describe the historical
November 3–4 measurements shown below.

In the following tables, **before → after** means the median of three runs,
except in columns explicitly marked “max”. Percentiles are calculated per run
and then aggregated; they are not percentiles of a pooled sample. Minima and
the full spread remain available in the machine-readable files.

### Call latency (ns)

| Mode | Producers | p50 | p95 | p99 |
| --- | ---: | ---: | ---: | ---: |
| filtered | 1 | 141 → 50 | 161 → 60 | 180 → 61 |
| simple | 1 | 10 369 → 652 | 18 144 → 1 573 | 25 398 → 4 428 |
| formatted | 1 | 10 980 → 1 463 | 17 433 → 5 741 | 24 636 → 8 025 |
| backtrace | 1 | 25 107 → 12 233 | 81 723 → 16 370 | 81 723 → 16 370 |
| slow | 1 | 9 648 → 311 | 15 699 → 1 162 | 23 214 → 2 225 |
| filtered | 4 | 781 → 380 | 1 173 → 762 | 1 452 → 1 052 |
| simple | 4 | 12 473 → 1 674 | 63 038 → 6 672 | 958 229 → 14 307 |
| formatted | 4 | 12 143 → 1 793 | 59 071 → 6 562 | 1 288 840 → 15 900 |
| backtrace | 4 | 119 294 → 18 685 | 342 633 → 239 239 | 342 633 → 239 239 |
| slow | 4 | 12 743 → 2 224 | 51 797 → 7 234 | 864 203 → 9 288 |

### Observed throughput (calls/s or traces/s)

“Emission” counts completed calls, including filtered calls. “Drained” counts
expected/received callbacks over the full emission + shutdown duration.
A maximum is the best of three trials, not a guaranteed capacity.

| Mode | Producers | Median emission | Max emission | Median drained | Max drained |
| --- | ---: | ---: | ---: | ---: | ---: |
| filtered | 1 | 3 978 975 → 8 044 146 | 3 984 524 → 8 669 568 | 0 → 0 | 0 → 0 |
| simple | 1 | 87 916 → 1 087 172 | 89 135 → 1 762 779 | 80 464 → 1 029 119 | 81 448 → 1 442 435 |
| formatted | 1 | 82 560 → 462 132 | 92 646 → 1 671 950 | 75 937 → 460 248 | 84 409 → 1 349 761 |
| backtrace | 1 | 25 841 → 42 604 | 30 419 → 44 416 | 10 937 → 41 978 | 11 647 → 43 625 |
| slow | 1 | 93 704 → 1 641 702 | 119 607 → 1 731 470 | 6 591 → 6 653 | 6 614 → 6 661 |
| filtered | 4 | 3 054 704 → 5 154 267 | 6 048 107 → 8 248 307 | 0 → 0 | 0 → 0 |
| simple | 4 | 71 900 → 1 151 875 | 78 053 → 1 517 801 | 66 796 → 1 144 687 | 72 015 → 1 486 538 |
| formatted | 4 | 71 397 → 1 138 889 | 88 352 → 2 557 970 | 66 372 → 1 132 865 | 80 783 → 2 494 680 |
| backtrace | 4 | 20 797 → 39 000 | 28 976 → 41 136 | 9 927 → 38 282 | 11 451 → 40 444 |
| slow | 4 | 82 924 → 1 129 760 | 84 021 → 1 830 476 | 6 596 → 6 662 | 6 624 → 6 666 |

### Process resources

RSS is the total peak `VmHWM` in KiB. Thread counts are sampled peaks for each
run, not the exact maximum number of live threads. The sampler can miss
short-lived producers in very short runs.

| Mode | Producers | Median RSS (KiB) | Max RSS (KiB) | Median threads | Max threads |
| --- | ---: | ---: | ---: | ---: | ---: |
| filtered | 1 | 3 008 → 3 132 | 3 036 → 3 152 | 3 → 4 | 3 → 4 |
| simple | 1 | 3 372 → 3 204 | 3 384 → 3 280 | 21 → 4 | 23 → 4 |
| formatted | 1 | 3 360 → 3 188 | 3 364 → 3 404 | 21 → 4 | 44 → 4 |
| backtrace | 1 | 38 540 → 38 600 | 38 608 → 38 624 | 3 → 4 | 3 → 4 |
| slow | 1 | 3 516 → 3 252 | 3 592 → 3 260 | 12 → 4 | 31 → 4 |
| filtered | 4 | 3 068 → 3 196 | 3 080 → 3 196 | 4 → 6 | 6 → 6 |
| simple | 4 | 4 160 → 3 272 | 4 216 → 3 440 | 292 → 5 | 363 → 6 |
| formatted | 4 | 4 220 → 3 280 | 4 288 → 3 324 | 247 → 7 | 273 → 7 |
| backtrace | 4 | 38 656 → 38 668 | 38 696 → 38 688 | 4 → 6 | 4 → 6 |
| slow | 4 | 3 904 → 3 468 | 3 956 → 3 472 | 149 → 6 | 177 → 6 |

In these small four-producer trials, median simple and formatted emission
throughput is about 16 times the baseline, with lower p99 latencies.
Median sampled thread peaks fall from 292 to 5 and from 247 to 7, respectively.
For filtering and backtraces, the permanent worker can instead increase the
observed thread count. RSS in backtrace runs remains around 38 MiB: these
measurements isolate neither the queue nor symbol-resolution allocations.

The slow consumer still limits drained throughput to about 6.6k traces/s;
faster emission moves the remaining work to final draining. No loss on this
bounded workload does not imply bounded memory under an indefinitely slow
workload. Detailed progress remains in the `progress` fields of `baseline.json`
and `after.json`: separately read counters do not give an exact channel length.
This comparison does not measure file output; its I/O protocol and limitations
are described in [file_output.md](file_output.md).

## Historical test isolation

Each `tests/{formater,trace,log,tracing,hconfig}.rs` scenario is a standalone
Cargo target with at most one test using the singletons. `tests/tests.rs` no
longer includes them, avoiding duplicate execution and a shared terminal
lifecycle. Scenarios using the supplied modules are gated by `default_module`;
bridges and Hconfig are gated by their respective features. Trace/config paths
are process-specific temporary directories; only directories created by the
test are removed.
