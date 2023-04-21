use std::collections::HashMap;
use crate::ModuleAbstract::ModuleAbstract;
use crate::OneLog::OneLog;
use anyhow::Result;
use json::JsonValue;
use strfmt::strfmt;
use crate::Type::Type;

pub struct CommandLineConfig
{
	pub colors: HashMap<Type,String>,
	pub lineReturn: String,
	pub lineFormat: String
}

impl Default for CommandLineConfig
{
	fn default() -> Self {
		let mut colors = HashMap::new();
		
		colors.insert(Type::DEBUG,"0".to_string());
		colors.insert(Type::NOTICE,"1;32".to_string());
		colors.insert(Type::NOTICEDERR,"35".to_string());
		colors.insert(Type::WARNING,"1;97;43".to_string());
		colors.insert(Type::DEBUGERR,"1".to_string());
		colors.insert(Type::ERROR,"1;97;41".to_string());
		colors.insert(Type::FATAL,"1;97;45".to_string());
		
		return CommandLineConfig{
			colors,
			lineReturn: "|".to_string(),
			lineFormat: "\x1b[3m{time}\x1b[0m {lvl} ({file}:l{line}) : \x1b[{color}m{msg}\x1b[0m".to_string(),
		};
	}
}

pub struct CommandLine
{
	_name: String,
	_configs: CommandLineConfig
}

impl CommandLine
{
	pub fn new(configs: CommandLineConfig) -> CommandLine {
		return CommandLine{
			_name: String::new(),
			_configs: configs
		};
	}
	
	pub fn draw(&self,log: OneLog)
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
		
		let mut vars = HashMap::new();
		vars.insert("time".to_string(),log.date.format("%H:%M:%S").to_string());
		vars.insert("color".to_string(),self._configs.colors.get(&log.level).unwrap_or(&"0".to_string()).to_string());
		vars.insert("lvl".to_string(),log.level.convert4LengthString());
		vars.insert("file".to_string(),log.filename);
		vars.insert("line".to_string(),log.fileline.to_string());
		vars.insert("msg".to_string(),msg);
		
		let format = self._configs.lineFormat.clone();
		println!("{}",strfmt(&format, &vars).unwrap());
	}
}

impl ModuleAbstract for CommandLine
{
	fn setModuleName(&mut self, moduleName: String) -> Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn getModuleName(&self) -> Result<String> {
		return Ok(self._name.clone());
	}
	
	fn setConfig(&mut self, configs: &mut JsonValue) -> Result<()>
	{
		if(!configs.contains("colors"))
		{
			configs["colors"] = JsonValue::new_object();
		}
		
			let allcolor = [Type::DEBUG,
			Type::DEBUGERR,
			Type::ERROR,
			Type::FATAL,
			Type::NOTICE,
			Type::NOTICEDERR,
			Type::WARNING,
			Type::NORMAL];
			
			for i in allcolor
			{
				if(configs["colors"][&i.to_string()].is_string())
				{
					self._configs.colors.insert(i, configs["colors"][&i.to_string()].to_string());
				}
				else
				{
					configs["colors"][&i.to_string()] = JsonValue::String(self._configs.colors.get(&i).unwrap_or(&"0".to_string()).to_string());
				}
			}
		
		
		if(!configs.contains("lineReturn"))
		{
			configs["lineReturn"] = JsonValue::String(self._configs.lineReturn.to_string());
		}
		else
		{
			self._configs.lineReturn = configs["lineReturn"].to_string();
		}
			
		if(!configs.contains("lineFormat"))
		{
			configs["lineFormat"] = JsonValue::String(self._configs.lineFormat.to_string());
		}
		else
		{
			self._configs.lineFormat = configs["lineFormat"].to_string();
		}
		
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
	
	}
	
}
