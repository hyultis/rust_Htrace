
pub mod module_abstract;

#[cfg(feature = "default_module")]
pub mod file;
#[cfg(feature = "default_module")]
pub mod file_config;

#[cfg(feature = "default_module")]
pub mod command_line;
#[cfg(feature = "default_module")]
pub mod command_line_config;

#[cfg(feature = "hconfig")]
pub mod utils_hconfig {
	use std::collections::HashMap;
	use Hconfig::tinyjson::JsonValue;

	/// try to get a string config from a Hconfig, if it exists, check the condition
	/// if it exists and the condition is ok => set the module config value
	/// else update/add the Hconfig value
	pub fn setConfig_String(config: &mut HashMap<String, JsonValue>, key: &str, val: &mut String, condition: impl Fn(&str) -> bool)
	{
		if let Some(path) = config.get(key)
		{
			let tmp: String = path.clone().try_into().unwrap();
			if (condition(tmp.as_str()))
			{
				*val = tmp;
				return;
			}
		}
		config.insert(key.to_string(), JsonValue::String(val.clone()));
	}

	/// try to get a boolean config from a Hconfig, if it exists
	/// if it exists => set the module config value
	/// else update/add the Hconfig value
	pub fn setConfig_boolean(config: &mut HashMap<String, JsonValue>, key: &str, val: &mut bool)
	{
		if let Some(path) = config.get(key)
		{
			let tmp: &bool = path.get().unwrap();
			*val = *tmp;
			return;
		}
		config.insert(key.to_string(), JsonValue::Boolean(*val));
	}
}