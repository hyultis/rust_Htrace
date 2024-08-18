#![allow(unused_parens)]

use std::fs::create_dir;
use std::path::Path;
use Hconfig::HConfigManager::HConfigManager;
use Htrace::HTracer::HTracer;
use Htrace::Type::Type;
use Htrace::{HTrace, CommandLine, File, HTraceError};
use Htrace::CommandLine::CommandLineConfig;
use Htrace::File::FileConfig;

#[test]
fn log() {
	let config_dir = Path::new("./config");
	if (!config_dir.exists())
	{
		create_dir(config_dir).unwrap();
	}
	
	HConfigManager::singleton().setConfPath("./config");
	HTracer::appendModule("cmd", CommandLine::CommandLine::new(CommandLineConfig::default())).expect("Cannot append module");
	HTracer::appendModule("file", File::File::new(FileConfig::default())).expect("Cannot append module");
	HTracer::threadSetName("testThreadName");
	HTracer::minlvl_default(Type::DEBUG);
	
	// simple trace of variable
	let string_test = "machin".to_string();
	HTrace!(string_test);
	
	// trace with auto format
	HTrace!("test macro {}",87);
	
	// trace with return line
	HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
	
	// trace different level (ERROR level and above show backtrace)
	HTrace!((Type::DEBUG) "my debug");
	HTrace!((Type::NOTICE) "my trace");
	HTrace!((Type::ERROR) 21);
	HTrace!((Type::ERROR) "test macro {}",87);

	// macro for consuming Result, and tracing the error, default to ERROR (ERROR level and above show backtrace)
	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!(testerror);
	HTraceError!("File error is : {}",testerror);
	HTraceError!((Type::FATAL) testerror);
	HTraceError!((Type::FATAL) "File error is : {}",testerror);
	
	HTracer::drop(); // cannot be put in "Drop" because of OnceCell
}
