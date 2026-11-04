# File output: protocol, decision, and limitations

## Selected behavior

`File` writes each line with `write_all`, under a mutex shared by concurrent
direct calls. The `forceInOneFile`, `bySrc`, and `byThreadId` destinations remain
additive; two destinations configured to use the same file therefore produce
two writes. With no destination enabled, no file operation is performed. The
day/hour suffix comes from `trace.date`, including for a trace that arrives
after more recent traces.

`byThreadId` retains its historical semantics based on the producer's **name**,
retrieved from `trace.context.threadName_get()`. Two producers with the same
name share their destination. The worker's name no longer selects the file.
Producer names are encoded by UTF-8 byte: only ASCII alphanumeric characters,
`_`, and `-` are preserved; all others become `%XX` (including `%`). Thus
`../escape\\name` becomes `%2E%2E%2Fescape%5Cname`, with no separator or `..`
segment and no collisions caused by simple character replacement. As before,
the source name is reduced to the segment before the first dot in the final
path component, then encoded in the same way. `FileConfig.path` and
`forceInOneFile` remain paths explicitly chosen by the caller; they do not
provide a confinement boundary against a hostile configuration.

The LRU cache retains at most **16 handles per File instance**, without adding
a field to `FileConfig`. An old date or destination can be reopened in append
mode after eviction. With an active set of more than 16 destinations, the cache
may open and close a file for every line. Handles are released on eviction or
module destruction. If an external tool renames or replaces an open file,
writes may therefore continue through the old handle until eviction; internal
date-based rotation explicitly selects the path for each trace.

Handles use `std::fs::File` directly. A successful line means that `write_all`
was accepted by the OS; it can already be read without waiting for an
application buffer. There is no `fsync` or guarantee of physical persistence.
`ModuleAbstract::flush()` is implemented and is also called by exit callbacks
and on eviction. For a standard unbuffered `File`, `flush` does not add
synchronization with the physical storage medium.

`File::stats()` returns a public snapshot of:

- `linesWritten`: fully accepted lines, per destination;
- `createErrors`: directory creation errors/missing parent;
- `openErrors`: open errors after parent creation;
- `writeErrors`: `write_all` errors, even if a prefix has already been written;
- `flushErrors`: `flush` errors, including during eviction/callbacks;
- `cachedHandles`: handles currently retained, at most 16.

The counters are atomic, but reading them together does not provide an atomic
snapshot in the presence of concurrent writes. No error is fed back into
Htrace. Callbacks with no error return use these counters; an explicit flush
also returns the first error while still attempting the other handles. A write
error closes the affected handle; the next trace attempts to open it again.
A file may contain the prefix of a failed line: `write_all` is not a
transaction, and the module does not retry the entire line. Direct calls to
the same instance keep each complete line together; no guarantee is claimed
across processes or distinct instances.

## I/O microbenchmark

`examples/file_benchmark.rs` compares three isolated strategies:

- `open`: `create_dir_all`, open in append mode, `write_all`, close for each line;
- `cached`: LRU of 16 direct handles, using the same open path on a cache miss;
- `buffered`: LRU of 16 `BufWriter` instances with the default capacity of
  8 192 bytes, with an explicit flush on eviction and at the end.

No strategy performs `fsync`. Total time includes final flushes and closes.
Verification rereads all files outside the timed section and checks every
byte as well as the total line count. A failure returns a nonzero exit code.
The paths are temporary and exclusive to the process, and only these paths
are deleted.

```bash
cargo build --offline --locked --release --example file_benchmark
for destinations in 4 32; do
    for mode in open cached buffered; do
        for repetition in 1 2 3; do
            timeout 30s cargo run --offline --locked --release \
                --example file_benchmark -- "$mode" 10000 "$destinations"
        done
    done
done
```

Run from the repository root with your existing Cargo environment and cache.
The commands require `timeout` (GNU coreutils). These reruns print results to
stdout; they do not update the retained historical JSON files.

Arguments: mode, total line count (1 to 20 000), destinations (1 to 64).
The recorded workload writes 10 000 identical messages of 111 bytes each,
cycling through 4 or 32 destinations. The OS page cache is active and is not
cleared. Each process creates fresh names. There is no separate warm-up.
The series were run sequentially in the order above, with three repetitions
per case. `file_environment.json` records the command, harness fingerprint,
toolchain, CPU, and kernel; `file_io.json` records the 18 results, commands,
and exit codes.

Results recorded on 2026-11-03: **18/18 valid**, with no timeouts or missing
bytes. Throughput in lines/s, minimum / median / maximum of three repetitions:

| Destinations | Strategy | Minimum | Median | Maximum |
| --- | --- | ---: | ---: | ---: |
| 4 | open per line | 471 787 | 474 944 | 475 727 |
| 4 | direct cache 16 | 5 840 047 | 6 168 940 | 6 174 055 |
| 4 | BufWriter cache 16 | 33 506 338 | 33 654 396 | 38 807 221 |
| 32 | open per line | 469 363 | 470 588 | 478 749 |
| 32 | direct cache 16 | 441 731 | 462 971 | 469 727 |
| 32 | BufWriter cache 16 | 437 812 | 452 224 | 459 464 |

The direct cache was selected: the gain with four active destinations is clear
(≈13× at the median), while retaining an OS write for each line. `BufWriter`
further reduces OS calls for the active set of four files, but introduces a
visibility delay and a different flush contract. That additional change was
not selected here. With 32 destinations in rotation, the cache of 16 provides
no gain, and the measurements show a slight overhead, which is documented
rather than hidden.

These numbers measure only I/O for a small payload on `/tmp` with the OS cache;
they do not estimate sustained storage throughput. The microbenchmark includes
no line formatting, queue, Htrace context, or mutex/statistics from the actual
module. The `Box<dyn Write>` shared by the cached strategies also adds a cost
absent from the `open` path. The measured factor must therefore not be
presented as the logger's overall improvement. A different workload, different
storage, or a larger set of destinations may change the result.

## Targeted checks

```bash
timeout 30s cargo test --offline --locked --test file_output
timeout 30s cargo test --offline --locked --lib modules::file::tests
```

The historical checks passed: additive and disabled destinations, recursive parent
creation, injected dates/days/hours and late arrival, eviction and reopening
in append mode, the 16-handle limit, 400 complete lines from concurrent direct
calls, provenance from two named producers, and names containing
separators/`..`/`%`. Directory and open errors are triggered without `chmod`.
An injected private writer accepts partial writes and then fails: the tests
check the complete line when possible, the prefix on error, and the
write/flush counters.

The singleton scenario is the only one in its test target and checks flush
followed by shutdown with deadlines. The tests use only their temporary tree
and delete none of the maintainer's traces or configuration. Flush errors for
a real OS file cannot be triggered portably, so the test uses an injected
writer. Blocked I/O remains subject to the runtime control timeout and is not
cancelled by the File module.
