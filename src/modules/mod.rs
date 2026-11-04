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
pub mod utils_hconfig
{
	use Hconfig::tinyjson::JsonValue;
	use std::collections::HashMap;

	/// try to get a string config from a Hconfig, if it exists, check the condition
	/// if it exists and the condition is ok => set the module config value
	/// else update/add the Hconfig value
	pub fn setConfig_String(
		config: &mut HashMap<String, JsonValue>,
		key: &str,
		val: &mut String,
		condition: impl Fn(&str) -> bool,
	)
	{
		if let Some(JsonValue::String(tmp)) = config.get(key)
		{
			if condition(tmp.as_str())
			{
				*val = tmp.clone();
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
		if let Some(JsonValue::Boolean(tmp)) = config.get(key)
		{
			*val = *tmp;
			return;
		}
		config.insert(key.to_string(), JsonValue::Boolean(*val));
	}
}
#[cfg(all(test, feature = "hconfig"))]
mod config_tests
{
	use super::utils_hconfig::*;
	use Hconfig::tinyjson::JsonValue;
	use std::collections::HashMap;

	#[test]
	fn malformed_values_restore_defaults_without_panicking()
	{
		let mut config = HashMap::from([
			("path".into(), JsonValue::Boolean(false)),
			("enabled".into(), JsonValue::String("wrong".into())),
		]);
		let mut path = String::from("default");
		let mut enabled = true;
		setConfig_String(&mut config, "path", &mut path, |_| true);
		setConfig_boolean(&mut config, "enabled", &mut enabled);
		assert_eq!(path, "default");
		assert!(enabled);
		assert!(matches!(config.get("path"), Some(JsonValue::String(value)) if value == "default"));
		assert!(matches!(
			config.get("enabled"),
			Some(JsonValue::Boolean(true))
		));
	}
}
