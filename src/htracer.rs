use crate::components::context::Context;
use crate::components::hbacktrace::Hbacktrace;
use crate::components::level::Level;
use crate::components::trace::OneTrace;
use crate::context_manager::ContextManager;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use crate::crates::bridge::HtraceBridge;
use crate::thread_manager::{MAIN_THREAD, ThreadManager};
use std::any::Any;
use std::fmt::{Debug, Display};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use time::OffsetDateTime;

mod worker;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

/// A control operation never waits past its timeout to join a blocked worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ControlError
{
	#[error("Htrace has not been initialized")]
	NotInitialized,
	#[error("Htrace admission is closed")]
	Closed,
	#[error("Htrace worker is unavailable")]
	Unavailable,
	#[error("Htrace control operation timed out; the worker may still be running")]
	Timeout,
	#[error("Htrace control cannot wait from a worker callback")]
	Reentrant,
	#[error("an Htrace module callback or flush failed; inspect HTracer::stats()")]
	ModuleFailed,
}

/// Cumulative diagnostics. Fields are sampled independently while producers run.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats
{
	pub backtraces: u64,
	pub backtraceErrors: u64,
	pub accepted: u64,
	pub processed: u64,
	pub rejected: u64,
	pub moduleErrors: u64,
	pub workerErrors: u64,
	/// Traces awaiting consumption, excluding the currently executing callback.
	pub queued: u64,
}

pub struct HTracer
{
	runtime: worker::Runtime,
}

static BACKTRACES: AtomicU64 = AtomicU64::new(0);
static BACKTRACE_ERRORS: AtomicU64 = AtomicU64::new(0);

static BRIDGESET: OnceLock<()> = OnceLock::new();
static SINGLETON: OnceLock<HTracer> = OnceLock::new();

impl HTracer
{
	/// Accessing the singleton does not create a worker before configuration.
	pub fn singleton() -> &'static HTracer
	{
		SINGLETON.get_or_init(|| HTracer {
			runtime: worker::Runtime::default(),
		})
	}

	/// Set or override the global context. A completed shutdown is terminal;
	/// later configuration does not restart the worker. Bridges install once.
	pub fn globalContext_set(
		mut context: Context,
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))] bridge: HtraceBridge,
	)
	{
		if context.threadName_get().is_none()
		{
			context.threadName_set(MAIN_THREAD);
		}
		let modules = context
			.modules_get()
			.values()
			.flatten()
			.map(Arc::downgrade)
			.collect();
		ContextManager::singleton().global_set(context);
		ThreadManager::local_setName(MAIN_THREAD);
		// A failed start is observable through workerErrors and control results.
		let _ = Self::singleton().runtime.start(modules);

		BRIDGESET.get_or_init(|| {
			#[cfg(feature = "log_consumer")]
			{
				// Contexts may lower their threshold after installation.
				log::set_max_level(log::LevelFilter::Trace);
				if let Err(err) = log::set_boxed_logger(Box::new(bridge.clone()))
				{
					log::warn!("[Htrace] global default logger is already set : {}", err);
				}
			}
			#[cfg(feature = "tracing_consumer")]
			{
				use tracing_subscriber::layer::SubscriberExt;
				let subscriber = tracing_subscriber::Registry::default().with(bridge.filtered());
				if let Err(err) = tracing::subscriber::set_global_default(subscriber)
				{
					tracing::warn!(
						"[Htrace] global default tracing-subscriber is already set : {}",
						err
					);
				}
			}
		});
	}

	/// Fast eligibility check: no message, snapshot or backtrace is constructed.
	/// Concurrent reconfiguration/closure can still reject a subsequent trace.
	pub fn enabled(level: Level) -> bool
	{
		Self::singleton().runtime.is_accepting()
			&& level.tou8() >= ContextManager::singleton().minimum_level().tou8()
	}

	/// Explicit Display path, including format_args! and borrowed values.
	pub fn trace_display<T: Display + ?Sized>(
		entry: &T,
		level: Level,
		file: &str,
		line: u32,
		backtraces: Vec<Hbacktrace>,
	)
	{
		Self::trace_message(|| format!("{entry}"), level, file, line, backtraces);
	}

	/// Explicit Debug path for values containing non-static references.
	pub fn trace_debug<T: Debug + ?Sized>(
		entry: &T,
		level: Level,
		file: &str,
		line: u32,
		backtraces: Vec<Hbacktrace>,
	)
	{
		Self::trace_message(|| format!("{entry:?}"), level, file, line, backtraces);
	}

	fn trace_message(
		message: impl FnOnce() -> String,
		level: Level,
		file: &str,
		line: u32,
		backtraces: Vec<Hbacktrace>,
	)
	{
		let tracer = Self::singleton();
		if !tracer.runtime.is_accepting()
		{
			tracer.runtime.reject();
			return;
		}
		if level.tou8() < ContextManager::singleton().minimum_level().tou8()
		{
			return;
		}
		let date = OffsetDateTime::now_utc();
		let context = ContextManager::singleton().resolve();
		tracer.runtime.send_trace(OneTrace {
			message: message(),
			date,
			level,
			context,
			filename: file.to_owned(),
			fileline: line,
			backtraces,
		});
	}

	/// Compatibility path for owned/static Debug values. Use trace_display or
	/// trace_debug for borrowed values, or a formatted HTrace! invocation.
	pub fn trace<T: Any + Debug>(
		rawEntry: &T,
		level: Level,
		file: &str,
		line: u32,
		backtraces: Vec<Hbacktrace>,
	)
	{
		Self::trace_message(
			|| {
				let entry = rawEntry as &dyn Any;
				if let Some(content) = entry.downcast_ref::<String>()
				{
					content.clone()
				}
				else if let Some(content) = entry.downcast_ref::<&str>()
				{
					(*content).to_owned()
				}
				else
				{
					format!("{rawEntry:?}")
				}
			},
			level,
			file,
			line,
			backtraces,
		);
	}

	/// Flush traces inserted before this barrier and their modules. Producers
	/// may continue. No fsync is promised. Any prior module failure is reported.
	pub fn flush() -> Result<(), ControlError>
	{
		Self::flush_timeout(CONTROL_TIMEOUT)
	}

	pub fn flush_timeout(timeout: Duration) -> Result<(), ControlError>
	{
		Self::singleton().runtime.flush(timeout)
	}

	/// Close admission, drain accepted traces, flush and notify living modules.
	/// Repeated calls observe the same completion. A timeout does not kill I/O.
	/// Shutting down before initialization also closes admission permanently.
	pub fn shutdown() -> Result<(), ControlError>
	{
		Self::shutdown_timeout(CONTROL_TIMEOUT)
	}

	pub fn shutdown_timeout(timeout: Duration) -> Result<(), ControlError>
	{
		Self::singleton().runtime.shutdown(timeout)
	}

	pub fn stats() -> Stats
	{
		{
			let mut stats = Self::singleton().runtime.stats();
			stats.backtraces = BACKTRACES.load(Ordering::Relaxed);
			stats.backtraceErrors = BACKTRACE_ERRORS.load(Ordering::Relaxed);
			stats
		}
	}

	pub(crate) fn context_exit(context: Context)
	{
		Self::singleton().runtime.context_exit(context);
	}
	/// Capture the calling thread once and resolve each frame once. Missing
	/// symbols are normal; a recoverable resolver panic produces an empty trace.
	#[inline(never)]
	pub fn backtrace(base: &str) -> Vec<Hbacktrace>
	{
		BACKTRACES.fetch_add(1, Ordering::Relaxed);
		match catch_unwind(AssertUnwindSafe(|| {
			let mut frames = Vec::new();
			backtrace::trace(|frame| {
				backtrace::resolve_frame(frame, |symbol| {
					let name = symbol.name().map(|name| name.to_string());
					let filename = symbol
						.filename()
						.map(|path| path.to_string_lossy().into_owned());
					if name.is_some() || filename.is_some()
					{
						frames.push(Hbacktrace {
							funcName: name.unwrap_or_else(|| "<unknown>".to_owned()),
							fileName: filename,
							line: symbol.lineno(),
						});
					}
				});
				true
			});
			Self::backtrace_filter(frames, base)
		}))
		{
			Ok(frames) => frames,
			Err(_) =>
			{
				BACKTRACE_ERRORS.fetch_add(1, Ordering::Relaxed);
				Vec::new()
			}
		}
	}

	fn backtrace_filter(frames: Vec<Hbacktrace>, base: &str) -> Vec<Hbacktrace>
	{
		let start = frames
			.iter()
			.position(|frame| {
				!base.is_empty()
					&& frame
						.fileName
						.as_ref()
						.is_some_and(|file| file.ends_with(base))
			})
			.or_else(|| {
				frames
					.iter()
					.rposition(|frame| frame.funcName.contains("::HTracer::backtrace"))
					.map(|index| index + 1)
			})
			.unwrap_or(0);
		frames
			.into_iter()
			.skip(start)
			.filter(|frame| {
				!frame
					.fileName
					.as_ref()
					.is_some_and(|file| file.starts_with("/rustc/"))
					&& !frame.funcName.contains("::HTracer::backtrace")
			})
			.collect()
	}
}

#[cfg(test)]
mod backtrace_tests
{
	use super::*;

	fn frame(name: &str, file: Option<&str>) -> Hbacktrace
	{
		Hbacktrace {
			funcName: name.into(),
			fileName: file.map(str::to_owned),
			line: None,
		}
	}

	#[test]
	fn absent_symbols_and_partial_frames_have_a_safe_fallback()
	{
		assert!(HTracer::backtrace_filter(Vec::new(), "missing.rs").is_empty());
		let frames = HTracer::backtrace_filter(vec![frame("<unknown>", None)], "missing.rs");
		assert_eq!(frames.len(), 1);
		assert_eq!(frames[0].funcName, "<unknown>");
		let frames = HTracer::backtrace_filter(
			vec![
				frame("htrace::htracer::HTracer::backtrace", None),
				frame("application::request", None),
			],
			"missing.rs",
		);
		assert_eq!(frames.len(), 1);
		assert_eq!(frames[0].funcName, "application::request");
	}

	#[test]
	fn source_origin_filters_internal_and_rustc_frames()
	{
		let frames = HTracer::backtrace_filter(
			vec![
				frame(
					"htrace::htracer::HTracer::backtrace",
					Some("src/htracer.rs"),
				),
				frame("application::request", Some("/remapped/src/app.rs")),
				frame("core::call", Some("/rustc/abc/core.rs")),
				frame("external", None),
			],
			"src/app.rs",
		);
		assert_eq!(frames.len(), 2);
		assert_eq!(frames[0].funcName, "application::request");
		assert_eq!(frames[1].funcName, "external");
	}
}
