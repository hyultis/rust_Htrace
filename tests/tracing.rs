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
fn trace_from_tracing_crate()
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

	tracing::trace!("test trace! from tracing crate"); // should not be displayed // trace in tracing have upper level than debug, so its converted to DEBUG in htrace
	tracing::debug!("test debug! from tracing crate"); // debug in tracing have upper level than normal, so its converted to NORMAL in htrace
	tracing::info!("test info! from tracing crate");
	tracing::warn!("test warn! from tracing crate");
	tracing::error!("test error! from tracing crate");

	// we need to wait all threads are done
	HTracer::drop();
}