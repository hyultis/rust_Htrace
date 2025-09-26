use std::fmt;
use std::sync::Arc;
use crate::modules::module_abstract::ModuleAbstract;
use crate::components::trace::OneTrace;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Level
{
	/// Lowest trace level, used for debugging information during development.
	DEBUG,

	/// Debug information specific to errors, useful when reproducing errors.
	DEBUGERR,

	/// Standard trace level for normal operations.
	NORMAL,

	/// More pronounced trace level, for events that deserve special attention but are not errors.
	NOTICE,

	/// Warning or error reported by another system (e.g., an email).
	NOTICEDERR,

	/// Trace level indicating a situation that requires special attention (e.g., low disk space, undeletable file, etc.).
	WARNING,

	/// Trace level indicating an error that must be manually corrected.
	ERROR,

	/// Trace level indicating a fatal error that prevents the system from functioning normally.
	FATAL,
}

impl Level
{
	pub fn to_string(&self) -> String
	{
		match *self
		{
			Level::DEBUG => "DEBUG".to_string(),
			Level::DEBUGERR => "DEBUGERR".to_string(),
			Level::ERROR => "ERROR".to_string(),
			Level::FATAL => "FATAL".to_string(),
			Level::NOTICE => "NOTICE".to_string(),
			Level::NOTICEDERR => "NOTICEDERR".to_string(),
			Level::WARNING => "WARNING".to_string(),
			Level::NORMAL => "NORMAL".to_string()
		}
	}
	
	pub fn convert4LengthString(&self) -> String
	{
		match *self
		{
			Level::DEBUG => "DBUG".to_string(),
			Level::DEBUGERR => "ERRD".to_string(),
			Level::ERROR => "ERR ".to_string(),
			Level::FATAL => "FATA".to_string(),
			Level::NOTICE => "NOTI".to_string(),
			Level::NOTICEDERR => "NOER".to_string(),
			Level::WARNING => "WARN".to_string(),
			Level::NORMAL => "    ".to_string()
		}
	}
	
	pub fn tou8(&self) -> u8
	{
		match *self
		{
			Level::DEBUG => 0,
			Level::DEBUGERR => 1,
			Level::NORMAL => 2,
			Level::NOTICE => 3,
			Level::NOTICEDERR => 4,
			Level::WARNING => 5,
			Level::ERROR => 6,
			Level::FATAL => 7,
		}
	}
	
	pub fn launchModuleFunc(module: &Arc<dyn ModuleAbstract>, oneTrace: &OneTrace)
	{
		match oneTrace.level
		{
			Level::DEBUG => module.event_onDebug(oneTrace),
			Level::DEBUGERR => module.event_onDebugErr(oneTrace),
			Level::ERROR => module.event_onError(oneTrace),
			Level::FATAL => module.event_onFatal(oneTrace),
			Level::NOTICE => module.event_onNotice(oneTrace),
			Level::NOTICEDERR => module.event_onNoticeErr(oneTrace),
			Level::WARNING => module.event_onWarning(oneTrace),
			Level::NORMAL => module.event_onNormal(oneTrace)
		}
	}

	/// return the lowest possible level (DEBUG)
	pub fn min() -> Level
	{
		Level::DEBUG
	}

	/// return max possible level (FATAL)
	pub fn max() -> Level
	{
		Level::FATAL
	}
}

impl fmt::Display for Level {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// The `f` value implements the `Write` trait, which is what the
		// write! macro is expecting. Note that this formatting ignores the
		// various flags provided to format strings.
		write!(f, "{:<4}", self.convert4LengthString())
	}
}

impl From<u8> for Level
{
	fn from(value: u8) -> Self {
		match value
		{
			0 => Level::DEBUG,
			1 => Level::DEBUGERR,
			2 => Level::NORMAL,
			3 => Level::NOTICE,
			4 => Level::NOTICEDERR,
			5 => Level::WARNING,
			6 => Level::ERROR,
			_ => Level::FATAL
		}
	}
}
