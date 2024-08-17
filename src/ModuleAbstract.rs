use crate::OneLog::OneLog;
use anyhow::Result;
use Hconfig::rusty_json::base::{JsonObject, JsonValue};

pub trait ModuleAbstract: Sync + Send
{
	fn setModuleName(&mut self,moduleName: String) -> Result<()>;
	fn getModuleName(&self) -> Result<String>;
	
	fn setConfig(&mut self, configs: &mut JsonValue) -> Result<()>;
	
	fn Event_onDebug(&self, log: &OneLog);
	fn Event_onDebugErr(&self, log: &OneLog);
	fn Event_onNormal(&self, log: &OneLog);
	fn Event_onNotice(&self, log: &OneLog);
	fn Event_onNoticeErr(&self, log: &OneLog);
	fn Event_onWarning(&self, log: &OneLog);
	fn Event_onError(&self, log: &OneLog);
	fn Event_onFatal(&self, log: &OneLog);
	
	fn Event_onExit(&self);
}


pub fn setConfig_String(config: &mut JsonObject, key: &str, val: &mut String, cond: impl Fn(&String)->bool)
{
	if let Some(path) = config.get(key)
	{
		if let Ok(tmp) = path.parse()
		{
			if(cond(&tmp))
			{
				*val = tmp;
				return;
			}
		}
	}
	config.set(key, JsonValue::String(val.clone()));
}

pub fn setConfig_boolean(config: &mut JsonObject, key: &str, val: &mut bool)
{
	if let Some(path) = config.get(key)
	{
		if let Ok(tmp) = path.parse()
		{
			*val = tmp;
			return;
		}
	}
	config.set(key, JsonValue::Boolean(*val));
}
