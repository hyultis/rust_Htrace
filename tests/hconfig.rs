#![allow(unused_parens)]

use std::fs;
use Hconfig::HConfigManager::HConfigManager;
use Hconfig::IO::json::WrapperJson;
use std::fs::create_dir;
use std::path::Path;
use Htrace::components::context::Context;
use Htrace::components::level::Level;
use Htrace::crates::bridge::HtraceBridge;
use Htrace::HTrace;
use Htrace::htracer::HTracer;
use Htrace::modules::{command_line, file};
use Htrace::modules::command_line_config::CommandLineConfig;
use Htrace::modules::file_config::FileConfig;

#[test]
fn trace_with_hconfig()
{
	let _ = fs::remove_dir_all("./traces");


	// hconfig creating config dir
	let config_dir = Path::new("./config");
	if (!config_dir.exists())
	{
		create_dir(config_dir).unwrap();
	}

	// initialising hconfig and htrace config
	HConfigManager::singleton().confPath_set("./config");
	HConfigManager::singleton()
		.create::<WrapperJson>("htrace")
		.expect("bug from hconfig");

	let mut global_context = Context::default();
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::create_from_hconfig(
			HConfigManager::singleton()
				.get("htrace")
				.unwrap()
				.value_get_mut("cmd")
				.unwrap(),
		)),
	);
	global_context.module_add(
		"file",
		file::File::new(FileConfig::create_from_hconfig(
			HConfigManager::singleton()
				.get("htrace")
				.unwrap()
				.value_get_mut("file")
				.unwrap(),
		)),
	);
	global_context.level_setMin(Some(Level::DEBUG));
	#[cfg(all(not(feature = "tracing_consumer"),not(feature = "log_consumer")))]
	HTracer::globalContext_set(global_context);
	#[cfg(any(feature = "tracing_consumer",feature = "log_consumer"))]
	HTracer::globalContext_set(global_context, HtraceBridge::default());

	// simple trace of variable
	let string_test = "test with hconfig".to_string();
	HTrace!(string_test);

	// we need to wait all threads are done
	HTracer::drop();
}