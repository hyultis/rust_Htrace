use crate::components::context::Context;
use crate::components::formater::FormaterCompiled;
use crate::components::trace::OneTrace;
use crate::modules::command_line_config::CommandLineConfig;
use crate::modules::module_abstract::ModuleAbstract;
use anyhow::Result;
use owo_colors::{OwoColorize, Style};
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct CommandLine
{
	_writeErrors: AtomicU64,
	_name: String,
	_configs: CommandLineConfig,
	_formaterCompiled: FormaterCompiled,
}

impl CommandLine
{
	pub fn new(config: CommandLineConfig) -> CommandLine
	{
		let binding = &config.formaterCompiler;
		let fmtComp = binding(&config.lineFormat);
		return CommandLine {
			_writeErrors: AtomicU64::new(0),
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
		};
	}

	pub fn draw(&self, trace: &OneTrace)
	{
		let color = *self
			._configs
			.colors
			.get(&trace.level)
			.unwrap_or(&Style::new());

		let binding = &self._configs.formaterParamBuilder;
		let mut parameters = binding(trace, &self._configs.lineReturn);
		parameters
			.get_mut("lvl")
			.iter_mut()
			.for_each(|x| **x = x.style(color).to_string());
		parameters
			.get_mut("msg")
			.iter_mut()
			.for_each(|x| **x = x.style(color).to_string());

		let line = format!("{}\n", self._formaterCompiled.render(parameters));
		let _ = self.writeLine(&mut io::stdout().lock(), line.as_bytes());
	}

	/// Cumulative stdout write/flush failures; diagnostics never recurse into Htrace.
	pub fn writeErrors(&self) -> u64
	{
		self._writeErrors.load(Ordering::Relaxed)
	}

	fn writeLine(&self, writer: &mut impl Write, line: &[u8]) -> io::Result<()>
	{
		writer.write_all(line).inspect_err(|_| {
			self._writeErrors.fetch_add(1, Ordering::Relaxed);
		})
	}
}

impl ModuleAbstract for CommandLine
{
	fn name_set(&mut self, moduleName: String) -> Result<()>
	{
		self._name = moduleName;
		return Ok(());
	}

	fn name_get(&self) -> Result<String>
	{
		return Ok(self._name.clone());
	}

	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onError(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onContextExit(&self, _: &Context) {}

	fn event_onGlobalExit(&self) {}

	fn flush(&self) -> Result<()>
	{
		io::stdout().lock().flush().inspect_err(|_| {
			self._writeErrors.fetch_add(1, Ordering::Relaxed);
		})?;
		if self.writeErrors() > 0
		{
			return Err(io::Error::other(
				"stdout write failed; inspect CommandLine::writeErrors()",
			)
			.into());
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn output_error_is_counted_without_panicking()
	{
		let output = CommandLine::new(CommandLineConfig::default());
		let mut full = io::Cursor::new([0u8; 2]);
		assert!(output.writeLine(&mut full, b"too long").is_err());
		assert_eq!(output.writeErrors(), 1);
	}
}
