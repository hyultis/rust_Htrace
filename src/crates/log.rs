use crate::components::level::Level;
use crate::crates::bridge::HtraceBridge;
use crate::htracer;
use log::{Level as LogLevel, LevelFilter, Log, Metadata, Record};
use std::borrow::Cow;

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
		// Static file names (the common case) are not copied.
		let file: Cow<'static, str> = match record.file_static()
		{
			Some(file) => Cow::Borrowed(file),
			None => record
				.file()
				.map_or(Cow::Borrowed("unknown"), |file| Cow::Owned(file.to_owned())),
		};
		let line = record.line().unwrap_or(0);

		let mut backtrace = None;
		if (self.isBacktrace(&convertedLevel))
		{
			backtrace = Some(htracer::HTracer::backtrace(file.clone()));
		}

		// Formatted once, only if the trace is admitted.
		htracer::HTracer::trace_display(record.args(), convertedLevel, file, line, backtrace);
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

/// Most verbose `log` filter whose records the bridge can accept at this
/// threshold (`Off` when none can, e.g. FATAL). Levels between two `log`
/// levels map to the next more severe `log` level that still reaches it.
pub(crate) fn HtraceLevelToLogFilter(minimum: &Level) -> LevelFilter
{
	for level in [
		LogLevel::Trace,
		LogLevel::Debug,
		LogLevel::Info,
		LogLevel::Warn,
		LogLevel::Error,
	]
	{
		if LogLevelToHtraceMapper(&level).tou8() >= minimum.tou8()
		{
			return level.to_level_filter();
		}
	}
	LevelFilter::Off
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn log_filter_lets_exactly_accepted_levels_pass()
	{
		for (minimum, expected) in [
			(Level::DEBUG, LevelFilter::Trace),
			(Level::DEBUGERR, LevelFilter::Debug),
			(Level::NORMAL, LevelFilter::Debug),
			(Level::NOTICE, LevelFilter::Info),
			(Level::NOTICEDERR, LevelFilter::Warn),
			(Level::WARNING, LevelFilter::Warn),
			(Level::ERROR, LevelFilter::Error),
			(Level::FATAL, LevelFilter::Off),
		]
		{
			assert_eq!(HtraceLevelToLogFilter(&minimum), expected, "{minimum:?}");
			// The filter admits a log level iff the bridge threshold accepts it.
			for level in [
				LogLevel::Error,
				LogLevel::Warn,
				LogLevel::Info,
				LogLevel::Debug,
				LogLevel::Trace,
			]
			{
				let accepted = LogLevelToHtraceMapper(&level).tou8() >= minimum.tou8();
				assert_eq!(level <= expected, accepted, "{minimum:?} {level:?}");
			}
		}
	}
}
