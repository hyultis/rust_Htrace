#![cfg(all(feature = "log_consumer", feature = "default_module"))]
#![allow(unused_parens)]

use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
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

	let mut global_context = Context::default();
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::default()),
	);
	global_context.module_add("file", file::File::new(fileConfig));
	global_context.level_setMin(Some(Level::DEBUG));
	HTracer::globalContext_set(global_context, HtraceBridge::default());

	log::trace!("test trace! from log crate"); // trace in tracing have upper level than debug, so its converted to DEBUG in htrace
	log::debug!("test debug! from log crate"); // debug in tracing have upper level than normal, so its converted to NORMAL in htrace
	log::info!("test info! from log crate");
	log::warn!("test warn! from log crate");
	log::error!("test error! from log crate");

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	fs::remove_dir_all(&tracePath).unwrap();
}
