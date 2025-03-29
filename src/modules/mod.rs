
pub mod file;
pub mod module_abstract;
pub mod command_line;

pub mod utils {
	use std::collections::HashMap;
	use Hconfig::tinyjson::JsonValue;

	pub fn setConfig_String(config: &mut HashMap<String, JsonValue>, key: &str, val: &mut String, cond: impl Fn(&str) -> bool)
	{
		if let Some(path) = config.get(key)
		{
			let tmp: String = path.clone().try_into().unwrap();
			if (cond(tmp.as_str()))
			{
				*val = tmp;
				return;
			}
		}
		config.insert(key.to_string(), JsonValue::String(val.clone()));
	}

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