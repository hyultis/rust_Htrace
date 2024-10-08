use std::collections::HashMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::path::Path;
use anyhow::Result;
use Hconfig::serde_json::Value;
use strfmt::strfmt;
use crate::HTracer::HTracer;
use crate::ModuleAbstract::{ModuleAbstract, setConfig_boolean, setConfig_String};
use crate::OneLog::OneLog;

pub struct FileConfig
{
	pub path: String,
	pub lineReturn: String,
	pub lineFormat: String,
	/// write a file by threadid
	pub byThreadId: bool,
	/// write a file by src of trace
	pub bySrc: bool,
	/// write a file by hour (add hour after date in filename)
	pub byHour: bool,
	/// write all trace in one file (auto append "_{time}.trc")
	pub forceInOneFile: Option<String>
}

impl Default for FileConfig
{
	fn default() -> Self {
		return FileConfig{
			path: "./traces".to_string(),
			lineReturn: " | ".to_string(),
			lineFormat: "{time} {lvl} ({file}:l{line}) : {msg}".to_string(),
			byThreadId: true,
			bySrc: false,
			byHour: false,
			forceInOneFile: None
		};
	}
}

pub struct File
{
	_name: String,
	_configs: FileConfig
}

impl File
{
	pub fn new(config: FileConfig) -> File {
		return File{
			_name: String::new(),
			_configs: config
		};
	}
	
	fn generateLine(&self, log: &OneLog)
	{
		let mut msg = log.message.clone();
		if(msg.contains("\n") || msg.contains("\r") || msg.contains("\\n") || msg.contains("\\r"))
		{
			let linereturn = format!("\n{}",self._configs.lineReturn);
			msg = msg.replace("\n\r","\n");
			msg = msg.replace("\\n\\r","\n");
			msg = msg.replace("\r","\n");
			msg = msg.replace("\\r","\n");
			msg = msg.replace("\\n","\n");
			msg = msg.replace("\n",linereturn.as_str());
		}
		
		if(log.backtraces.len()>0)
		{
			let mut drawBacktraces= "".to_string();
			log.backtraces.iter().for_each(|one|{
				drawBacktraces = format!("{}\n{}{}",drawBacktraces,self._configs.lineReturn,one.to_string());
			});
			
			msg = format!("{}, with : {}",msg,drawBacktraces)
		}
		
		let mut filedateformat = "%Y%m%d";
		if(self._configs.byHour)
		{
			filedateformat = "%Y%m%d_%H";
		}
		
		let mut vars = HashMap::new();
		vars.insert("time".to_string(),log.date.format("%H:%M:%S%.6f").to_string());
		vars.insert("lvl".to_string(),log.level.convert4LengthString());
		vars.insert("file".to_string(),log.filename.clone());
		vars.insert("line".to_string(),log.fileline.to_string());
		vars.insert("msg".to_string(),msg);
		
		let format = self._configs.lineFormat.clone();
		let formatResult = strfmt(&format, &vars).unwrap();
		if self._configs.forceInOneFile.is_some()
		{
			let path = format!("{}/{}_{}.trc",self._configs.path,self._configs.forceInOneFile.clone().unwrap(),log.date.format(filedateformat));
			self.writeToFile(path, formatResult.clone());
		};
		if self._configs.bySrc
		{
			let filename = log.filename.clone();
			let tmp: Vec<_> = filename.split("/").collect();
			let tmp: Vec<_> = tmp.last().unwrap().split(".").collect();
			let filename = tmp[0].to_string();
			let path = format!("{}/{}_{}.trc",self._configs.path,filename,log.date.format(filedateformat));
			self.writeToFile(path, formatResult.clone());
		}
		if self._configs.byThreadId
		{
			let filename = HTracer::threadGetName(log.threadId);
			let path = format!("{}/{}_{}.trc",self._configs.path,filename,log.date.format(filedateformat));
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
	fn setModuleName(&mut self, moduleName: String) -> Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn getModuleName(&self) -> Result<String> {
		return Ok(self._name.clone());
	}
	
	fn setConfig(&mut self, configs: &mut Value) -> Result<()>
	{
		let Value::Object(config) = configs else {return Ok(())};
		setConfig_String(config,"path",&mut self._configs.path, |_|true);
		setConfig_String(config,"lineReturn",&mut self._configs.lineReturn, |_|true);
		setConfig_String(config,"lineFormat",&mut self._configs.lineFormat, |_|true);
		setConfig_boolean(config,"byHour",&mut self._configs.byHour);
		setConfig_boolean(config,"bySrc",&mut self._configs.bySrc);
		setConfig_boolean(config,"byThreadId",&mut self._configs.byThreadId);
		
		
		if let Some(val) = config.get("forceInOneFile")
		{
			let Some(tmp) = val.as_str() else {return Ok(())};
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
			config.insert("forceInOneFile".to_string(), Value::String(self._configs.forceInOneFile.clone().unwrap_or("".to_string())));
		}
		
		Ok(())
	}
	
	fn Event_onDebug(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onDebugErr(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNormal(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNotice(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNoticeErr(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onWarning(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onError(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onFatal(&self, log: &OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onExit(&self) {
	
	}
}
