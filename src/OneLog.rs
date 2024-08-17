
use chrono::{Local};
use crate::backtrace::Hbacktrace;
use crate::Type::Type;

pub struct OneLog
{
	pub message: String,
	pub date: chrono::DateTime<Local>,
	pub level: Type,
	pub threadId: u64,
	pub filename: String,
	pub fileline: u32,
	pub backtraces: Vec<Hbacktrace>
}
