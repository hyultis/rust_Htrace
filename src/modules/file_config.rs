use crate::components::formater::{FormaterCompile, FormaterCompilerSignature, FormaterParamBuilder, FormaterParamBuilderSignature};

#[cfg(feature = "hconfig")]
use Hconfig::tinyjson::JsonValue;

/// note: if byThreadId is false, bySrc is false and forceInOneFile is None, no trace will be written
pub struct FileConfig
{
	/// base path where to write files
	pub path: String,
	/// adding string when a trace have à return char "\n"/"\r"
	pub lineReturn: String,
	/// format of the trace, view HtraceDefaultFormater for available variable
	pub lineFormat: String,
	/// write a file by thread name
	pub byThreadId: bool,
	/// write a file by src of trace
	pub bySrc: bool,
	/// each file wrote is by hour (add hour after date in filename)
	pub byHour: bool,
	/// write all trace in one file (auto append "_{time}.trc")
	pub forceInOneFile: Option<String>,
	/// define the way to collect data (using lineReturn)
	pub formaterParamBuilder: FormaterParamBuilderSignature,
	/// define the way convert collected data into string (using lineFormat)
	pub formaterCompiler: FormaterCompilerSignature
}

impl Default for FileConfig
{
	fn default() -> Self {
		return FileConfig{
			path: "./traces".to_string(),
			lineReturn: " | ".to_string(),
			lineFormat: "{time} {lvl} ({thread:>, }{file}:l{line}) : {msg}".to_string(),
			byThreadId: true,
			bySrc: false,
			byHour: false,
			forceInOneFile: None,
			formaterParamBuilder: FormaterParamBuilder,
			formaterCompiler: FormaterCompile,
		};
	}
}

#[cfg(feature = "hconfig")]
impl FileConfig
{
	pub fn create_from_hconfig(configs: &mut JsonValue) -> Self
	{
		let mut newConfig = Self::default();
		use crate::modules::utils_hconfig::{setConfig_String, setConfig_boolean};
		use Hconfig::tinyjson::JsonValue;

		let JsonValue::Object(config) = configs else {return newConfig};
		setConfig_String(config,"path",&mut newConfig.path, |_|true);
		setConfig_String(config,"lineReturn",&mut newConfig.lineReturn, |_|true);
		setConfig_String(config,"lineFormat",&mut newConfig.lineFormat, |_|true);
		setConfig_boolean(config,"byHour",&mut newConfig.byHour);
		setConfig_boolean(config,"bySrc",&mut newConfig.bySrc);
		setConfig_boolean(config,"byThreadId",&mut newConfig.byThreadId);


		if let Some(val) = config.get("forceInOneFile")
		{
			let Ok(tmp) = val.clone().try_into() else {return newConfig};
			let tmp: &String = &tmp;
			if(tmp == "")
			{
				newConfig.forceInOneFile = None;
			}
			else
			{
				newConfig.forceInOneFile = Some(tmp.to_string());
			}
		}
		else
		{
			config.insert("forceInOneFile".to_string(), JsonValue::String(newConfig.forceInOneFile.clone().unwrap_or("".to_string())));
		}

		return newConfig;
	}
}