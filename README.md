# HTrace

A Rust tracing library for readable logs with low caller latency. Traces carry
an emission timestamp and context; one background worker writes them to the
configured outputs.

## Installation

Requires Rust 1.97 or later.

```toml
[dependencies]
Htrace = "3"
```

The crates.io package name is **`Htrace`** (capital H), while the Rust library
name is **`htrace`** (lowercase): use `htrace::...` in your code.
To also use a lowercase dependency key in `Cargo.toml`, write
`htrace = { package = "Htrace", version = "3" }` instead.

## Example

With the default features:

```rust
use htrace::{HTrace, HTraceError, Spaned};
use htrace::components::{context::Context, level::Level};
use htrace::htracer::HTracer;
use htrace::modules::{command_line::CommandLine, command_line_config::CommandLineConfig};

fn main() -> Result<(), htrace::htracer::ControlError> {
    let mut context = Context::default();
    context.module_add("console", CommandLine::new(CommandLineConfig::default()));
    context.level_setMin(Some(Level::DEBUG));
    HTracer::globalContext_set(context);

    HTrace!("started");
    HTrace!("count = {}", 42);
    {
        Spaned!("request");
        HTrace!((Level::NOTICE) "handling request");
    }

    let result: Result<(), String> = Err("missing input".into());
    HTraceError!((Level::WARNING) "request error: {}", result);
    assert!(result.is_err());

    HTracer::shutdown()
}
```

Example terminal output (levels and messages are colored):

```text
18:31:44.587416      (MAIN, src/main.rs:l12) : started
18:31:44.587427      (MAIN, src/main.rs:l13) : count = 42
18:31:44.587438 NOTI (MAIN, request, src/main.rs:l16) : handling request
18:31:44.587446 WARN (MAIN, src/main.rs:l20) : request error: missing input
```

## Messages and contexts

- `HTrace!(value)` logs a string or a `Debug + Any` value at NORMAL.
- `HTrace!("value = {}", value)` formats a message, including borrowed or Display-only values.
- `HTrace!((Level::WARNING) "message")` selects a level.
- `HTraceError!(result)` logs an error by reference at ERROR, leaving the Result usable.
  It accepts a level and format string as shown above; without a format string it uses Debug.
- `Spaned!("name")` names a scope. Pass a `Context` to customize its threshold,
  outputs or extra fields. Contexts inherit from their parent and are isolated per thread.

Disabled traces skip message evaluation, formatting and backtrace capture.
`HTraceError!` always evaluates the Result expression once, even when filtered.
A single string expression is literal: `HTrace!("x = {x}")` prints the braces;
use `HTrace!("x = {}", x)` or `HTrace!("x = {x}",)` for interpolation.

Levels, in ascending order: **DEBUG, DEBUGERR, NORMAL, NOTICE, NOTICEDERR,
WARNING, ERROR, FATAL**. Native ERROR and FATAL traces include a backtrace;
FATAL does not itself panic. File/line details require debug symbols, such as
`debug = "line-tables-only"` in `[profile.release]`.

## Customizing log lines

Set `lineFormat` on `CommandLineConfig` or `FileConfig` before creating the output.
Using the imports from the example, a custom console setup looks like this:

```rust
let config = CommandLineConfig {
    lineFormat: "{time} {lvl} {context:> | }{extra[request_id]:> | }{msg}".into(),
    lineReturn: "  > ".into(),
    ..Default::default()
};
let mut context = Context::default();
context.name_set("checkout");
context.extra_set("request_id", "req-42");
context.module_add("console", CommandLine::new(config));
HTracer::globalContext_set(context);

HTrace!((Level::NOTICE) "Payment accepted");
```

```text
18:36:27.439415 NOTI checkout | req-42 | Payment accepted
```

Available fields: `{time}`, `{lvl}`, `{msg}`, `{thread}`, `{context}`, `{file}`,
`{line}` and `{extra[key]}`. Extra fields are set with `Context::extra_set()`
and can also be attached to a scoped context through `Spaned!`.

`{context:> | }` appends ` | ` only when the context name is nonempty;
`{context:<scope: }` adds a conditional prefix. `lineReturn` prefixes continuation
lines in multiline messages and backtraces. Console colors are configured per
level through `CommandLineConfig::colors`; an empty map disables coloring.

## Outputs and features

`CommandLine` writes colored stdout. `File` writes logs by date/hour, producer
name or source file, or to a shared file. Both have customizable formats.
Implement `ModuleAbstract` to add an output.

| Feature | Purpose |
| --- | --- |
| `default_module` (default) | Console and file outputs |
| `hconfig` | Load/save module configuration with Hconfig; enables default modules |
| `log_consumer` | Receive events from `log` |
| `tracing_consumer` | Receive `tracing` events and span names, with span filtering |

Initialize Htrace once at the application's entry point. With either consumer
feature, pass a bridge as the second argument:
`HTracer::globalContext_set(context, htrace::crates::bridge::HtraceBridge::default())`.

## Finishing and diagnostics

Call **`HTracer::shutdown()`** before exiting to drain accepted traces and stop
the worker. The logger cannot restart after shutdown. **`HTracer::flush()`**
waits for previously submitted traces while keeping the logger running—for
example, before reading a log file during execution.

Both return a Result and wait up to five seconds by default; `flush_timeout`
and `shutdown_timeout` accept a `Duration`. A timeout does not cancel a blocked
write, and flushing does not guarantee physical disk durability.

The queue is unbounded: sustained slow output can grow memory usage.
`HTracer::stats()`, `File::stats()` and `CommandLine::writeErrors()` expose errors
and activity without logging recursively.

See [runtime details](docs/runtime.md), [benchmarks](https://github.com/hyultis/rust_Htrace/tree/master/benchmarks)
and the [changelog](CHANGELOG.md) for more information.

## Development

Since version 3.0, HTrace is developed using agentic engineering: AI coding
agents assist with implementation, testing and documentation under the
maintainer's direction and review.

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
