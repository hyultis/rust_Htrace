# Runtime and diagnostics

## Flush, shutdown and delivery

- `flush()` inserts a barrier and waits for earlier commands and their module
  flush hooks. Concurrent producers may continue; their later messages are
  outside that barrier.
- `shutdown()` closes admission, drains accepted traces, flushes modules and
  notifies living modules through `event_onGlobalExit()`. Repeated shutdowns
  observe the same completion. Shutdown before initialization is also terminal.
- Both return `Result<(), ControlError>` and default to a five-second timeout.
  `flush_timeout(Duration)` / `shutdown_timeout(Duration)` accept another budget.

An accepted trace is one successfully inserted before admission closes. The
worker preserves channel order, not a global timestamp sort. Concurrent calls
have no defined relative order until insertion; timestamps remain those of the
producers. Each producer's insertion order is preserved.

A timeout returns control without cancelling a blocked OS write or module
callback. The worker may finish later; a later shutdown call can observe it.
Success means callbacks and flush hooks completed, not physical disk durability
(`fsync`). File write errors must also be inspected through `File::stats()`.
`Stats::processed` counts dispatched traces, not successful delivery to every sink.

There is no implicit restart after shutdown, including through another call to
`globalContext_set()`. Ordinary traces before initialization or after closure
are ignored without panic. Direct rejected submissions are counted; macros
filtered out by `enabled()` never submit a trace. Module callback panics in
unwind builds are isolated, counted and reported as `ControlError::ModuleFailed`
by subsequent barriers (the error remains sticky). The Rust panic hook can
still print a diagnostic. Blocking control calls from a worker callback return
`Reentrant`; recursive traces/context-exit notifications are rejected to prevent
unbounded logging recursion.

Message arguments and their application-provided Debug/Display implementations
remain caller code: a panic in that code is not intercepted.

The queue is **unbounded**. Sustained production faster than output can exhaust
memory. `HTracer::stats()` exposes accepted, processed, queued and rejected traces,
module/worker errors, and backtrace captures/errors. Fields are sampled separately.
Modules awaiting a flush barrier are retained until that barrier. Flush regularly
if scopes create many short-lived output instances. No guarantee is made against
process abort, fatal allocation failure, `panic=abort` or an endlessly blocked module.

## Contexts and spans

Contexts inherit modules, extra fields and thresholds; a threshold of None
inherits its parent. Spans are isolated by the producing thread's identity, even
when threads share a name. `Spaned!` creates a scoped span. These are thread scopes,
not task-local async contexts; a moved guard still removes its original scope.

`Context::threadName_set()` overrides the displayed producer name and file group
within the span installed from that context. The innermost explicit name wins;
an unnamed child inherits its parent. Leaving the span restores the outer name
or the producer's native name (`MAIN` if unnamed). This does not rename the OS
thread or change span isolation. Install the context in each worker scope whose
traces should share the group; spans do not propagate to spawned tasks.

Context clones and trace snapshots never fire exit callbacks. Exiting a span
queues one logical notification after commands already inserted. Dropping an
outer guard out of order removes only that scope; inner guards stay active.
The exit snapshot contains that scope and its remaining ancestors, not children.
Replacing the global context queues an exit for its previous configuration.
This is not a barrier against producers still constructing old snapshots; quiesce
producers and flush first when an application requires a strict reconfiguration
boundary. Keep spans inside the logger's lifetime: exits after shutdown cannot
be delivered. Never-installed contexts have no logical exit notification.

## File output

`FileConfig` supports a shared file, files per source or producer name, and daily
or hourly suffixes. Options can be combined. With all destinations disabled,
no file is written. `byThreadId` selects files by the producer’s captured thread
name, falling back to `MAIN` for unnamed threads. Spaces and parentheses remain
readable (for example, `Async Compute Task Pool (0)_20261104.trc`). Letters and
numbers, including Unicode, and `_` and `-` are also preserved. All other
characters are replaced with `_`, including path separators and `%`. Names
that become identical share a file. Explicit configuration paths remain
application-controlled.

Files use `write_all()` with a cache of at most 16 open handles per module.
There is no userspace BufWriter buffer. Rotation uses each trace's timestamp;
direct concurrent calls to a module serialize whole lines. Inspect `File::stats()`
for directory/open/write/flush errors, written lines and cached handle count.
Stats do not acquire the output lock. A renamed/unlinked file can remain open
until cache eviction or module destruction; external rotation must account for it.
See [file measurements and tradeoffs](https://github.com/hyultis/rust_Htrace/blob/master/benchmarks/file_output.md).

The console similarly records write/flush errors in `CommandLine::writeErrors()`
and reports prior output failures through its flush hook. Invalid Hconfig scalar
types fall back to module defaults instead of panicking.

## Backtraces and validation

Backtraces capture the producer stack once and resolve each frame once, with
safe fallbacks for missing symbols. Full file/line information depends on debug
symbols. For release builds, `debug = "line-tables-only"` in `[profile.release]`
can provide file/line information; path remapping remains available through
Rust's `--remap-path-prefix` option. Missing information may yield partial or
empty output and must not prevent the application from continuing.

Tests use isolated output directories. Run `cargo test --all-features`, and
`cargo test --no-default-features` for the minimal configuration. The full feature
matrix and Clippy are part of validation; formatting uses the configured nightly
rustfmt. See [benchmark protocol and results](https://github.com/hyultis/rust_Htrace/tree/master/benchmarks) for latency,
throughput, memory sampling and the limits of comparisons.

