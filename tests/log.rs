#![allow(unused_parens)]

use std::fs;
use Htrace::components::context::Context;
use Htrace::components::level::Level;
use Htrace::crates::bridge::HtraceBridge;
use Htrace::htracer::HTracer;
use Htrace::modules::{command_line, file};
use Htrace::modules::command_line_config::CommandLineConfig;
use Htrace::modules::file_config::FileConfig;

#[test]
fn trace_from_log_crate()
{
	let _ = fs::remove_dir_all("./traces");

	let mut global_context = Context::default();
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::default()),
	);
	global_context.module_add("file", file::File::new(FileConfig::default()));
	global_context.level_setMin(Some(Level::DEBUG));
	HTracer::globalContext_set(global_context, HtraceBridge::default());

	log::trace!("test trace! from log crate"); // trace in tracing have upper level than debug, so its converted to DEBUG in htrace
	log::debug!("test debug! from log crate"); // debug in tracing have upper level than normal, so its converted to NORMAL in htrace
	log::info!("test info! from log crate");
	log::warn!("test warn! from log crate");
	log::error!("test error! from log crate");

	// we need to wait all threads are done
	HTracer::drop();
}