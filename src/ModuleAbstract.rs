use crate::OneLog::OneLog;
use anyhow::Result;
use Hconfig::serde_json;
use Hconfig::serde_json::Value;

pub trait ModuleAbstract: Sync + Send
{
	fn setModuleName(&mut self,moduleName: String) -> Result<()>;
	fn getModuleName(&self) -> Result<String>;
	
	fn setConfig(&mut self, configs: &mut Value) -> Result<()>;
	
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


pub fn setConfig_String(config: &mut serde_json::Map<String,Value>, key: &str, val: &mut String, cond: impl Fn(&str)->bool)
{
	if let Some(path) = config.get(key)
	{
		if let Some(tmp) = path.as_str()
		{
			if(cond(&tmp))
			{
				*val = tmp.to_string();
				return;
			}
		}
	}
	config.insert(key.to_string(), Value::String(val.clone()));
}

pub fn setConfig_boolean(config: &mut serde_json::Map<String,Value>, key: &str, val: &mut bool)
{
	if let Some(path) = config.get(key)
	{
		if let Some(tmp) = path.as_bool()
		{
			*val = tmp;
			return;
		}
	}
	config.insert(key.to_string(), Value::Bool(*val));
}
