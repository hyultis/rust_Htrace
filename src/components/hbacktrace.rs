use std::fmt::{Display, Formatter};

pub struct Hbacktrace
{
	pub funcName: String,
	pub fileName: Option<String>,
	pub line: Option<u32>,
}

impl Display for Hbacktrace
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		if let Some(file) = &self.fileName && let Some(line) = &self.line
		{
			return write!(f, "{}({}): {}", file, line, self.funcName);
		}

		write!(f, "- {}", self.funcName)
	}
}
