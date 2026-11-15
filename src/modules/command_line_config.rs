use crate::components::formater::{
	FormaterCompile, FormaterCompilerSignature, FormaterParamBuilder, FormaterParamBuilderSignature,
};
use crate::components::level::Level;
use owo_colors::Style;
use std::collections::HashMap;

#[cfg(feature = "hconfig")]
use crate::Errors;
#[cfg(feature = "hconfig")]
use Hconfig::tinyjson::JsonValue;

pub struct CommandLineConfig
{
	/// color of level and message for each level
	pub colors: HashMap<Level, Style>,
	/// String added after a newline or carriage return ("\n"/"\r") in a trace.
	pub lineReturn: String,
	/// format of the trace, view HtraceDefaultFormater for available variable
	pub lineFormat: String,
	/// define the way to collect data (using lineReturn)
	pub formaterParamBuilder: FormaterParamBuilderSignature,
	/// define the way convert collected data into string (using lineFormat)
	pub formaterCompiler: FormaterCompilerSignature,
}

impl Default for CommandLineConfig
{
	fn default() -> Self
	{
		let mut colors = HashMap::new();

		colors.insert(Level::DEBUG, Style::new());
		colors.insert(Level::NOTICE, Style::new().green());
		colors.insert(Level::NOTICEDERR, Style::new().bright_green());
		colors.insert(Level::WARNING, Style::new().yellow());
		colors.insert(Level::DEBUGERR, Style::new().on_bright_red());
		colors.insert(Level::ERROR, Style::new().on_red().black());
		colors.insert(Level::FATAL, Style::new().on_purple().black());

		return CommandLineConfig {
			colors,
			lineReturn: " | ".to_string(),
			lineFormat: "{time} {lvl} ({thread:>, }{context:>, }{file}:l{line}) : {msg}"
				.to_string(),
			formaterParamBuilder: FormaterParamBuilder,
			formaterCompiler: FormaterCompile,
		};
	}
}

#[cfg(feature = "hconfig")]
impl CommandLineConfig
{
	/// Build a configuration from the `moduleKey` node of the loaded Hconfig
	/// `configName`, starting from `defaults` (see `create_from_hconfig`).
	///
	/// Returns `Errors::CannotFoundConfigNode("configName/moduleKey")` when the
	/// config `configName` is not loaded, and `Errors::CannotFoundConfigNode("moduleKey")`
	/// when the node cannot be reached. An absent node is created by Hconfig and
	/// filled with `defaults`.
	pub fn from_hconfig(configName: &str, moduleKey: &str, defaults: Self) -> Result<Self, Errors>
	{
		crate::modules::utils_hconfig::fromHconfig(
			configName,
			moduleKey,
			defaults,
			Self::create_from_hconfig,
		)
	}

	/// Apply the JSON object `configs` over `newConfig`. Missing or badly typed
	/// values keep the `newConfig` value, which is written back into `configs`.
	///
	/// Returns `Errors::ModuleConfigError("command_line", cause)` when `configs`
	/// is not a JSON object.
	pub fn create_from_hconfig(configs: &mut JsonValue, mut newConfig: Self)
	-> Result<Self, Errors>
	{
		use crate::modules::utils_hconfig::{notAnObject, setConfig_String};

		let JsonValue::Object(config) = configs
		else
		{
			return Err(notAnObject("command_line", configs));
		};

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

		setConfig_String(config, "lineReturn", &mut newConfig.lineReturn, |_| true);
		setConfig_String(config, "lineFormat", &mut newConfig.lineFormat, |a| {
			!a.contains("{color}")
		});

		Ok(newConfig)
	}
}
#[cfg(all(test, feature = "hconfig"))]
mod tests
{
	use super::*;

	#[test]
	fn object_node_is_applied_and_badly_typed_values_fall_back()
	{
		let mut json = JsonValue::Object(HashMap::from([
			("lineReturn".into(), JsonValue::String(" / ".into())),
			("lineFormat".into(), JsonValue::Boolean(true)),
		]));
		let config =
			CommandLineConfig::create_from_hconfig(&mut json, CommandLineConfig::default())
				.unwrap();
		assert_eq!(config.lineReturn, " / ");
		assert_eq!(config.lineFormat, CommandLineConfig::default().lineFormat);
		let JsonValue::Object(object) = json
		else
		{
			unreachable!()
		};
		assert!(
			matches!(object.get("lineFormat"), Some(JsonValue::String(value)) if *value == config.lineFormat)
		);
	}

	#[test]
	fn non_objects_are_module_config_errors()
	{
		for mut json in [
			JsonValue::Null,
			JsonValue::Number(2.0),
			JsonValue::Boolean(false),
			JsonValue::String("invalid".into()),
			JsonValue::Array(vec![]),
		]
		{
			let result =
				CommandLineConfig::create_from_hconfig(&mut json, CommandLineConfig::default());
			let Err(error @ Errors::ModuleConfigError(..)) = result
			else
			{
				panic!("expected a module configuration error")
			};
			let Errors::ModuleConfigError(module, _) = &error
			else
			{
				unreachable!()
			};
			assert_eq!(module, "command_line");
			assert!(error.to_string().starts_with("module 'command_line' configuration returned a error : configuration node must be a JSON object, found "));
			assert!(!matches!(json, JsonValue::Object(_)));
		}
	}
}
