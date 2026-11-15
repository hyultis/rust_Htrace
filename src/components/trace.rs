use crate::components::context::Context;
use crate::components::hbacktrace::Hbacktrace;
use crate::components::level::Level;
use std::borrow::Cow;
use std::sync::Arc;
use time::OffsetDateTime;

pub struct OneTrace
{
	pub message: String,
	pub date: OffsetDateTime,
	pub level: Level,
	/// Resolved snapshot, shared by every trace emitted under the same spans.
	pub context: Arc<Context>,
	/// Source file of the trace; static for macros and the `tracing` bridge.
	pub filename: Cow<'static, str>,
	pub fileline: u32,
	pub backtraces: Vec<Hbacktrace>,
}

impl OneTrace
{
	pub fn emit(&self)
	{
		self.context.modules_get().iter().for_each(|(_, module)| {
			if let Some(inner) = module
			{
				Level::launchModuleFunc(inner, self);
			}
		})
	}
}
