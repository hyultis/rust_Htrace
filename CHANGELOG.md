# Changelog

All notable changes to this project are documented in this file.  
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [3.0.0] — Unreleased

### Changed
- One MPSC channel and one writer replace per-trace threads; FIFO insertion order replaces per-batch timestamp sorting.
- Explicit flush/shutdown barriers with finite timeouts and observable diagnostics; shutdown closes the singleton permanently.
- `event_onGlobalExit` is sent at shutdown only to the modules of the current global context, never to a replaced one still kept alive.
- Context exits follow logical spans, not temporary clones; threads have isolated span stacks and inherited thresholds.
- Macros filter before evaluation/formatting/backtrace capture and evaluate explicit levels once.
- Backtraces walk the producer stack once on the producer thread without resolving symbols; the worker resolves and filters the frames before dispatch, with safe missing-symbol fallbacks. Modules see the same `OneTrace::backtraces` frames.
- Admission tests the level first and then a lock-free accepting flag; only an emitted trace takes the admission lock (shared read mode; shutdown/start take write mode). Traces filtered by level are no longer counted in `Stats::rejected`.
- Producer and worker counters live on separate cache lines; `Stats::queued` is derived (approximate while producers run).
- File output uses producer names, complete writes and a bounded cache of 16 open handles.
- A span's context is resolved once at span entry (global, ancestors, span) and shared by every trace emitted under it; each producer thread caches its resolved context and threshold, so an unchanged trace takes no lock and merges nothing. A new global context is seen at the producer's next trace. Contexts handed to a span or to the global are not modified afterwards.
- The `tracing` bridge no longer installs an Htrace span per event inside a tracing span: the event uses the producer's resolved context with only its name replaced by the tracing span path (`root::child`, computed once per tracing span), so modules no longer receive `event_onContextExit` after every tracing event.
- The filtered `tracing` bridge layer disables event callsites below `tracing_min_level` (per callsite, for this layer only); spans of every level stay enabled, so span paths and `tracing_filter_span` are unaffected.
- The `log` bridge formats a record once, and only if the trace is admitted.
- Backtraces keep at most 128 raw frames (innermost first) to bound the memory of a queued trace.
- File cleanup filters names before any filesystem call and verifies the directory before each deletion instead of for every entry read; current destinations are resolved at most once per batch, only when needed.
- File output computes the date component and destination paths once per date key (day, or hour) and source/producer; rendering pre-sizes its output and builds message and backtraces in one buffer (identical output).

### Added
- Optional file cleanup through `purgeAfterDays` and `purgeOnStartup`, also available in Hconfig; direct regular `.trc` files only, with cleanup counters and no new dependencies.
- A default `ModuleAbstract::event_onStartup()` hook for initial modules, executed on the writer before traces.
- `enabled`, `flush`, `flush_timeout`, `shutdown`, `shutdown_timeout`, `stats`, `trace_display`, `trace_debug`.
- A default module `flush()` hook.
- File and console error counters, concurrency/lifecycle/macro/file regression tests, feature matrix and reproducible benchmarks.
- `HtraceBridge` and `HtraceBridgeSpanFilterList` implement `Debug`, `PartialEq` and `Eq`.
- `FileConfig::from_hconfig` and `CommandLineConfig::from_hconfig(configName, moduleKey, defaults)` load a module configuration from a loaded Hconfig; an unloaded config is `Errors::CannotFoundConfigNode("configName/moduleKey")`, an unreachable node `Errors::CannotFoundConfigNode("moduleKey")`.

### Fixed
- File cleanup resumes after its directory is deleted and recreated between passes: each pass records the directory it starts with (previously every later pass failed with "trace purge root changed").
- Cleanup uses one canonical namespace for enumeration and deletion, rejects ambiguous root spellings and malformed candidate components, and refuses pathname roots (such as `/` or a drive/share root).
- Span contexts honor `threadName_set()` for displayed names and file grouping, inherit names through unnamed children, and restore the previous name (or `MAIN`) on exit.
- Producers never expose numeric thread IDs; generated filenames preserve readable spaces and parentheses in span thread names.
- `log::set_max_level` is set only once the Htrace logger is installed, to the bridge's `log_min_level` threshold (it was forced to `Trace` even when another logger owned `log`).
- A foreign `log` logger or `tracing` subscriber is reported as an error instead of a warning sent to that foreign consumer.
- `HTrace!` and `HTraceError!` no longer trigger the `semicolon_in_expressions_from_macros` future-incompatibility lint in expression position (match arms, closure bodies).
- `log_consumer` standalone compilation and optional test feature gates; duplicate test execution removed.
- The first `globalContext_set` publishes the context before opening admission: traces sent meanwhile are rejected (and counted) instead of being accepted without modules.
- Context callbacks and destructors run outside manager locks; malformed Hconfig scalar values fall back to defaults.
- Console write errors no longer panic through `println!`.
- A line is written once when several file destinations (`forceInOneFile`, `bySrc`, `byThreadId`) resolve to the same path; paths differing only by case remain distinct.
- Literal `\n`/`\r` sequences (backslash + letter) are no longer treated as line breaks in messages: `C:\new\rapport` is kept verbatim (visible output change); `\r\n` and `\n\r` count as one break.
- Line breaks in `{thread}`, `{context}`, `{file}` and `{extra[key]}` are flattened to a single space; `lineReturn` only applies to `{msg}`.

### Breaking changes
- `FileConfig` has two new fields; struct literals can use `..FileConfig::default()` to keep cleanup disabled. Relative file paths are resolved at module construction.
- The Rust library target is now `htrace`: change imports from `Htrace::` to `htrace::`. The crates.io package remains `Htrace`.
- Rust 1.97 is the minimum supported compiler version.
- `HTraceError!` now borrows its Result in every form instead of consuming its error.
- `HTracer::drop()` is removed; call `HTracer::shutdown()` and handle its result.
- `globalContext_set` returns `Result<(), htrace::Errors>`; its parameters are unchanged (the bridge stays conditional on consumer features). Errors: `WorkerUnavailable` after shutdown (nothing applied) or when the worker cannot start (the context stays applied), `LoggerAlreadySet` / `TracingSubscriberAlreadySet` when another component owns the global `log` logger / `tracing` subscriber (the context is still applied), `BridgeAlreadyInstalled` when a bridge different from the first installed one is passed (nothing applied; one bridge per process).
- `htrace::Errors` is `#[non_exhaustive]`; matches need a wildcard arm.
- Native OS thread names (main, named or anonymous threads) no longer appear in `{thread}` nor select `byThreadId` files: every producer is `MAIN` unless a span sets `threadName_set()`; the global context's thread name is ignored.
- `HtraceBridgeSpanFilterList` gains `AllowAll` (new default) and `DenyAll`. `Allow` and `Deny` now check every ancestor span of an event, including an explicit parent, instead of the current span only; events outside spans always pass. Empty lists lose their special meaning: replace `Allow(vec![])` with `DenyAll` and `Deny(vec![])` with `AllowAll`.
- Shutdown is terminal; context clone destruction no longer emits exit events.
- File handles may survive external rename/unlink until eviction; filenames replace unsafe producer-name characters with `_`. Names that become identical share a file.
- Plain string expressions remain literal; formatted calls use explicit formatting syntax.
- For the `hconfig` feature, `FileConfig::create_from_hconfig` and `CommandLineConfig::create_from_hconfig` return `Result<Self, htrace::Errors>`: a node that is not a JSON object (previously ignored, defaults returned) and, for files, an empty or whitespace-only resulting `path` are `Errors::ModuleConfigError("file" | "command_line", cause)`; the empty path cause downcasts to `Errors::EmptyPath`. Badly typed values still fall back to defaults.
- `OneTrace::filename` is a `Cow<'static, str>` (static for macros and the `tracing` bridge, no per-trace copy); struct literals can use `"file.rs".into()`.
- `HTracer::trace`, `trace_display` and `trace_debug` take `file: impl Into<Cow<'static, str>>` and `backtrace: Option<RawBacktrace>` (pass `None`); `HTracer::backtrace()` returns the hidden internal `RawBacktrace` and is reserved to the macros and bridges.
- `HtraceBridge::default().min_level_backtrace` is `Some(Level::ERROR)` (was `Some(Level::WARNING)`), like the macros.
- `File::new` returns `Result<File, htrace::Errors>` and rejects an empty or whitespace-only `path` with the new `Errors::EmptyPath` variant instead of writing at the filesystem root.
- `OneTrace::context` is an `Arc<Context>` shared by traces of the same spans; getters are reached through `Deref` (`trace.context.name_get()`), struct literals wrap it with `Arc::new(context)` and an owned copy is `Context::clone(&trace.context)`.

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
