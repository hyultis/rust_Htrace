# File retention validation and measurement

The file module provides optional age-based and startup cleanup without new
dependencies. Both default to disabled. Its directory belongs to HTrace and
must remain stable, together with its parents, during cleanup. This is not a
defense against hostile concurrent filesystem replacements.

## Reproduce the bounded measurement

```sh
cargo build --release --example retention_benchmark
cargo run --release --example retention_benchmark -- off 512 4096
cargo run --release --example retention_benchmark -- age 512 4096
cargo run --release --example retention_benchmark -- startup 512 4096
```

Each process creates its own temporary directory and 512 files whose mtime is
three days old. One producer sends 4,096 fixed messages through HTracer. The
age mode retains one day; startup removes existing `.trc` files before emission.
The example checks exact written/deleted counts, closes the logger, and removes
only its own directory. Arguments cap the workload at 2,000 files and 20,000
traces. Run each mode in a fresh process.

## Local results

Measured on 2026-11-05, Linux x86_64, Rust 1.99.0, release profile. Temporary
storage was **tmpfs**, not a physical disk. These measurements precede the
additional canonical-path confinement checks added during the subsequent audit. Three repetitions per mode, ordered
off/age/startup; values below are medians across those repetitions.

| Mode | Startup + barrier (ms) | Emission through drain (ms) | Producer p50 / p95 / p99 (ns) | Deleted files |
| --- | ---: | ---: | ---: | ---: |
| Off | 0.139 | 6.098 | 370 / 1623 / 2505 | 0 |
| Age | 0.124 | 7.144 | 251 / 891 / 1623 | 512 |
| Startup | 3.039 | 5.084 | 361 / 1613 / 2425 | 512 |

Startup timing includes logger creation and its first flush barrier. Drain
timing includes message emission and final flush, but excludes startup cleanup.
Call samples include timer overhead. Lower producer samples in age mode do
**not** establish a speed improvement: scheduling and contention differ while
the worker is occupied by cleanup. Three short tmpfs runs cannot predict disk,
network-filesystem, sustained-load or tail-latency behavior. There is no previous
implementation baseline because it had no purge.

Periodic passes examine at most 64 entries and delete at most 8 files per write
or module flush. Unit tests check both budgets and hourly scheduling. An active
scan retains a directory iterator, not a vector of every entry. No process-memory
measurement was made. Filesystem calls can block despite these budgets, delaying
the sole writer and growing the unbounded queue. Startup cleanup completes its
pass before messages are processed.

## Checks and limits

Tests cover default/zero behavior, Hconfig validation, exact extensions, age
boundaries, recent appends, future timestamps, non-UTF-8 names on Unix, links,
nested directories, changed-root detection, cache closure, concurrent direct
writes, error/retry behavior, startup without traces, reconfiguration, and two
separate application processes. Existing append tests still pass.

The full suite passes on Linux with Rust 1.97 and 1.99, plus the minimal feature
configuration. Android `aarch64-linux-android` and `armv7-linux-androideabi` pass
`cargo check --lib`; this does not link or execute an Android application.
Windows and Android runtime tests were unavailable in this environment.
Clippy completes with existing warnings. See the
[runtime contract](../docs/runtime.md#file-cleanup) for details.
