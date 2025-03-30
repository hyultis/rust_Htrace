use std::collections::HashMap;
use crate::modules::module_abstract::ModuleAbstract;
use crate::components::trace::OneTrace;
use anyhow::Result;
use Hconfig::tinyjson::JsonValue;
use owo_colors::{OwoColorize, Style};
use crate::components::context::Context;
use crate::components::formater::{FormaterParamBuilderSignature, FormaterCompilerSignature, FormaterParamBuilder, FormaterCompile, FormaterCompiled};
use crate::components::level::Level;
use crate::modules::utils::setConfig_String;

pub struct CommandLineConfig
{
	/// color of level and message for each level
	pub colors: HashMap<Level,Style>,
	/// adding string when a trace have à return char "\n"/"\r"
	pub lineReturn: String,
	/// format of the trace, view HtraceDefaultFormater for available variable
	pub lineFormat: String,
	/// define the way to collect data (using lineReturn)
	pub formaterParamBuilder: FormaterParamBuilderSignature,
	/// define the way convert collected data into string (using lineFormat)
	pub formaterCompiler: FormaterCompilerSignature
}

impl Default for CommandLineConfig
{
	fn default() -> Self {
		let mut colors = HashMap::new();
		
		colors.insert(Level::DEBUG, Style::new());
		colors.insert(Level::NOTICE, Style::new().green());
		colors.insert(Level::NOTICEDERR, Style::new().bright_green());
		colors.insert(Level::WARNING, Style::new().yellow());
		colors.insert(Level::DEBUGERR, Style::new().on_bright_red());
		colors.insert(Level::ERROR, Style::new().on_red().black());
		colors.insert(Level::FATAL, Style::new().on_purple().black());

		return CommandLineConfig{
			colors,
			lineReturn: " | ".to_string(),
			lineFormat: "{time} {lvl} ({thread:>, }{file}:l{line}) : {msg}".to_string(),
			formaterParamBuilder: FormaterParamBuilder,
			formaterCompiler: FormaterCompile,
		};
	}
}

pub struct CommandLine
{
	_name: String,
	_configs: CommandLineConfig,
	_formaterCompiled: FormaterCompiled,
}

impl CommandLine
{
	pub fn new(config: CommandLineConfig) -> CommandLine {
		let binding = &config.formaterCompiler;
		let fmtComp = binding(&config.lineFormat);
		return CommandLine{
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
		};
	}
	
	pub fn draw(&self,trace: &OneTrace)
	{
		let color = self._configs.colors.get(&trace.level).unwrap_or(&Style::new()).clone();

		let binding = &self._configs.formaterParamBuilder;
		let mut parameters = binding(trace, &self._configs.lineReturn);
		parameters.get_mut("lvl").iter_mut().for_each(|x| **x = x.style(color).to_string());
		parameters.get_mut("msg").iter_mut().for_each(|x| **x = x.style(color).to_string());

		println!("{}",self._formaterCompiled.render(parameters));
	}
}

impl ModuleAbstract for CommandLine
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
		let JsonValue::Object(config) = configs else { return Ok(()) };
		
		/* removed until good way to save style internal stuff
		if (!config.contains_key("colors"))
		{
			config.set("colors", Value::Object(JsonObject::new()));
		}
		
		if let Some(colorsCase) = config.get_mut("colors")
		{
			if let Value::Array(colorArray) = colorsCase
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
						colorArray.insert(i as usize, Value::String(self._configs.colors.get(&i).unwrap_or(&Style::new()).clone()));
					}
				}
			}
		}*/

		setConfig_String(config,"lineReturn",&mut self._configs.lineReturn,|_|true);
		setConfig_String(config,"lineFormat",&mut self._configs.lineFormat,|a|{
			!a.contains("{color}")
		});

		// update formater
		let binding = &self._configs.formaterCompiler;
		let fmtComp = binding(&self._configs.lineFormat);
		self._formaterCompiled = fmtComp;
		
		Ok(())
	}
	
	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onError(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onContextExit(&self, _: &Context) {}
	
	fn event_onGlobalExit(&self) {
	
	}
	
}
