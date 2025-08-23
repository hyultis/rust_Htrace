use std::collections::HashMap;
use owo_colors::Style;
use crate::components::formater::{FormaterCompile, FormaterCompilerSignature, FormaterParamBuilder, FormaterParamBuilderSignature};
use crate::components::level::Level;

#[cfg(feature = "hconfig")]
use Hconfig::tinyjson::JsonValue;

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
			lineFormat: "{time} {lvl} ({thread:>, }{context:>, }{file}:l{line}) : {msg}".to_string(),
			formaterParamBuilder: FormaterParamBuilder,
			formaterCompiler: FormaterCompile,
		};
	}
}

#[cfg(feature = "hconfig")]
impl CommandLineConfig
{
	pub fn create_from_hconfig(configs: &mut JsonValue) -> Self
	{
		use crate::modules::utils_hconfig::setConfig_String;
		let mut newConfig = Self::default();

		let JsonValue::Object(config) = configs else { return newConfig };

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

		setConfig_String(config,"lineReturn",&mut newConfig.lineReturn,|_|true);
		setConfig_String(config,"lineFormat",&mut newConfig.lineFormat,|a|{
			!a.contains("{color}")
		});

		newConfig
	}
}