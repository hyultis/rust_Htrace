use log::{Level as LogLevel, Log, Metadata, Record};
use crate::components::level::Level;
use crate::crates::bridge::HtraceBridge;
use crate::htracer;

impl Log for HtraceBridge {
	fn enabled(&self, _: &Metadata) -> bool {
		println!("enabled");
		true
	}

	fn log(&self, record: &Record) {
		let convertedLevel = LogLevelToHtraceMapper(&record.level());
		if(!self.isLog(&convertedLevel))
		{
			return;
		}
		let file = record.file().unwrap_or("unknown");
		let line = record.line().unwrap_or(0);
		let arg = format!("{}",record.args());

		let mut backtrace = vec![];
		if(self.isBacktrace(&convertedLevel))
		{
			backtrace = htracer::HTracer::backtrace();
		}

		htracer::HTracer::trace(&arg, convertedLevel, file, line,backtrace);
	}

	fn flush(&self) {}
}

pub fn LogLevelToHtraceMapper(level: &LogLevel) -> Level {
	match level {
		LogLevel::Error => Level::ERROR,
		LogLevel::Warn => Level::WARNING,
		LogLevel::Info => Level::NOTICE,
		LogLevel::Debug => Level::NORMAL,
		LogLevel::Trace => Level::DEBUG,
	}
}

pub fn LogHtraceToLogLevelMapper(level: &Level) -> LogLevel {
	match level {
		Level::DEBUG => LogLevel::Trace,
		Level::DEBUGERR => LogLevel::Trace,
		Level::NORMAL => LogLevel::Debug,
		Level::NOTICE => LogLevel::Info,
		Level::NOTICEDERR => LogLevel::Info,
		Level::WARNING => LogLevel::Warn,
		Level::ERROR => LogLevel::Error,
		Level::FATAL => LogLevel::Error,
	}
}