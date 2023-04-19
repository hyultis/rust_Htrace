use crate::OneLog::OneLog;
use anyhow::Result;
use json::JsonValue;

pub trait ModuleAbstract: Sync + Send
{
	fn setModuleName(&mut self,moduleName: String) -> Result<()>;
	fn getModuleName(&self) -> Result<String>;
	
	fn setConfig(&mut self, configs: &mut JsonValue) -> Result<()>;
	
	fn Event_onDebug(&self, log: OneLog);
	fn Event_onDebugErr(&self, log: OneLog);
	fn Event_onNormal(&self, log: OneLog);
	fn Event_onNotice(&self, log: OneLog);
	fn Event_onNoticeErr(&self, log: OneLog);
	fn Event_onWarning(&self, log: OneLog);
	fn Event_onError(&self, log: OneLog);
	fn Event_onFatal(&self, log: OneLog);
	
	fn Event_onExit(&self);
}
