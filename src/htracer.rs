use crate::Errors;
use crate::components::context::Context;
use crate::components::hbacktrace::Hbacktrace;
use crate::components::level::Level;
use crate::components::trace::OneTrace;
use crate::context_manager::ContextManager;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use crate::crates::bridge::HtraceBridge;
use std::any::Any;
use std::borrow::Cow;
use std::fmt::{Debug, Display};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use time::OffsetDateTime;

mod worker;

const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of raw frames captured per backtrace (innermost first),
/// before resolution and filtering. Bounds the memory held by a queued trace;
/// a frame can still resolve to several (inlined) displayed entries.
pub(crate) const BACKTRACE_MAX_FRAMES: usize = 128;

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
	/// Approximate while producers run (never negative).
	pub queued: u64,
}

/// One unresolved frame: an instruction pointer, or the whole frame where
/// symbolication needs more than the address (inline context on Windows).
#[cfg(not(windows))]
type RawFrame = usize;
#[cfg(windows)]
type RawFrame = backtrace::Frame;

/// Producer stack walked by `HTracer::backtrace`, without symbols. The worker
/// resolves and filters it into `OneTrace::backtraces` before dispatch.
/// Internal to the macros and bridges.
#[doc(hidden)]
pub struct RawBacktrace
{
	frames: Vec<RawFrame>,
	base: Cow<'static, str>,
}

pub struct HTracer
{
	runtime: worker::Runtime,
}

static BACKTRACES: AtomicU64 = AtomicU64::new(0);
static BACKTRACE_ERRORS: AtomicU64 = AtomicU64::new(0);

static SINGLETON: OnceLock<HTracer> = OnceLock::new();

/// Process-wide bridge installation state, guarded for the whole configuration.
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
struct BridgeState
{
	/// Fixed by the first installation of at least one consumer.
	bridge: Option<HtraceBridge>,
	#[cfg(feature = "log_consumer")]
	log: bool,
	#[cfg(feature = "tracing_consumer")]
	tracing: bool,
}

#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
impl BridgeState
{
	fn installed(&self) -> bool
	{
		let mut installed = false;
		#[cfg(feature = "log_consumer")]
		{
			installed |= self.log;
		}
		#[cfg(feature = "tracing_consumer")]
		{
			installed |= self.tracing;
		}
		installed
	}

	/// Install each consumer not yet installed. The first consumer failure is
	/// reported; consumers that could be installed stay installed.
	fn install(&mut self, bridge: HtraceBridge) -> Result<(), Errors>
	{
		let mut result = Ok(());
		#[cfg(feature = "log_consumer")]
		if !self.log
		{
			let maxLevel = crate::crates::log::HtraceLevelToLogFilter(&bridge.log_min_level);
			match log::set_boxed_logger(Box::new(bridge.clone()))
			{
				Ok(()) =>
				{
					// Only once installed, and only up to what the bridge accepts.
					log::set_max_level(maxLevel);
					self.log = true;
				}
				Err(_) => result = Err(Errors::LoggerAlreadySet),
			}
		}
		#[cfg(feature = "tracing_consumer")]
		if !self.tracing
		{
			use tracing_subscriber::layer::SubscriberExt;
			let subscriber =
				tracing_subscriber::Registry::default().with(bridge.clone().filtered());
			match tracing::subscriber::set_global_default(subscriber)
			{
				Ok(()) => self.tracing = true,
				Err(_) =>
				{
					if result.is_ok()
					{
						result = Err(Errors::TracingSubscriberAlreadySet);
					}
				}
			}
		}
		if self.bridge.is_none() && self.installed()
		{
			self.bridge = Some(bridge);
		}
		result
	}
}

#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
static BRIDGE: parking_lot::Mutex<BridgeState> = parking_lot::Mutex::new(BridgeState {
	bridge: None,
	#[cfg(feature = "log_consumer")]
	log: false,
	#[cfg(feature = "tracing_consumer")]
	tracing: false,
});

impl HTracer
{
	/// Accessing the singleton does not create a worker before configuration.
	pub fn singleton() -> &'static HTracer
	{
		SINGLETON.get_or_init(|| HTracer {
			runtime: worker::Runtime::default(),
		})
	}

	/// Set or override the global context and start the worker if needed.
	///
	/// The global context's `threadName_set()` is ignored: producers are `MAIN`
	/// unless a span names them. A completed shutdown is terminal: later calls
	/// return `Errors::WorkerUnavailable` and apply nothing.
	///
	/// With a consumer feature, one bridge is installed per process. The first
	/// call installing it fixes the bridge; later calls must pass an equal bridge
	/// (only the context is replaced), otherwise `Errors::BridgeAlreadyInstalled`
	/// is returned and nothing is applied. `Errors::LoggerAlreadySet` or
	/// `Errors::TracingSubscriberAlreadySet` report that another component owns
	/// the global `log` logger or `tracing` subscriber; the context is applied
	/// and any other consumer is still installed.
	///
	/// If the worker cannot be started (thread spawn failure, or a shutdown
	/// racing with this call), `Errors::WorkerUnavailable` is returned but the
	/// context stays applied; the bridge is not installed.
	pub fn globalContext_set(
		context: Context,
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))] bridge: HtraceBridge,
	) -> Result<(), Errors>
	{
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
		let mut bridgeState = BRIDGE.lock();
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
		if bridgeState
			.bridge
			.as_ref()
			.is_some_and(|installed| *installed != bridge)
		{
			return Err(Errors::BridgeAlreadyInstalled);
		}

		let runtime = &Self::singleton().runtime;
		if runtime.is_terminated()
		{
			return Err(Errors::WorkerUnavailable);
		}

		let modules = context
			.modules_get()
			.values()
			.flatten()
			.map(Arc::downgrade)
			.collect();
		// The context is published before admission opens: a trace accepted once
		// the worker starts always resolves the configured modules, and a trace
		// sent earlier is rejected (and counted) rather than silently dropped.
		let previous = ContextManager::singleton().global_replace(context);
		let started = runtime.start(modules).is_ok();

		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
		let result = match started
		{
			true => bridgeState.install(bridge),
			false => Err(Errors::WorkerUnavailable),
		};
		#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
		let result = match started
		{
			true => Ok(()),
			false => Err(Errors::WorkerUnavailable),
		};
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
		drop(bridgeState);

		// Its destructors may reenter Htrace: notify outside the bridge lock.
		if let Some(previous) = previous
		{
			Self::context_exit(Arc::new(previous));
		}
		result
	}

	/// Fast eligibility check: no message, snapshot or backtrace is constructed.
	/// The level is tested first and no lock is taken. Concurrent
	/// reconfiguration/closure can still reject a subsequent trace.
	pub fn enabled(level: Level) -> bool
	{
		level.tou8() >= ContextManager::singleton().minimum_level().tou8()
			&& Self::singleton().runtime.is_accepting()
	}

	/// Explicit Display path, including format_args! and borrowed values.
	/// `backtrace` is internal to the macros and bridges: pass `None`.
	pub fn trace_display<T: Display + ?Sized>(
		entry: &T,
		level: Level,
		file: impl Into<Cow<'static, str>>,
		line: u32,
		backtrace: Option<RawBacktrace>,
	)
	{
		Self::trace_message(|| format!("{entry}"), level, file.into(), line, backtrace);
	}

	/// Explicit Debug path for values containing non-static references.
	/// `backtrace` is internal to the macros and bridges: pass `None`.
	pub fn trace_debug<T: Debug + ?Sized>(
		entry: &T,
		level: Level,
		file: impl Into<Cow<'static, str>>,
		line: u32,
		backtrace: Option<RawBacktrace>,
	)
	{
		Self::trace_message(|| format!("{entry:?}"), level, file.into(), line, backtrace);
	}

	/// A trace below the minimum level is ignored without any lock and is not
	/// counted; a trace refused by admission is counted in `Stats::rejected`.
	pub(crate) fn trace_message(
		message: impl FnOnce() -> String,
		level: Level,
		file: Cow<'static, str>,
		line: u32,
		backtrace: Option<RawBacktrace>,
	)
	{
		Self::trace_resolved(message, |context| context, level, file, line, backtrace);
	}

	/// Trace under the producer's resolved context with only its name replaced,
	/// without installing a span: no context-exit notification follows. Used by
	/// the `tracing` bridge to expose the tracing span path.
	#[cfg(feature = "tracing_consumer")]
	pub(crate) fn trace_named(
		message: impl FnOnce() -> String,
		contextName: impl FnOnce() -> String,
		level: Level,
		file: Cow<'static, str>,
		line: u32,
		backtrace: Option<RawBacktrace>,
	)
	{
		Self::trace_resolved(
			message,
			|context| Arc::new(context.renamed(contextName())),
			level,
			file,
			line,
			backtrace,
		);
	}

	fn trace_resolved(
		message: impl FnOnce() -> String,
		adapt: impl FnOnce(Arc<Context>) -> Arc<Context>,
		level: Level,
		file: Cow<'static, str>,
		line: u32,
		backtrace: Option<RawBacktrace>,
	)
	{
		if level.tou8() < ContextManager::singleton().minimum_level().tou8()
		{
			return;
		}
		let tracer = Self::singleton();
		if !tracer.runtime.is_accepting()
		{
			tracer.runtime.reject();
			return;
		}
		let date = OffsetDateTime::now_utc();
		let context = adapt(ContextManager::singleton().resolve());
		tracer.runtime.send_trace(
			OneTrace {
				message: message(),
				date,
				level,
				context,
				filename: file,
				fileline: line,
				backtraces: Vec::new(),
			},
			backtrace,
		);
	}

	/// Compatibility path for owned/static Debug values. Use trace_display or
	/// trace_debug for borrowed values, or a formatted HTrace! invocation.
	/// `backtrace` is internal to the macros and bridges: pass `None`.
	pub fn trace<T: Any + Debug>(
		rawEntry: &T,
		level: Level,
		file: impl Into<Cow<'static, str>>,
		line: u32,
		backtrace: Option<RawBacktrace>,
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
			file.into(),
			line,
			backtrace,
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

	pub(crate) fn context_exit(context: Arc<Context>)
	{
		Self::singleton().runtime.context_exit(context);
	}
	/// Walk the calling thread's stack once, without resolving symbols: the
	/// worker resolves the frames before dispatch. `base` is the caller's source
	/// file, where the displayed backtrace starts. At most
	/// `BACKTRACE_MAX_FRAMES` frames (innermost first) are kept, bounding the
	/// memory held by each queued trace; deeper frames are dropped. Internal to
	/// the macros and bridges.
	#[doc(hidden)]
	#[inline(never)]
	pub fn backtrace(base: impl Into<Cow<'static, str>>) -> RawBacktrace
	{
		BACKTRACES.fetch_add(1, Ordering::Relaxed);
		let mut frames = Vec::with_capacity(32);
		if catch_unwind(AssertUnwindSafe(|| {
			backtrace::trace(|frame| {
				frames.push(Self::backtrace_rawFrame(frame));
				frames.len() < BACKTRACE_MAX_FRAMES
			});
		}))
		.is_err()
		{
			BACKTRACE_ERRORS.fetch_add(1, Ordering::Relaxed);
			frames.clear();
		}
		RawBacktrace {
			frames,
			base: base.into(),
		}
	}

	#[cfg(not(windows))]
	fn backtrace_rawFrame(frame: &backtrace::Frame) -> RawFrame
	{
		frame.ip() as usize
	}

	#[cfg(windows)]
	fn backtrace_rawFrame(frame: &backtrace::Frame) -> RawFrame
	{
		frame.clone()
	}

	#[cfg(not(windows))]
	fn backtrace_resolveFrame(frame: &RawFrame, callback: impl FnMut(&backtrace::Symbol))
	{
		backtrace::resolve(*frame as *mut std::ffi::c_void, callback);
	}

	#[cfg(windows)]
	fn backtrace_resolveFrame(frame: &RawFrame, callback: impl FnMut(&backtrace::Symbol))
	{
		backtrace::resolve_frame(frame, callback);
	}

	/// Resolve each captured frame once, on the worker. Missing symbols are
	/// normal; a recoverable resolver panic produces an empty trace.
	fn backtrace_resolve(raw: RawBacktrace) -> Vec<Hbacktrace>
	{
		match catch_unwind(AssertUnwindSafe(|| {
			let mut frames = Vec::with_capacity(raw.frames.len());
			for frame in &raw.frames
			{
				Self::backtrace_resolveFrame(frame, |symbol| {
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
			}
			Self::backtrace_filter(frames, &raw.base)
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

	#[inline(never)]
	fn deep(depth: usize) -> RawBacktrace
	{
		if depth == 0
		{
			return HTracer::backtrace("");
		}
		let raw = deep(std::hint::black_box(depth - 1));
		std::hint::black_box(raw)
	}

	#[test]
	fn deep_stacks_are_capped()
	{
		let raw = deep(BACKTRACE_MAX_FRAMES * 2);
		// Raw frames are capped; a frame may still resolve to several inlined symbols.
		assert_eq!(raw.frames.len(), BACKTRACE_MAX_FRAMES);
		assert!(!HTracer::backtrace_resolve(raw).is_empty());
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
