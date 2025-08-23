# HTrace

A tracing library.

HTrace aims to generate simple, human-readable tracing lines across different modules.  
It can be used in real-time applications, scripts, APIs, or websites.  
It works as a service (singleton).

**HTrace provides two default modules:**

* **File**: writes traces into files (by day, by hour, by thread name, by source file, etc.)
* **CommandLine**: writes traces to stdout (with color highlighting)
* you can easily write your own using **ModuleAbstract** trait.

**List of trace levels (in order):**

* `Type::DEBUG` – debugging traces (for development) – LOWEST LEVEL
* `Type::DEBUGERR` – debugging traces for errors
* `Type::NORMAL` – normal traces
* `Type::NOTICE` – important traces for future reference
* `Type::NOTICEDERR` – traces of errors that have already been acknowledged (e.g., notified by mail)
* `Type::WARNING` – traces that should be checked
* `Type::ERROR` – traces of errors or blocking issues
* `Type::FATAL` – traces that lead to a panic – HIGHEST LEVEL

The configuration of HTrace and its modules is stored in the configuration directory (via [Hconfig](https://crates.io/crates/Hconfig)), in the file `Htrace.json`.

---

## Available features

* **hconfig** – load/save a module config from [Hconfig](https://crates.io/crates/Hconfig)
* **default_module** (enabled by default) – enables the default modules (those in `src/modules`)
* **tracing_subscriber** – create and enable a tracing subscriber (set as global)
* **log_consumer** – create and enable a log consumer (set as global)

---

## Backtrace

HTrace displays a backtrace if the trace level is **ERROR** or **FATAL**.  
This uses the [backtrace](https://crates.io/crates/backtrace) crate, which requires debug symbols in your build.

The profile `release` default configuration will only show method names.  
If you want to show file paths or file lines, you need to change the debug configuration.

### How to change improve the backtrace information

inside your `Cargo.toml`: (view more here [Cargo profiles – debug](https://doc.rust-lang.org/cargo/reference/profiles.html#debug))

```toml
[profile.release]
debug = "line-tables-only"
```

`line-tables-only` is the lowest level that provides enough backtrace information for HTrace (file information).
Beware, this will increase the size of your binary.

If you want to hide parts of file paths, you can use the `--remap-path-prefix flag`.
Example inside `<project>/.cargo/config.toml`:

```toml
[build]
rustflags = [
    "--remap-path-prefix=/home/user/myproject=/project", # remap all files from /home/user/myproject/... to /project/...
    "--remap-path-prefix=/home/user/.cargo/registry=/cargo/registry",
    # etc
]
```

### Why does it need file information?

File information is used to hide irrelevant symbols (such as those inside HTrace, or before your main()) and improve the readability of the backtrace.

## Online Documentation

[Master branch](https://github.com/hyultis/rust_Htrace)

## Example

```
fn main() {
    // settings
    let mut global_context = Context::default();
    global_context.module_add("cmd", command_line::CommandLine::new(CommandLineConfig::default()));
    global_context.module_add("file", file::File::new(FileConfig::default()));
    global_context.level_setMin(Some(Level::DEBUG));
    HTracer::globalContext_set(global_context);

    // simple trace of variable
    let string_test = "machin".to_string();
    HTrace!(string_test);

    // trace with auto format
    HTrace!("test macro {}", 87);

    // trace with newlines
    HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
    
    // trace with a different span
    {
        Spaned!("span test");
        HTrace!("Trace in a span");
    } // span is dropped here

    // traces with different levels (ERROR and above show backtrace)
    HTrace!((Level::NOTICE) "my trace");
    HTrace!((Level::ERROR) 21);
    HTrace!((Level::ERROR) "test macro {}", 87);

    // macro for consuming Result and tracing the error, defaults to ERROR
    // (ERROR and above show backtrace)
    let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
    HTraceError!((Level::FATAL) "File error is : {}", testerror);

	// we need to wait manually that all threads are done
	HTracer::drop();
}
```

you can also check tests.

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
