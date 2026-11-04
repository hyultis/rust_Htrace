#![cfg(feature = "hconfig")]
#![allow(unused_parens)]

use Hconfig::HConfigManager::HConfigManager;
use Hconfig::IO::json::WrapperJson;
use htrace::HTrace;
use htrace::components::context::Context;
use htrace::components::level::Level;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
use std::fs;

#[test]
fn trace_with_hconfig()
{
	let testPath = std::env::temp_dir().join(format!(
		"htrace-hconfig-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	let configPath = testPath.join("config");
	fs::create_dir_all(&configPath).unwrap();
	let mut fileConfig = FileConfig::default();
	fileConfig.path = testPath.join("traces").to_string_lossy().into_owned();

	// Each integration test binary owns its singleton and temporary configuration.
	HConfigManager::singleton().confPath_set(configPath.to_str().unwrap());
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
			CommandLineConfig::default(),
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
			fileConfig,
		)),
	);
	global_context.level_setMin(Some(Level::DEBUG));
	#[cfg(all(not(feature = "tracing_consumer"), not(feature = "log_consumer")))]
	HTracer::globalContext_set(global_context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(global_context, HtraceBridge::default());

	// simple trace of variable
	let string_test = "test with hconfig".to_string();
	HTrace!(string_test);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	fs::remove_dir_all(&testPath).unwrap();
}
