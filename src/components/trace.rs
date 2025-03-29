use time::OffsetDateTime;
use crate::components::backtrace::Backtrace;
use crate::components::context::Context;
use crate::components::level::Level;

pub struct OneTrace
{
	pub message: String,
	pub date: OffsetDateTime,
	pub level: Level,
	pub context: Context,
	pub filename: String,
	pub fileline: u32,
	pub backtraces: Vec<Backtrace>
}

impl OneTrace
{
	pub fn emit(&self)
	{
		self.context.modules_get().iter().for_each(|(_,module)| {
			if let Some(inner) = module
			{
				Level::launchModuleFunc(inner,self);
			}
		})
	}
}
