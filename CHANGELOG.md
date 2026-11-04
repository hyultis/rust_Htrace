# Changelog

All notable changes to this project are documented in this file.  
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0] — Unreleased

### Changed
- One MPSC channel and one writer replace per-trace threads; FIFO insertion order replaces per-batch timestamp sorting.
- Explicit flush/shutdown barriers with finite timeouts and observable diagnostics; shutdown closes the singleton permanently.
- Context exits follow logical spans, not temporary clones; threads have isolated span stacks and inherited thresholds.
- Macros filter before evaluation/formatting/backtrace capture and evaluate explicit levels once.
- Backtraces walk and resolve the producer stack once with safe missing-symbol fallbacks.
- File output uses producer names, complete writes and a bounded cache of 16 open handles.

### Added
- `enabled`, `flush`, `flush_timeout`, `shutdown`, `shutdown_timeout`, `stats`, `trace_display`, `trace_debug`.
- A default module `flush()` hook.
- File and console error counters, concurrency/lifecycle/macro/file regression tests, feature matrix and reproducible benchmarks.

### Fixed
- `log_consumer` standalone compilation and optional test feature gates; duplicate test execution removed.
- Context callbacks and destructors run outside manager locks; malformed Hconfig scalar values fall back to defaults.
- Console write errors no longer panic through `println!`.

### Breaking changes
- The Rust library target is now `htrace`: change imports from `Htrace::` to `htrace::`. The crates.io package remains `Htrace`.
- Rust 1.97 is the minimum supported compiler version.
- `HTraceError!` now borrows its Result in every form instead of consuming its error.
- `HTracer::drop()` is removed; call `HTracer::shutdown()` and handle its result.
- The conditional `globalContext_set` signature is unchanged.
- Shutdown is terminal; context clone destruction no longer emits exit events.
- File handles may survive external rename/unlink until eviction; filenames encode unsafe producer-name bytes.
- Plain string expressions remain literal; formatted calls use explicit formatting syntax.

## [2.0.1] — 2025-10-06
minor release.

### Added
- for "hconfig" feature, `create_from_hconfig` parameter initialization now takes a default parameter.

### Fixed
- Removed parasite logging

## [2.0.0] — 2025-09-26
Major release introducing breaking changes, new features, and internal refactors.

### Added
- File and CommandLine are now modules as **Default modules** (via `default_module` feature):
	- **File** module: write traces to files (rotation by day/hour, per thread, per source file, etc.).
	- **CommandLine** module: write traces to stdout with colors.
- **Optional integrations**
	- `tracing_subscriber` feature: global subscriber integration with `tracing`, including **span filtering** and **context propagation**.
	- `log_consumer` feature: global consumer integration with the `log` crate.
- **Macros**
	- `HTrace!` — trace messages/values.
	- `HTraceError!` — consume `Result` while logging errors.
	- `Span!` and `Spaned!` — span-based tracing, with context managed via a `ThreadManager`.
- **Span system**
	- New span mechanic allows defining a context for subsequent traces (thread-aware).
- **Context system**
	- New context mechanic allows defining module or specific content.
- **Formatter improvements**
	- Customizable formatters with editable templates.
	- Extra metadata available (thread name, context, etc.).
- **Config management**
	- Hconfig dependency is now optionnal via `hconfig` feature.
- **Backtrace**
	- Improved symbol resolution.
	- Filtering of internal and `/rustc/` frames for clarity.
	- Renamed `Backtrace` to `Hbacktrace`.
- **Tests**
	- Expanded test coverage (including span,tracing, log scenarios).

### Changed
- **Type**: renamed/normalized into "Level" inside "component".
- **Configuration**: Now use a global context.
- **Default features** in `Cargo.toml` revised.
- **Module organization**: files/structs reorganized, legacy names dropped, many method name revamped.

### Removed
- **Legacy APIs**: non-global configuration and obsolete level schemes (breaking compatibility with 1.x).
- **namedThread! macro** (superseded by span mechanics).
- Dependencies: removed `chrono` (replaced by `time`).

### Fixed / Optimized
- Multiple performance optimizations in backtrace and tracing paths.
- Reduced stack noise in backtrace outputs by filtering unnecessary frames.

---

[2.0.0]: https://github.com/hyultis/rust_Htrace/releases/tag/2.0.0
