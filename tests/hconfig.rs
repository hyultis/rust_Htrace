#![cfg(feature = "hconfig")]
#![allow(unused_parens)]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use hconfig::HConfigManager::HConfigManager;
use hconfig::IO::json::WrapperJson;
use hconfig::tinyjson::JsonValue;
use capture::Capture;
use htrace::components::context::Context;
use htrace::components::level::Level;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
use htrace::{Errors, HTrace};
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

	// Error cases: unknown config, unreachable node, node that is not an object.
	let missingConfig = FileConfig::from_hconfig("absent", "file", FileConfig::default());
	assert!(
		matches!(&missingConfig, Err(Errors::CannotFoundConfigNode(name)) if name == "absent/file"),
		"{:?}",
		missingConfig.err()
	);
	{
		let config = HConfigManager::singleton().get("htrace").unwrap();
		config
			.write()
			.unwrap()
			.value_set("scalar", JsonValue::String("not an object".into()));
	}
	let unreachable =
		CommandLineConfig::from_hconfig("htrace", "scalar/cmd", CommandLineConfig::default());
	assert!(
		matches!(&unreachable, Err(Errors::CannotFoundConfigNode(name)) if name == "scalar/cmd"),
		"{:?}",
		unreachable.err()
	);
	{
		let config = HConfigManager::singleton().get("htrace").unwrap();
		config
			.write()
			.unwrap()
			.value_set("array", JsonValue::Array(vec![]));
	}
	// An out-of-range array index is unreachable: the array is not grown.
	let outOfRange =
		CommandLineConfig::from_hconfig("htrace", "array/5/cmd", CommandLineConfig::default());
	assert!(
		matches!(&outOfRange, Err(Errors::CannotFoundConfigNode(name)) if name == "array/5/cmd"),
		"{:?}",
		outOfRange.err()
	);
	// Unreachable paths leave the configuration untouched.
	{
		let config = HConfigManager::singleton().get("htrace").unwrap();
		let config = config.read().unwrap();
		assert!(matches!(config.value_get("scalar"), Some(JsonValue::String(value)) if value == "not an object"));
		assert!(matches!(config.value_get("array"), Some(JsonValue::Array(items)) if items.is_empty()));
	}
	// A missing nested path below objects is created and filled with the defaults.
	CommandLineConfig::from_hconfig("htrace", "nested/deeper/cmd", CommandLineConfig::default())
		.unwrap();
	assert!(matches!(
		HConfigManager::singleton()
			.get("htrace")
			.unwrap()
			.read()
			.unwrap()
			.value_get("nested/deeper/cmd/lineFormat"),
		Some(JsonValue::String(_))
	));
	let emptyKey = CommandLineConfig::from_hconfig("htrace", "", CommandLineConfig::default());
	assert!(
		matches!(&emptyKey, Err(Errors::CannotFoundConfigNode(_))),
		"{:?}",
		emptyKey.err()
	);
	let notObject =
		CommandLineConfig::from_hconfig("htrace", "scalar", CommandLineConfig::default());
	assert!(
		matches!(&notObject, Err(Errors::ModuleConfigError(module, _)) if module == "command_line"),
		"{:?}",
		notObject.err()
	);
	let notObject = FileConfig::from_hconfig("htrace", "scalar", FileConfig::default());
	assert!(
		matches!(&notObject, Err(Errors::ModuleConfigError(module, _)) if module == "file"),
		"{:?}",
		notObject.err()
	);
	let emptyPath = FileConfig::from_hconfig(
		"htrace",
		"emptyPath",
		FileConfig {
			path: "".into(),
			..FileConfig::default()
		},
	);
	match emptyPath
	{
		Err(Errors::ModuleConfigError(module, cause)) =>
		{
			assert_eq!(module, "file");
			assert!(matches!(
				cause.downcast_ref::<Errors>(),
				Some(Errors::EmptyPath)
			));
		}
		other => panic!("{:?}", other.err()),
	}

	// Absent nodes are created and filled with the defaults.
	let capture = Capture::new();
	let mut global_context = Context::default();
	global_context.name_set("hconfig-global");
	global_context.module_add("capture", capture.clone());
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(
			CommandLineConfig::from_hconfig("htrace", "cmd", CommandLineConfig::default()).unwrap(),
		),
	);
	global_context.module_add(
		"file",
		file::File::new(FileConfig::from_hconfig("htrace", "file", fileConfig).unwrap()).unwrap(),
	);
	global_context.level_setMin(Some(Level::DEBUG));
	#[cfg(all(not(feature = "tracing_consumer"), not(feature = "log_consumer")))]
	HTracer::globalContext_set(global_context).unwrap();
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(global_context, HtraceBridge::default()).unwrap();

	// simple trace of variable
	let string_test = "test with hconfig".to_string();
	HTrace!(string_test);

	HTracer::flush().unwrap();

	let traces = capture.snapshot();
	assert_eq!(traces.len(), 1, "{traces:#?}");
	assert_eq!(traces[0].level, Level::NORMAL);
	assert_eq!(traces[0].message, "test with hconfig");
	assert_eq!(traces[0].contextName.as_deref(), Some("hconfig-global"));
	assert_eq!(traces[0].backtraceCount, 0);
	// The file module built from the Hconfig node writes to the supplied path.
	let written = fs::read_dir(testPath.join("traces"))
		.unwrap()
		.map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
		.collect::<Vec<_>>();
	assert!(
		!written.is_empty()
			&& written
				.iter()
				.all(|contents| contents.contains("test with hconfig")),
		"{written:?}"
	);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	assert_eq!(capture.globalExits(), 1);
	fs::remove_dir_all(&testPath).unwrap();
}
