use std::fmt::{Display, Formatter};

pub struct Backtrace
{
	pub funcName: String,
	pub fileName: String,
	pub line: u32,
}

impl Display for Backtrace
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}({}): {}", self.fileName, self.line, self.funcName)
	}
}
