use std::collections::HashMap;
use Hconfig::Config::Config;
use crate::ModuleAbstract::ModuleAbstract;
use crate::OneLog::OneLog;
use anyhow::{Result,anyhow};
use crate::Type::Type;

pub struct CommandLine
{
	_name: String,
	_configs: HashMap<String, String>
}

impl CommandLine
{
	pub fn new() -> CommandLine {
		return CommandLine{
			_name: String::new(),
			_configs: HashMap::new()
		};
	}
	
	pub fn draw(&self,log: OneLog)
	{
		let colorStr;
		match log.level
		{
			Type::NOTICE => colorStr = "1;32",
			Type::NOTICEDERR => colorStr = "35",
			Type::WARNING => colorStr = "1;97;43",
			Type::DEBUGERR => colorStr = "1",
			Type::ERROR => colorStr = "1;97;41",
			Type::FATAL => colorStr = "1;97;45",
			_ => colorStr = "0"
		}
		
		let mut msg = log.message;
		if(msg.contains("\n") || msg.contains("\r"))
		{
			msg = msg.replace("\n\r","\n");
			msg = msg.replace("\r","\n");
			msg = msg.replace("\n","\n|");
		}
		
		println!("\x1b[3m{}\x1b[0m \x1b[{}m{}\x1b[0m",log.date.format("%H:%M:%S"),colorStr,msg);
	}
}

impl ModuleAbstract for CommandLine
{
	fn setModuleName(&mut self, moduleName: String) -> anyhow::Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn getModuleName(&self) -> anyhow::Result<String> {
		return Ok(self._name.clone());
	}
	
	fn setConfig(&mut self, configs: HashMap<String, String>) -> Result<()>
	{
		self._configs = configs;
		Ok(())
	}
	
	fn Event_onDebug(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onDebugErr(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNormal(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNotice(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNoticeErr(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onWarning(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onError(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onFatal(&self, log: OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onExit(&self) {
		todo!()
	}
	
}
