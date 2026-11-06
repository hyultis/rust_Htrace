use crate::components::formater::{
	FormaterCompile, FormaterCompilerSignature, FormaterParamBuilder, FormaterParamBuilderSignature,
};

#[cfg(feature = "hconfig")]
use Hconfig::tinyjson::JsonValue;

/// note: if byThreadId is false, bySrc is false and forceInOneFile is None, no trace will be written
pub struct FileConfig
{
	/// base path where to write files
	pub path: String,
	/// String added after a newline or carriage return ("\n"/"\r") in a trace.
	pub lineReturn: String,
	/// format of the trace, view HtraceDefaultFormater for available variable
	pub lineFormat: String,
	/// write a file by thread name
	pub byThreadId: bool,
	/// write a file by src of trace
	pub bySrc: bool,
	/// each file wrote is by hour (add hour after date in filename)
	pub byHour: bool,
	/// write all trace in one file (auto append "_{time}.trc")
	pub forceInOneFile: Option<String>,
	/// Retain trace files for this many days. None or Some(0) disables retention.
	pub purgeAfterDays: Option<u32>,
	/// Delete all existing direct regular .trc files, regardless of age, once at logger startup.
	/// Disabled by default.
	pub purgeOnStartup: bool,
	/// define the way to collect data (using lineReturn)
	pub formaterParamBuilder: FormaterParamBuilderSignature,
	/// define the way convert collected data into string (using lineFormat)
	pub formaterCompiler: FormaterCompilerSignature,
}

impl Default for FileConfig
{
	fn default() -> Self
	{
		return FileConfig {
			path: "./traces".to_string(),
			lineReturn: " | ".to_string(),
			lineFormat: "{time} {lvl} ({thread:>, }{context:>, }{file}:l{line}) : {msg}"
				.to_string(),
			byThreadId: true,
			bySrc: false,
			byHour: false,
			forceInOneFile: None,
			purgeAfterDays: None,
			purgeOnStartup: false,
			formaterParamBuilder: FormaterParamBuilder,
			formaterCompiler: FormaterCompile,
		};
	}
}

#[cfg(feature = "hconfig")]
impl FileConfig
{
	pub fn create_from_hconfig(configs: &mut JsonValue, mut newConfig: Self) -> Self
	{
		use crate::modules::utils_hconfig::{setConfig_String, setConfig_boolean};
		use Hconfig::tinyjson::JsonValue;

		newConfig.purgeAfterDays = newConfig.purgeAfterDays.filter(|days| *days > 0);
		let JsonValue::Object(config) = configs
		else
		{
			return newConfig;
		};
		setConfig_String(config, "path", &mut newConfig.path, |_| true);
		setConfig_String(config, "lineReturn", &mut newConfig.lineReturn, |_| true);
		setConfig_String(config, "lineFormat", &mut newConfig.lineFormat, |_| true);
		setConfig_boolean(config, "byHour", &mut newConfig.byHour);
		setConfig_boolean(config, "bySrc", &mut newConfig.bySrc);
		setConfig_boolean(config, "byThreadId", &mut newConfig.byThreadId);
		setConfig_boolean(config, "purgeOnStartup", &mut newConfig.purgeOnStartup);
		match config.get("purgeAfterDays")
		{
			Some(JsonValue::Null) => newConfig.purgeAfterDays = None,
			Some(JsonValue::Number(days))
				if days.is_finite()
					&& days.fract() == 0.0
					&& *days >= 0.0
					&& *days <= u32::MAX as f64 =>
			{
				newConfig.purgeAfterDays = Some(*days as u32).filter(|days| *days > 0);
			}
			_ =>
			{
				config.insert(
					"purgeAfterDays".to_string(),
					newConfig
						.purgeAfterDays
						.map_or(JsonValue::Null, |days| JsonValue::Number(days as f64)),
				);
			}
		}

		if let Some(JsonValue::String(tmp)) = config.get("forceInOneFile")
		{
			if (tmp == "")
			{
				newConfig.forceInOneFile = None;
			}
			else
			{
				newConfig.forceInOneFile = Some(tmp.to_string());
			}
		}
		else if !config.contains_key("forceInOneFile")
		{
			config.insert(
				"forceInOneFile".to_string(),
				JsonValue::String(newConfig.forceInOneFile.clone().unwrap_or("".to_string())),
			);
		}

		return newConfig;
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn retention_is_disabled_by_default()
	{
		let config = FileConfig::default();
		assert_eq!(config.purgeAfterDays, None);
		assert!(!config.purgeOnStartup);
	}

	#[cfg(feature = "hconfig")]
	mod hconfig
	{
		use super::*;
		use std::collections::HashMap;

		fn defaults() -> FileConfig
		{
			FileConfig {
				purgeAfterDays: Some(7),
				purgeOnStartup: true,
				..FileConfig::default()
			}
		}

		#[test]
		fn accepts_null_zero_and_positive_integer_days()
		{
			for (value, expected) in [
				(JsonValue::Null, None),
				(JsonValue::Number(0.0), None),
				(JsonValue::Number(1.0), Some(1)),
				(JsonValue::Number(u32::MAX as f64), Some(u32::MAX)),
			]
			{
				let mut json = JsonValue::Object(HashMap::from([("purgeAfterDays".into(), value)]));
				let config = FileConfig::create_from_hconfig(&mut json, defaults());
				assert_eq!(config.purgeAfterDays, expected);
			}
		}

		#[test]
		fn invalid_or_absent_days_restore_and_serialize_custom_default()
		{
			for value in [
				None,
				Some(JsonValue::Boolean(true)),
				Some(JsonValue::String("7".into())),
				Some(JsonValue::Array(vec![])),
				Some(JsonValue::Object(HashMap::new())),
				Some(JsonValue::Number(-1.0)),
				Some(JsonValue::Number(1.5)),
				Some(JsonValue::Number(u32::MAX as f64 + 1.0)),
				Some(JsonValue::Number(f64::NAN)),
				Some(JsonValue::Number(f64::INFINITY)),
				Some(JsonValue::Number(f64::NEG_INFINITY)),
			]
			{
				let mut object = HashMap::new();
				if let Some(value) = value
				{
					object.insert("purgeAfterDays".into(), value);
				}
				let mut json = JsonValue::Object(object);
				let config = FileConfig::create_from_hconfig(&mut json, defaults());
				assert_eq!(config.purgeAfterDays, Some(7));
				let JsonValue::Object(object) = json
				else
				{
					unreachable!()
				};
				assert!(matches!(
					object.get("purgeAfterDays"),
					Some(JsonValue::Number(7.0))
				));
			}
		}

		#[test]
		fn disabled_defaults_are_normalized_and_serialized_as_null()
		{
			for defaultDays in [None, Some(0)]
			{
				for value in [None, Some(JsonValue::String("invalid".into()))]
				{
					let mut object = HashMap::new();
					if let Some(value) = value
					{
						object.insert("purgeAfterDays".into(), value);
					}
					let mut json = JsonValue::Object(object);
					let config = FileConfig::create_from_hconfig(
						&mut json,
						FileConfig {
							purgeAfterDays: defaultDays,
							..defaults()
						},
					);
					assert_eq!(config.purgeAfterDays, None);
					let JsonValue::Object(object) = json
					else
					{
						unreachable!()
					};
					assert!(matches!(
						object.get("purgeAfterDays"),
						Some(JsonValue::Null)
					));
				}
			}
		}

		#[test]
		fn startup_boolean_uses_configured_value_or_custom_default()
		{
			for (value, expected) in [
				(None, true),
				(Some(JsonValue::Boolean(false)), false),
				(Some(JsonValue::Boolean(true)), true),
				(Some(JsonValue::Null), true),
				(Some(JsonValue::String("false".into())), true),
				(Some(JsonValue::Number(0.0)), true),
				(Some(JsonValue::Array(vec![])), true),
				(Some(JsonValue::Object(HashMap::new())), true),
			]
			{
				let mut object = HashMap::new();
				if let Some(value) = value
				{
					object.insert("purgeOnStartup".into(), value);
				}
				let mut json = JsonValue::Object(object);
				let config = FileConfig::create_from_hconfig(&mut json, defaults());
				assert_eq!(config.purgeOnStartup, expected);
				let JsonValue::Object(object) = json
				else
				{
					unreachable!()
				};
				assert!(
					matches!(object.get("purgeOnStartup"), Some(JsonValue::Boolean(value)) if *value == expected)
				);
			}
		}

		#[test]
		fn malformed_force_in_one_file_does_not_skip_retention()
		{
			let mut json = JsonValue::Object(HashMap::from([
				("forceInOneFile".into(), JsonValue::Boolean(false)),
				("purgeAfterDays".into(), JsonValue::Number(3.0)),
				("purgeOnStartup".into(), JsonValue::Boolean(true)),
			]));
			let config = FileConfig::create_from_hconfig(&mut json, FileConfig::default());
			assert_eq!(config.purgeAfterDays, Some(3));
			assert!(config.purgeOnStartup);
		}

		#[test]
		fn non_objects_preserve_custom_defaults_and_normalize_zero()
		{
			for defaultDays in [None, Some(0), Some(7)]
			{
				for mut json in [
					JsonValue::Null,
					JsonValue::Number(2.0),
					JsonValue::Boolean(false),
					JsonValue::String("invalid".into()),
					JsonValue::Array(vec![]),
				]
				{
					let config = FileConfig::create_from_hconfig(
						&mut json,
						FileConfig {
							purgeAfterDays: defaultDays,
							..defaults()
						},
					);
					assert_eq!(config.purgeAfterDays, defaultDays.filter(|days| *days > 0));
					assert!(config.purgeOnStartup);
					assert!(!matches!(json, JsonValue::Object(_)));
				}
			}
		}
	}
}
