
use chrono::{Local};
use crate::Type::Type;

#[derive(Clone)]
pub struct OneLog
{
	pub message: String,
	pub date: chrono::DateTime<Local>,
	pub level: Type,
	pub threadId: u64,
	pub filename: String,
	pub fileline: u32
}
