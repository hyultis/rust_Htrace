use std::fmt;
use crate::ModuleAbstract::ModuleAbstract;
use crate::OneLog::OneLog;

#[derive(Clone)]
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
			_ => "".to_string()
		}
	}
	
	pub fn launchModuleFunc(module: &Box<dyn ModuleAbstract>, onelog: OneLog)
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
			_ => module.Event_onNormal(onelog)
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
