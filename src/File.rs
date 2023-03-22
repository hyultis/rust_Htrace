use std::collections::HashMap;
use std::fs::{create_dir_all, File as stdFile, OpenOptions};
use std::io::Write;
use std::path::Path;
use anyhow::anyhow;
use crate::ModuleAbstract::ModuleAbstract;
use crate::OneLog::OneLog;

pub struct File
{
	_name: String,
	_forceFilename: Option<String>,
	_configs: HashMap<String, String>,
	_basePath: Option<String>
}

impl File
{
	pub fn new(parentPath: String) -> File {
		return File{
			_name: String::new(),
			_forceFilename: None,
			_configs: HashMap::new(),
			_basePath: Some(parentPath)
		};
	}
	
	pub fn forceFilename(&mut self, filename: String)
	{
		self._forceFilename = Some(filename);
	}
	
	fn generateLine(&self, onelog: OneLog)
	{
		if(self._basePath.is_none())
		{
			return;
		}
		//$finalmsg = $objlog->date->format('H:i:s') . ' ' . $this->levelToString($objlog->level) . ' (on /' . $this->getDiffDir(dirname($filename),loader::get_dirRoot()).basename($filename) . ':l' . $line . ') : ' . $objmsg;
		
		let mut msg = onelog.message;
		if(msg.contains("\n") || msg.contains("\r"))
		{
			msg = msg.replace("\n\r","\n");
			msg = msg.replace("\r","\n");
			msg = msg.replace("\n","\n|");
		}
		
		let finalmsg = format!("{} {:4} (on {}:l{}) : {}\n",onelog.date.format("%H:%M:%S"),onelog.level,onelog.filename,onelog.fileline,msg);
		let mut filename = self._forceFilename.clone();
		if(filename.is_none())
		{
			let tmp: Vec<_> = onelog.filename.split("/").collect();
			let tmp: Vec<_> = tmp.last().unwrap().split(".").collect();
			filename = Some(tmp[0].to_string());
		}
		let mut filedateformat = "%Y:%m:%d";
		if(true)
		{
			filedateformat = "%Y%m%d_%H";
		}
		let path = format!("{}/traces/{}_{}.trc",self._basePath.clone().unwrap(),filename.unwrap(),onelog.date.format(filedateformat));
		self.writeToFile(path,finalmsg);
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
		
		let _iswrited = Rfile.unwrap().write(lineToWrite.as_bytes());
	}
}


impl ModuleAbstract for File
{
	fn setModuleName(&mut self, moduleName: String) -> anyhow::Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn getModuleName(&self) -> anyhow::Result<String> {
		return Ok(self._name.clone());
	}
	
	fn setConfig(&mut self, configs: HashMap<String, String>) -> anyhow::Result<()>
	{
		/*if(!configs.contains_key("path"))
		{
			return Err(anyhow!("[Htrace/File] : \"path\" key is absent in config"));
		}
		else
		{
			let tmp = configs.get("path").unwrap().clone();
			if tmp.starts_with("%dynamic%")
			{
				configs.insert("path".to_string(),format!("{}{}","./dynamic",tmp.strip_prefix("%dynamic%").unwrap()));
			}
			self._basePath = Some(configs.get("path").unwrap().clone());
			println!("basepath : {}",self._basePath.clone().unwrap());
		}*/
		
		self._configs = configs;
		
		Ok(())
	}
	
	fn Event_onDebug(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onDebugErr(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNormal(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNotice(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onNoticeErr(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onWarning(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onError(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onFatal(&self, log: OneLog)
	{
		self.generateLine(log);
	}
	
	fn Event_onExit(&self) {
		todo!()
	}
}
