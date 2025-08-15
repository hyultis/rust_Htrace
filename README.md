# HTrace

A tracing class.

HTrace, aim to generate simple, human-readable, tracing lines into different modules.
Usable for realtime application, script, api or website.
Work as a service (singleton)

**Htrace contains two default modules: (you can write your own module easily)**

* File : write trace into file (write a file by day or by hour, by thread name, by source file, etc)
* CommandLine : write trace into stdout (with coloration)

**list of trace level : (in order)**

* Type::DEBUG : debugging trace (for developpement) - LOWER LEVEL
* Type::DEBUGERR : debugging trace for a error
* Type::NORMAL : normal trace
* Type::NOTICE : important trace for the remaining of trace
* Type::NOTICEDERR => trace of an error, but who have been noticed to somebody (mail by example)
* Type::WARNING => trace that need to be checked
* Type::ERROR : trace of an error or something that blocks
* Type::FATAL : trace who lead into panic - HIGHER LEVEL

Configuration of htrace and each module is saved into configuration dir (via Hconfig), into "Htrace.json"

## Available features

* hconfig : enable creating a module config from a [Hconfig](https://crates.io/crates/Hconfig)
* default_module (enabled by default) : enable default module (those in modules src/modules dir)
* tracing_subscriber : create and enable a tracing subcriber (set global)
* log_consumer : create and enable a log consumer (set global)

## Online Documentation

[Master branch](https://github.com/hyultis/rust_Htrace)

## Example

```
fn main()
{
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
	HTrace!("test macro {}",87);

	// trace with return line
	HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
	
	// trace with a different span
	{
		Spaned!("span test");
		HTrace!("Trace in a span");
	} // span is drop here

	// trace different level (ERROR level and above show backtrace)
	HTrace!((Level::NOTICE) "my trace");
	HTrace!((Level::ERROR) 21);
	HTrace!((Level::ERROR) "test macro {}",87);

	// macro for consuming Result, and tracing the error, default to ERROR (ERROR level and above show backtrace)
	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!((Level::FATAL) "File error is : {}",testerror);

	// with the default "threading" features, you need to wait all traces are emited before exiting
	sleep(Duration::from_millis(100));
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
