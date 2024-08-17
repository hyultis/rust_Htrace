use std::fmt;
use crate::ModuleAbstract::ModuleAbstract;
use crate::OneLog::OneLog;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Type
{
	DEBUG,
	NORMAL,
	NOTICE,
	NOTICEDERR,
	WARNING,
	DEBUGERR,
	ERROR,
	FATAL,
}

impl Type
{
	pub fn to_string(&self) -> String
	{
		match *self
		{
			Type::DEBUG => "DEBUG".to_string(),
			Type::DEBUGERR => "DEBUGERR".to_string(),
			Type::ERROR => "ERROR".to_string(),
			Type::FATAL => "FATAL".to_string(),
			Type::NOTICE => "NOTICE".to_string(),
			Type::NOTICEDERR => "NOTICEDERR".to_string(),
			Type::WARNING => "WARNING".to_string(),
			Type::NORMAL => "NORMAL".to_string()
		}
	}
	
	pub fn convert4LengthString(&self) -> String
	{
		match *self
		{
			Type::DEBUG => "DBUG".to_string(),
			Type::DEBUGERR => "ERRD".to_string(),
			Type::ERROR => "ERR ".to_string(),
			Type::FATAL => "FATA".to_string(),
			Type::NOTICE => "NOTI".to_string(),
			Type::NOTICEDERR => "NOER".to_string(),
			Type::WARNING => "WARN".to_string(),
			Type::NORMAL => "    ".to_string()
		}
	}
	
	pub fn tou8(&self) -> u8
	{
		match *self
		{
			Type::DEBUG => 0,
			Type::NORMAL => 1,
			Type::NOTICE => 2,
			Type::DEBUGERR => 3,
			Type::WARNING => 4,
			Type::NOTICEDERR => 5,
			Type::ERROR => 6,
			Type::FATAL => 7,
		}
	}
	
	pub fn launchModuleFunc(module: &Box<dyn ModuleAbstract + Send + Sync>, onelog: &OneLog)
	{
		match onelog.level
		{
			Type::DEBUG => module.Event_onDebug(onelog),
			Type::DEBUGERR => module.Event_onDebugErr(onelog),
			Type::ERROR => module.Event_onError(onelog),
			Type::FATAL => module.Event_onFatal(onelog),
			Type::NOTICE => module.Event_onNotice(onelog),
			Type::NOTICEDERR => module.Event_onNoticeErr(onelog),
			Type::WARNING => module.Event_onWarning(onelog),
			Type::NORMAL => module.Event_onNormal(onelog)
		}
	}
}

impl fmt::Display for Type {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// The `f` value implements the `Write` trait, which is what the
		// write! macro is expecting. Note that this formatting ignores the
		// various flags provided to format strings.
		write!(f, "{:<4}", self.convert4LengthString())
	}
}

impl From<u8> for Type
{
	fn from(value: u8) -> Self {
		match value
		{
			0 => Type::DEBUG,
			1 => Type::NOTICE,
			2 => Type::NORMAL,
			3 => Type::NOTICEDERR,
			4 => Type::WARNING,
			5 => Type::ERROR,
			6 => Type::FATAL ,
			_ => Type::DEBUG
		}
	}
}
