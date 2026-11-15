#![cfg(all(feature = "log_consumer", feature = "default_module"))]
#![allow(unused_parens)]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
use log::Log;
use std::fs;

#[test]
fn trace_from_log_crate()
{
	let tracePath = std::env::temp_dir().join(format!(
		"htrace-log-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	fs::create_dir(&tracePath).unwrap();
	let mut fileConfig = FileConfig::default();
	fileConfig.path = tracePath.to_string_lossy().into_owned();

	let capture = Capture::new();
	let mut global_context = Context::default();
	global_context.name_set("log-global");
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::default()),
	);
	global_context.module_add("file", file::File::new(fileConfig).unwrap());
	global_context.module_add("capture", capture.clone());
	global_context.level_setMin(Some(Level::DEBUG));
	HTracer::globalContext_set(global_context, HtraceBridge::default()).unwrap();
	// The default bridge accepts DEBUG, the most verbose `log` level.
	assert_eq!(log::max_level(), log::LevelFilter::Trace);

	log::trace!("test trace! from log crate");
	log::debug!("test debug! from log crate");
	log::info!("test info! from log crate");
	log::warn!("test warn! from log crate");
	log::error!("test error! from log crate");
	HTracer::flush().unwrap();

	// log -> Htrace level mapping, in emission order.
	let traces = capture.snapshot();
	assert_eq!(
		traces
			.iter()
			.map(|trace| (trace.level, trace.message.as_str()))
			.collect::<Vec<_>>(),
		vec![
			(Level::DEBUG, "test trace! from log crate"),
			(Level::NORMAL, "test debug! from log crate"),
			(Level::NOTICE, "test info! from log crate"),
			(Level::WARNING, "test warn! from log crate"),
			(Level::ERROR, "test error! from log crate"),
		]
	);
	for trace in &traces
	{
		assert_eq!(trace.contextName.as_deref(), Some("log-global"));
		assert!(trace.filename.ends_with("log.rs"), "{}", trace.filename);
		assert!(trace.fileline > 0);
		// Bridges capture backtraces from ERROR by default.
		match trace.level
		{
			Level::ERROR =>
			{
				assert!(trace.backtraceCount > 0, "{trace:?}");
				assert!(
					trace
						.backtraces
						.iter()
						.any(|frame| frame.contains("trace_from_log_crate")),
					"{:#?}",
					trace.backtraces
				);
			}
			_ => assert_eq!(trace.backtraceCount, 0, "{trace:?}"),
		}
	}

	// `log_min_level` is applied by the bridge itself, whatever the global filter.
	capture.clear();
	let strict = HtraceBridge {
		log_min_level: Level::WARNING,
		..HtraceBridge::default()
	};
	for (level, message) in [
		(log::Level::Info, "below log_min_level"),
		(log::Level::Warn, "at log_min_level"),
	]
	{
		let metadata = log::Metadata::builder().level(level).build();
		assert_eq!(strict.enabled(&metadata), level == log::Level::Warn);
		strict.log(
			&log::Record::builder()
				.level(level)
				.args(format_args!("{message}"))
				.file_static(Some(file!()))
				.line(Some(line!()))
				.build(),
		);
	}
	HTracer::flush().unwrap();
	assert_eq!(
		capture
			.snapshot()
			.iter()
			.map(|trace| (trace.level, trace.message.as_str()))
			.collect::<Vec<_>>(),
		vec![(Level::WARNING, "at log_min_level")]
	);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	assert_eq!(capture.globalExits(), 1);
	fs::remove_dir_all(&tracePath).unwrap();
}
