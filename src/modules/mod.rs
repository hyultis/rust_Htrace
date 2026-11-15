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

	/// Look up `moduleKey` in the loaded Hconfig `configName` and apply `create` to it.
	/// A missing config is reported as `CannotFoundConfigNode("configName/moduleKey")`;
	/// a key that cannot be reached (a parent node is not an object) as
	/// `CannotFoundConfigNode("moduleKey")`. A missing key is created by Hconfig.
	pub(crate) fn fromHconfig<T>(
		configName: &str,
		moduleKey: &str,
		defaults: T,
		create: impl FnOnce(&mut JsonValue, T) -> Result<T, crate::Errors>,
	) -> Result<T, crate::Errors>
	{
		use crate::Errors;
		use Hconfig::HConfigManager::HConfigManager;

		let Some(mut config) = HConfigManager::singleton().get(configName)
		else
		{
			return Err(Errors::CannotFoundConfigNode(format!(
				"{configName}/{moduleKey}"
			)));
		};
		// Hconfig 2.0.2 can panic on an array index past its end (`arr/5/x`):
		// treat that as an unreachable node instead of propagating the panic.
		let reachable = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			config.value_get_mut(moduleKey).is_some()
		}))
		.unwrap_or(false);
		let node = match reachable
		{
			true => config.value_get_mut(moduleKey),
			false => None,
		};
		let Some(node) = node
		else
		{
			return Err(Errors::CannotFoundConfigNode(moduleKey.to_string()));
		};
		create(node, defaults)
	}

	/// Error returned when a module configuration node is not a JSON object.
	pub(crate) fn notAnObject(module: &str, node: &JsonValue) -> crate::Errors
	{
		let kind = match node
		{
			JsonValue::Null => "null",
			JsonValue::Boolean(_) => "a boolean",
			JsonValue::Number(_) => "a number",
			JsonValue::String(_) => "a string",
			JsonValue::Array(_) => "an array",
			JsonValue::Object(_) => "an object",
		};
		crate::Errors::ModuleConfigError(
			module.to_string(),
			anyhow::anyhow!("configuration node must be a JSON object, found {kind}"),
		)
	}

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
