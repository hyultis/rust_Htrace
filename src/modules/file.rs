use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;
use anyhow::Result;
use time::macros::format_description;
use crate::components::context::Context;
use crate::components::formater::FormaterCompiled;
use crate::modules::module_abstract::ModuleAbstract;
use crate::components::trace::OneTrace;
use crate::modules::file_config::FileConfig;
use crate::thread_manager::{ThreadManager, MAIN_THREAD};

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
