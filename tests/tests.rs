#![allow(unused_parens)]

use std::fs::create_dir;
use std::io;
use std::path::Path;
use Hconfig::HConfigManager::HConfigManager;
use Htrace::HTracer::HTracer;
use Htrace::Type::Type;
use Htrace::{HTrace, CommandLine, File, HTraceError};
use Htrace::CommandLine::CommandLineConfig;
use Htrace::File::FileConfig;

#[test]
fn log() {
	let configDir = Path::new("./config");
	if (!configDir.exists())
	{
		create_dir(configDir).unwrap();
	}
	
	HConfigManager::singleton().setConfPath("./config");
	HTracer::appendModule("cmd", CommandLine::CommandLine::new(CommandLineConfig::default())).expect("Cannot append module");
	HTracer::appendModule("file", File::File::new(FileConfig::default())).expect("Cannot append module");
	HTracer::threadSetName("testThreadName");
	
	let tudzpsofh = "machin".to_string();
	HTracer::log(&tudzpsofh, Type::NORMAL, file!(), line!());
	HTrace!(tudzpsofh);
	
	HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
	HTrace!("test macro {}",87);
	HTrace!((Type::ERROR) 21);
	HTrace!((Type::ERROR) "test macro {}",87);

	let testerror = std::fs::File::open(Path::new("idontexist.muahahah"));
	HTraceError!((Type::FATAL) testerror);
	HTraceError!((Type::FATAL) "File error is : {}",testerror);
	
	HTracer::drop(); // cannot be put in "Drop" because of OnceCell
}
