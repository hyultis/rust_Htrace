#![cfg(feature = "default_module")]
#![allow(unused_parens)]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
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

	let capture = Capture::new();
	let mut global_context = Context::default();
	global_context.name_set("trace-global");
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(default_command_config),
	);
	global_context.module_add("file", file::File::new(fileConfig).unwrap());
	global_context.module_add("capture", capture.clone());
	global_context.level_setMin(Some(Level::DEBUG));
	#[cfg(all(not(feature = "tracing_consumer"), not(feature = "log_consumer")))]
	HTracer::globalContext_set(global_context).unwrap();
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(global_context, HtraceBridge::default()).unwrap();

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

	HTracer::flush().unwrap();

	let traces = capture.snapshot();
	assert_eq!(traces.len(), 10, "{traces:#?}");
	assert_eq!(
		traces[..9]
			.iter()
			.map(|trace| (
				trace.level,
				trace.message.as_str(),
				trace.contextName.as_deref().unwrap_or("")
			))
			.collect::<Vec<_>>(),
		vec![
			(Level::NORMAL, "machin", "trace-global"),
			(Level::NORMAL, "test inside span 1", "span lvl 1"),
			(Level::NORMAL, "test inside span 2", "span lvl 2"),
			(Level::NORMAL, "test inside span 1", "span lvl 1"),
			(Level::NORMAL, "test macro 87", "trace-global"),
			(Level::DEBUG, "my debug", "trace-global"),
			(Level::NOTICE, "my trace", "trace-global"),
			(Level::ERROR, "21", "trace-global"),
			(Level::FATAL, "test macro 87", "trace-global"),
		]
	);
	// The borrowed error is traced at ERROR with its Display output.
	let error = &traces[9];
	assert_eq!(error.level, Level::ERROR);
	assert!(
		error.message.starts_with("File error is : ") && error.message.len() > 16,
		"{}",
		error.message
	);
	for trace in &traces
	{
		assert!(trace.filename.ends_with("trace.rs"), "{}", trace.filename);
		assert!(trace.fileline > 0);
		// Producers are MAIN unless a span names them.
		assert_eq!(trace.threadName.as_deref(), Some("MAIN"));
		// Macros capture backtraces from ERROR.
		match trace.level
		{
			Level::ERROR | Level::FATAL =>
			{
				assert!(trace.backtraceCount > 0, "{trace:?}")
			}
			_ => assert_eq!(trace.backtraceCount, 0, "{trace:?}"),
		}
	}
	// One exit notification per span.
	assert_eq!(capture.contextExits(), 2);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	assert_eq!(capture.globalExits(), 1);
	fs::remove_dir_all(&tracePath).unwrap();
}
