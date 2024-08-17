use std::collections::HashMap;
use crate::ModuleAbstract::{ModuleAbstract, setConfig_String};
use crate::OneLog::OneLog;
use anyhow::Result;
use Hconfig::rusty_json::base::JsonValue;
use owo_colors::{OwoColorize, Style};
use strfmt::strfmt;
use crate::Type::Type;

pub struct CommandLineConfig
{
	pub colors: HashMap<Type,Style>,
	pub lineReturn: String,
	pub lineFormat: String
}

impl Default for CommandLineConfig
{
	fn default() -> Self {
		let mut colors = HashMap::new();
		
		colors.insert(Type::DEBUG,Style::new());
		colors.insert(Type::NOTICE,Style::new().green());
		colors.insert(Type::NOTICEDERR,Style::new().bright_green());
		colors.insert(Type::WARNING,Style::new().yellow());
		colors.insert(Type::DEBUGERR,Style::new().on_bright_red());
		colors.insert(Type::ERROR,Style::new().on_red().black());
		colors.insert(Type::FATAL,Style::new().on_purple().black());
		
		return CommandLineConfig{
			colors,
			lineReturn: "|".to_string(),
			lineFormat: "{time} {lvl} ({file}:l{line}) : {msg}".to_string(),
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
	
	pub fn draw(&self,log: &OneLog)
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
				drawBacktraces = format!("{}\n | {}",drawBacktraces,one.to_string());
			});
			
			msg = format!("{}, with : {}",msg,drawBacktraces)
		}
		
		let color = self._configs.colors.get(&log.level).unwrap_or(&Style::new()).clone();
		
		let mut vars = HashMap::new();
		vars.insert("time".to_string(),log.date.format("%H:%M:%S%.6f").to_string());
		vars.insert("lvl".to_string(),log.level.convert4LengthString().style(color).to_string());
		vars.insert("file".to_string(),log.filename.clone());
		vars.insert("line".to_string(),log.fileline.to_string());
		
		vars.insert("msg".to_string(),msg.style(color).to_string());
		
		let format = self._configs.lineFormat.clone();
		if let Ok(result)=strfmt(&format, &vars)
		{
			println!("{}",result);
		}
		else
		{
			println!("{}",strfmt(&"{time} {lvl} ({file}:l{line}) : {msg}", &vars).unwrap());
		}
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
		let JsonValue::Object(config) = configs else { return Ok(()) };
		
		/* removed until good way to save style internal stuff
		if (!config.contains_key("colors"))
		{
			config.set("colors", JsonValue::Object(JsonObject::new()));
		}
		
		if let Some(colorsCase) = config.get_mut("colors")
		{
			if let JsonValue::Array(colorArray) = colorsCase
			{
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
					if let Some(colorData) = colorArray.get(i as usize)
					{
						if let Ok(val) = colorData.parse::<String>()
						{
							
							self._configs.colors.insert(i, val);
						}
					}
					else
					{
						let tmp = Style::new();
						println!("{}",serde_json::to_string(&tmp).unwrap());
						colorArray.insert(i as usize, JsonValue::String(self._configs.colors.get(&i).unwrap_or(&Style::new()).clone()));
					}
				}
			}
		}*/
		
		
		setConfig_String(config,"lineReturn",&mut self._configs.lineReturn,|_|true);
		setConfig_String(config,"lineFormat",&mut self._configs.lineFormat,|a|{
			!a.contains("{color}")
		});
		
		Ok(())
	}
	
	fn Event_onDebug(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onDebugErr(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNormal(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNotice(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onNoticeErr(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onWarning(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onError(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onFatal(&self, log: &OneLog)
	{
		self.draw(log);
	}
	
	fn Event_onExit(&self) {
	
	}
	
}
