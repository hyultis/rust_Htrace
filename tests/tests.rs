#![allow(unused_parens)]

use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use time::macros::datetime;
use Htrace::htracer::HTracer;
use Htrace::components::level::Level;
use Htrace::{HTrace, HTraceError, Spaned};
use Htrace::components::context::Context;
use Htrace::components::formater::{FormaterCompile};
use Htrace::modules::command_line_config::CommandLineConfig;
use Htrace::modules::file_config::FileConfig;
use Htrace::modules::{file, command_line};
use Htrace::components::trace::OneTrace;


#[test]
fn formater()
{
	use Htrace::components::formater::FormaterParamBuilder;
	HTracer::globalContext_set(Context::default());

	let compiled = FormaterCompile(&"{time} {lvl} ({thread:>, }{file}:l{line} |{extra}{extra[test]}|) : {msg}".to_string());

	let mut context = Context::default();
	context.extra_set("test","cake");

	let parameters = FormaterParamBuilder(&OneTrace {
		message: "message line".to_string(),
		date: datetime!(1900-01-01 0:00 UTC),
		level: Level::DEBUG,
		context: context,
		filename: "file.rs".to_string(),
		fileline: 42,
		backtraces: vec![],
	}, &" | ".to_string());
	assert_eq!(compiled.render(parameters), "00:00:00.000000 DBUG (MAIN, file.rs:l42 |{extra}cake|) : message line", "simpleFormater format changed");
}

#[test]
fn trace() {
	let mut global_context = Context::default();
	global_context.module_add("cmd", command_line::CommandLine::new(CommandLineConfig::default()));
	global_context.module_add("file", file::File::new(FileConfig::default()));
	global_context.level_setMin(Some(Level::DEBUG));
	HTracer::globalContext_set(global_context);

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
	HTrace!("test macro {}",87);

	// trace different level (ERROR level and above show backtrace)
	HTrace!((Level::DEBUG) "my debug");
	HTrace!((Level::NOTICE) "my trace");
	HTrace!((Level::ERROR) 21);
	HTrace!((Level::FATAL) "test macro {}",87);

	// macro for consuming Result, and tracing the error, default to ERROR
	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!("File error is : {}",testerror);

	// we need to wait all thread are done
	sleep(Duration::from_millis(100));
}

#[cfg(feature = "hconfig")]
#[test]
fn trace_with_hconfig() {
	use std::fs::create_dir;
	use Hconfig::HConfigManager::HConfigManager;
	use Hconfig::IO::json::WrapperJson;

	// hconfig creating config dir
	let config_dir = Path::new("./config");
	if (!config_dir.exists())
	{
		create_dir(config_dir).unwrap();
	}

	// initialising hconfig and htrace config
	HConfigManager::singleton().confPath_set("./config");
	HConfigManager::singleton().create::<WrapperJson>("htrace").expect("bug from hconfig");

	let mut global_context = Context::default();
	global_context.module_add("cmd", command_line::CommandLine::new(CommandLineConfig::create_from_hconfig(HConfigManager::singleton().get("htrace").unwrap().value_get_mut("cmd").unwrap())));
	global_context.module_add("file", file::File::new(FileConfig::create_from_hconfig(HConfigManager::singleton().get("htrace").unwrap().value_get_mut("file").unwrap())));
	global_context.level_setMin(Some(Level::DEBUG));
	HTracer::globalContext_set(global_context);

	// simple trace of variable
	let string_test = "test with hconfig".to_string();
	HTrace!(string_test);
}