use crate::components::level::Level;
use crate::crates::bridge::HtraceBridge;
use crate::htracer;
use log::{Level as LogLevel, Log, Metadata, Record};

impl Log for HtraceBridge
{
	fn enabled(&self, metadata: &Metadata) -> bool
	{
		let level = LogLevelToHtraceMapper(&metadata.level());
		self.isLog(&level) && htracer::HTracer::enabled(level)
	}

	fn log(&self, record: &Record)
	{
		let convertedLevel = LogLevelToHtraceMapper(&record.level());
		if !self.isLog(&convertedLevel) || !htracer::HTracer::enabled(convertedLevel)
		{
			return;
		}
		let file = record.file().unwrap_or("unknown");
		let line = record.line().unwrap_or(0);
		let arg = format!("{}", record.args());

		let mut backtrace = vec![];
		if (self.isBacktrace(&convertedLevel))
		{
			backtrace = htracer::HTracer::backtrace(file);
		}

		htracer::HTracer::trace(&arg, convertedLevel, file, line, backtrace);
	}

	fn flush(&self)
	{
		// log::Log cannot return errors; diagnostics stay available on HTracer.
		let _ = htracer::HTracer::flush();
	}
}

pub fn LogLevelToHtraceMapper(level: &LogLevel) -> Level
{
	match level
	{
		LogLevel::Error => Level::ERROR,
		LogLevel::Warn => Level::WARNING,
		LogLevel::Info => Level::NOTICE,
		LogLevel::Debug => Level::NORMAL,
		LogLevel::Trace => Level::DEBUG,
	}
}
