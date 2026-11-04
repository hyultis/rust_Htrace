#![cfg(feature = "default_module")]
#![allow(unused_parens)]

use htrace::components::context::Context;
use htrace::components::level::Level;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
use htrace::{HTrace, HTraceError, Spaned};
use std::fs;
use std::path::Path;

#[test]
fn trace()
{
	let tracePath = std::env::temp_dir().join(format!(
		"htrace-trace-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	fs::create_dir(&tracePath).unwrap();
	let mut fileConfig = FileConfig::default();
	fileConfig.path = tracePath.to_string_lossy().into_owned();

	// updating lineFormat to check if context is working
	let mut default_command_config = CommandLineConfig::default();
	default_command_config.lineFormat =
		"{time} {lvl} ({thread:>, }{context:>, }{file}:l{line}) : {msg}".to_string();

	let mut global_context = Context::default();
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(default_command_config),
	);
	global_context.module_add("file", file::File::new(fileConfig));
	global_context.level_setMin(Some(Level::DEBUG));
	#[cfg(all(not(feature = "tracing_consumer"), not(feature = "log_consumer")))]
	HTracer::globalContext_set(global_context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(global_context, HtraceBridge::default());

	// simple trace of variable
	let string_test = "machin".to_string();
	HTrace!(string_test);

	// trace with return line
	{
		let mut local_context = Context::default();
		local_context.name_set("span lvl 1");
		Spaned!(local_context);
		HTrace!("test inside span 1");
		{
			Spaned!("span lvl 2");
			HTrace!("test inside span 2");
		}
		HTrace!("test inside span 1");
	}

	// trace with auto format
	HTrace!("test macro {}", 87);

	// trace different level (ERROR level and above show backtrace)
	HTrace!((Level::DEBUG) "my debug");
	HTrace!((Level::NOTICE) "my trace");
	HTrace!((Level::ERROR) 21);
	HTrace!((Level::FATAL) "test macro {}",87);

	// Borrow the Result and trace its error, at ERROR by default.
	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!("File error is : {}", testerror);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	fs::remove_dir_all(&tracePath).unwrap();
}
