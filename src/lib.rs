#![allow(non_snake_case)]
#![allow(unused_parens)]

extern crate alloc;
extern crate core;

pub mod components;
mod context_manager;
pub mod htracer;
pub mod modules;
mod thread_manager;

pub mod crates;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors
{
	#[error("'module/{0}' not found in config")]
	CannotFoundConfigNode(String),
	#[error("module '{0}' configuration returned a error : {1}")]
	ModuleConfigError(String, #[source] anyhow::Error),
}

/// Trace a value (Debug + Any) or a formatted message, at NORMAL by default.
/// Formatting and backtrace capture are skipped when the level is disabled.
/// A single expression is raw: `HTrace!("val = {x}")` does not interpolate.
/// Use `HTrace!("val = {}", x)` or `HTrace!("val = {x}",)` for formatting.
/// Display-only and borrowed values work in the formatted form.
/// ```
/// use htrace::{HTrace, components::level::Level};
/// let value = 42;
/// HTrace!(value);
/// HTrace!("value = {}", value);
/// HTrace!((Level::DEBUG) "value = {}", value);
/// ```
#[macro_export]
macro_rules! HTrace
{
	(($level:expr) $fmt:expr, $($args:tt)*) => {{
		let __htrace_level = $level;
		if $crate::htracer::HTracer::enabled(__htrace_level)
		{
			let __htrace_backtrace = if __htrace_level.tou8() >= $crate::components::level::Level::ERROR.tou8()
			{
				$crate::htracer::HTracer::backtrace(file!())
			}
			else { ::std::vec::Vec::new() };
			$crate::htracer::HTracer::trace_display(&format_args!($fmt, $($args)*), __htrace_level, file!(), line!(), __htrace_backtrace);
		}
	}};
	(($level:expr) $value:expr) => {{
		let __htrace_level = $level;
		if $crate::htracer::HTracer::enabled(__htrace_level)
		{
			let __htrace_backtrace = if __htrace_level.tou8() >= $crate::components::level::Level::ERROR.tou8()
			{
				$crate::htracer::HTracer::backtrace(file!())
			}
			else { ::std::vec::Vec::new() };
			$crate::htracer::HTracer::trace(&$value, __htrace_level, file!(), line!(), __htrace_backtrace);
		}
	}};
	($fmt:expr, $($args:tt)*) => {
		$crate::HTrace!(($crate::components::level::Level::NORMAL) $fmt, $($args)*);
	};
	($value:expr) => {
		$crate::HTrace!(($crate::components::level::Level::NORMAL) $value);
	};
}

/// Log a Result's error by reference, at ERROR by default.
/// The Result remains usable after the call. Its expression is evaluated once
/// even when tracing is disabled; only formatting and backtrace are skipped.
/// Errors use Debug unless a format string is provided.
/// ```
/// use htrace::HTraceError;
/// let result: Result<(), String> = Err("example".into());
/// HTraceError!("error: {}", result);
/// assert!(result.is_err());
/// ```
#[macro_export]
macro_rules! HTraceError
{
	(($level:expr) $desc:expr, $result:expr) => {{
		if let Err(__htrace_error) = &$result
		{
			$crate::HTrace!(($level) $desc, __htrace_error);
		}
	}};
	(($level:expr) $result:expr) => {{
		if let Err(__htrace_error) = &$result
		{
			$crate::HTrace!(($level) "{:?}", __htrace_error);
		}
	}};
	($desc:expr, $result:expr) => {
		$crate::HTraceError!(($crate::components::level::Level::ERROR) $desc, $result);
	};
	($result:expr) => {
		$crate::HTraceError!(($crate::components::level::Level::ERROR) $result);
	};
}
