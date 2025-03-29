use crate::components::trace::OneTrace;
use anyhow::Result;
use Hconfig::tinyjson::JsonValue;
use crate::components::context::Context;

pub trait ModuleAbstract: Sync + Send
{
	fn name_set(&mut self, moduleName: String) -> Result<()>;
	fn name_get(&self) -> Result<String>;
	
	fn config_set(&mut self, configs: &mut JsonValue) -> Result<()>;
	
	fn event_onDebug(&self, trace: &OneTrace);
	fn event_onDebugErr(&self, trace: &OneTrace);
	fn event_onNormal(&self, trace: &OneTrace);
	fn event_onNotice(&self, trace: &OneTrace);
	fn event_onNoticeErr(&self, trace: &OneTrace);
	fn event_onWarning(&self, trace: &OneTrace);
	fn event_onError(&self, trace: &OneTrace);
	fn event_onFatal(&self, trace: &OneTrace);

	fn event_onContextExit(&self, context: &Context);
	
	fn event_onGlobalExit(&self);
}