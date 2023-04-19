#[cfg(test)]
mod tests {
	use std::fs::create_dir;
	use std::path::Path;
	use Hconfig::HConfigManager::HConfigManager;
	use crate::CommandLine::CommandLineConfig;
	use crate::File::FileConfig;
	use super::super::*;
	
	#[test]
	fn log() {
		
		let configDir = Path::new("./config");
		if (!configDir.exists())
		{
			create_dir(configDir).unwrap();
		}
		
		HConfigManager::singleton().setConfPath("./config");
		HTracer::HTracer::appendModule("cmd", CommandLine::CommandLine::new(CommandLineConfig::default())).expect("Cannot append module");
		HTracer::HTracer::appendModule("file", File::File::new(FileConfig::default())).expect("Cannot append module");
		HTracer::HTracer::threadSetName("testThreadName");
		
		let tudzpsofh = "machin".to_string();
		HTracer::HTracer::log(&tudzpsofh, Type::Type::NORMAL, file!(), line!());
		println!("{:?}",tudzpsofh);
		HTrace!(tudzpsofh);
		println!("{:?}",tudzpsofh);
		
		HTrace!("test macro\nlmsdkhfsldf\nmsdf\nhjsdf");
		HTrace!("test macro", Type::Type::ERROR);
		HTrace!(21);
		
		HTracer::HTracer::drop();
	}
}
