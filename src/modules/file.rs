use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;
use anyhow::Result;
use Hconfig::tinyjson::JsonValue;
use time::macros::format_description;
use crate::components::context::Context;
use crate::components::formater::{FormaterParamBuilderSignature, FormaterCompilerSignature, FormaterParamBuilder, FormaterCompile, FormaterCompiled};
use crate::modules::module_abstract::ModuleAbstract;
use crate::components::trace::OneTrace;
use crate::modules::utils::{setConfig_String, setConfig_boolean};
use crate::thread_manager::{ThreadManager, MAIN_THREAD};

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

pub struct File
{
	_name: String,
	_configs: FileConfig,
	_formaterCompiled: FormaterCompiled,
}

impl File
{
	pub fn new(config: FileConfig) -> File {
		let binding = &config.formaterCompiler;
		let fmtComp = binding(&config.lineFormat);
		return File{
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
		};
	}
	
	fn generateLine(&self, trace: &OneTrace)
	{

		let binding = &self._configs.formaterParamBuilder;
		let parameters = binding(trace, &self._configs.lineReturn);
		let formatResult = self._formaterCompiled.render(parameters);

		let mut filedateformat = format_description!("[year][month][day]");
		if(self._configs.byHour)
		{
			filedateformat = format_description!("[year][month][day]_[hour repr:24]");
		}

		if self._configs.forceInOneFile.is_some()
		{
			let path = format!("{}/{}_{}.trc",self._configs.path,self._configs.forceInOneFile.clone().unwrap(),trace.date.format(filedateformat).unwrap_or("00000000".to_string()));
			self.writeToFile(path, formatResult.clone());
		};
		if self._configs.bySrc
		{
			let filename = trace.filename.clone();
			let tmp: Vec<_> = filename.split("/").collect();
			let tmp: Vec<_> = tmp.last().unwrap().split(".").collect();
			let filename = tmp[0].to_string();
			let path = format!("{}/{}_{}.trc",self._configs.path,filename,trace.date.format(filedateformat).unwrap_or("00000000".to_string()));
			self.writeToFile(path, formatResult.clone());
		}
		if self._configs.byThreadId
		{
			let filename = ThreadManager::local_getName().unwrap_or(MAIN_THREAD.to_string());
			let path = format!("{}/{}_{}.trc",self._configs.path,filename,trace.date.format(filedateformat).unwrap_or("00000000".to_string()));
			self.writeToFile(path, formatResult.clone());
		}
	}
	
	fn writeToFile(&self, filepath: String,lineToWrite: String)
	{
		let filepathC = Path::new(filepath.as_str());
		if(filepathC.parent().is_none())
		{
			return;
		}
		let parentPathC = filepathC.parent().unwrap();
		if(create_dir_all(parentPathC).is_err())
		{
			return;
		}
		let Rfile = OpenOptions::new().create(true).append(true).open(filepath);
		if(Rfile.is_err())
		{
			return;
		}
		let mut Rfile = Rfile.unwrap();
		
		let _iswrited = Rfile.write(format!("{}\n",lineToWrite).as_bytes());
	}
	
}


impl ModuleAbstract for File
{
	fn name_set(&mut self, moduleName: String) -> Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn name_get(&self) -> Result<String> {
		return Ok(self._name.clone());
	}
	
	fn config_set(&mut self, configs: &mut JsonValue) -> Result<()>
	{
		let JsonValue::Object(config) = configs else {return Ok(())};
		setConfig_String(config,"path",&mut self._configs.path, |_|true);
		setConfig_String(config,"lineReturn",&mut self._configs.lineReturn, |_|true);
		setConfig_String(config,"lineFormat",&mut self._configs.lineFormat, |_|true);
		setConfig_boolean(config,"byHour",&mut self._configs.byHour);
		setConfig_boolean(config,"bySrc",&mut self._configs.bySrc);
		setConfig_boolean(config,"byThreadId",&mut self._configs.byThreadId);
		
		
		if let Some(val) = config.get("forceInOneFile")
		{
			let tmp: String = val.clone().try_into()?;
			if(tmp == "")
			{
				self._configs.forceInOneFile = None;
			}
			else
			{
				self._configs.forceInOneFile = Some(tmp.to_string());
			}
		}
		else
		{
			config.insert("forceInOneFile".to_string(), JsonValue::String(self._configs.forceInOneFile.clone().unwrap_or("".to_string())));
		}

		// update formater
		let binding = &self._configs.formaterCompiler;
		let fmtComp = binding(&self._configs.lineFormat);
		self._formaterCompiled = fmtComp;
		
		return Ok(());
	}
	
	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onError(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}
	
	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onContextExit(&self, _: &Context) {}

	fn event_onGlobalExit(&self) {}
}
