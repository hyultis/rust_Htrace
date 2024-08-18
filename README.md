# HTrace

A tracing class.

HTrace, aim to generate simple, human-readable, tracing lines into different modules.
Usable for realtime application, script, api or website.
Work as a service (singleton)

**Htrace contains 2 default modules : (you can write your own module easily )**

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

## Online Documentation

[Master branch](https://github.com/hyultis/rust_Htrace)

## Example

Note: the crate is using [Hconfig](https://crates.io/crates/Hconfig) for configuration

```
fn main()
{
	// configuration path, the directory need to be existing or created before continuing
	HConfigManager::singleton().setConfPath("./config");
	
	// Adding modules into Htrace, default configuration is used if there is no configuration file, or missing part.
	HTracer::appendModule("cmd", CommandLine::CommandLine::new(CommandLineConfig::default())).expect("Cannot append module");
	HTracer::appendModule("file", File::File::new(FileConfig::default())).expect("Cannot append module");
	
	// settings
	HTracer::threadSetName("testThreadName"); // default thread, can be call for each thread
	HTracer::minlvl_default(Type::DEBUG);
	
	// simple trace of variable
	let string_test = "machin".to_string();
	HTrace!(string_test);
	
	// trace with auto format
	HTrace!("test macro {}",87);
	
	// trace with return line
	HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
	
	// trace different level (ERROR level and above show backtrace)
	HTrace!((Type::NOTICE) "my trace");
	HTrace!((Type::ERROR) 21);
	HTrace!((Type::ERROR) "test macro {}",87);

	// macro for consuming Result, and tracing the error, default to ERROR (ERROR level and above show backtrace)
	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!((Type::FATAL) "File error is : {}",testerror);
	
	HTracer::drop(); // cannot be put in "Drop" because of OnceCell
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
