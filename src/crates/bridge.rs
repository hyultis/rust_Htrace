use crate::components::level::Level;

#[derive(Clone)]
pub struct HtraceBridge {
	/// minimum level for backtrace (default warn)
	min_level_backtrace: Level,

	#[cfg(feature = "log_consumer")]
	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Log.level::Trace is the maximum level
	pub min_level_log: Level,
	#[cfg(feature = "tracing_consumer")]
	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Tracing.level::Trace is the maximum level
	pub min_level_tracing: Level,
}

impl HtraceBridge {

	/// return if a level is a backtrace level (equal or above the "min_level_backtrace")
	pub fn isBacktrace(&self, level: &Level) -> bool {
		return level.tou8() >= self.min_level_backtrace.tou8();
	}


	#[cfg(feature = "log_consumer")]
	/// return if a level is a log level (equal or above the "min_level_log")
	pub fn isLog(&self, level: &Level) -> bool {
		return level.tou8() >= self.min_level_log.tou8();
	}

	#[cfg(feature = "tracing_consumer")]
	/// return if a level is a tracing level (equal or above the "min_level_tracing")
	pub fn isTracing(&self, level: &Level) -> bool {
		return level.tou8() >= self.min_level_tracing.tou8();
	}
}

impl Default for HtraceBridge {
	fn default() -> Self {
		HtraceBridge {
			min_level_backtrace: Level::WARNING,
			#[cfg(feature = "log_consumer")]
			min_level_log: Level::DEBUG,
			#[cfg(feature = "tracing_consumer")]
			min_level_tracing: Level::DEBUG,
		}
	}
}