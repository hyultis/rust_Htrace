//! In-memory capture module for integration tests.
//!
//! Include it from a test target with:
//! `#[path = "support/capture.rs"] mod capture;`
//!
//! Only public getters of `OneTrace`/`Context` are used, so the snapshot stays
//! independent of how the trace fields are stored internally.
#![allow(dead_code)]
#![allow(non_snake_case)]

use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::components::trace::OneTrace;
use htrace::modules::module_abstract::ModuleAbstract;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Owned snapshot of one trace callback.
#[derive(Clone, Debug)]
pub struct Captured
{
	pub level: Level,
	pub message: String,
	pub filename: String,
	pub fileline: u32,
	pub contextName: Option<String>,
	pub threadName: Option<String>,
	pub extras: HashMap<String, String>,
	pub backtraceCount: usize,
	pub backtraces: Vec<String>,
}

impl Captured
{
	fn from_trace(trace: &OneTrace) -> Self
	{
		let context: &Context = &trace.context;
		Self {
			level: trace.level,
			message: trace.message.to_string(),
			filename: trace.filename.to_string(),
			fileline: trace.fileline,
			contextName: context.name_get().clone(),
			threadName: context.threadName_get().clone(),
			extras: context.extra_getAll().clone(),
			backtraceCount: trace.backtraces.len(),
			backtraces: trace
				.backtraces
				.iter()
				.map(|frame| frame.to_string())
				.collect(),
		}
	}
}

/// Recording module. Clones share the same storage: keep one clone as the
/// test handle and register another one in a context.
#[derive(Clone)]
pub struct Capture
{
	name: String,
	traces: Arc<Mutex<Vec<Captured>>>,
	contextExits: Arc<AtomicUsize>,
	globalExits: Arc<AtomicUsize>,
}

impl Default for Capture
{
	fn default() -> Self
	{
		Self::new()
	}
}

impl Capture
{
	pub fn new() -> Self
	{
		Self {
			name: "capture".to_string(),
			traces: Arc::new(Mutex::new(Vec::new())),
			contextExits: Arc::new(AtomicUsize::new(0)),
			globalExits: Arc::new(AtomicUsize::new(0)),
		}
	}

	/// Shared storage handle.
	pub fn handle(&self) -> Arc<Mutex<Vec<Captured>>>
	{
		self.traces.clone()
	}

	fn lock(&self) -> MutexGuard<'_, Vec<Captured>>
	{
		self.traces
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner())
	}

	fn record(&self, trace: &OneTrace)
	{
		let captured = Captured::from_trace(trace);
		self.lock().push(captured);
	}

	/// Copy of every captured trace, in callback order.
	pub fn snapshot(&self) -> Vec<Captured>
	{
		self.lock().clone()
	}

	/// Remove captured traces (exit counters are kept).
	pub fn clear(&self)
	{
		self.lock().clear();
	}

	/// Number of captured traces.
	pub fn count(&self) -> usize
	{
		self.lock().len()
	}

	/// First captured trace matching the predicate.
	pub fn find(&self, predicate: impl Fn(&Captured) -> bool) -> Option<Captured>
	{
		self.lock()
			.iter()
			.find(|captured| predicate(captured))
			.cloned()
	}

	/// Number of `event_onContextExit` callbacks.
	pub fn contextExits(&self) -> usize
	{
		self.contextExits.load(Ordering::Acquire)
	}

	/// Number of `event_onGlobalExit` callbacks.
	pub fn globalExits(&self) -> usize
	{
		self.globalExits.load(Ordering::Acquire)
	}
}

macro_rules! capture_levels
{
	($($method:ident),*) => {
		$(fn $method(&self, trace: &OneTrace) { self.record(trace); })*
	};
}

impl ModuleAbstract for Capture
{
	fn name_set(&mut self, name: String) -> anyhow::Result<()>
	{
		self.name = name;
		Ok(())
	}

	fn name_get(&self) -> anyhow::Result<String>
	{
		Ok(self.name.clone())
	}

	capture_levels!(
		event_onDebug,
		event_onDebugErr,
		event_onNormal,
		event_onNotice,
		event_onNoticeErr,
		event_onWarning,
		event_onError,
		event_onFatal
	);

	fn event_onContextExit(&self, _: &Context)
	{
		self.contextExits.fetch_add(1, Ordering::AcqRel);
	}

	fn event_onGlobalExit(&self)
	{
		self.globalExits.fetch_add(1, Ordering::AcqRel);
	}
}
