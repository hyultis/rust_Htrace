use super::{ControlError, Stats};
use crate::components::context::Context;
use crate::components::level::Level;
use crate::components::trace::OneTrace;
use crate::modules::module_abstract::ModuleAbstract;
use parking_lot::{Condvar, Mutex};
use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak, mpsc};
use std::thread;
use std::time::{Duration, Instant};

type Module = Arc<dyn ModuleAbstract>;
type WeakModule = Weak<dyn ModuleAbstract>;
type ControlResult = Result<(), ControlError>;

thread_local! {
	// All module callbacks and their destructors run within this boundary.
	static IN_WORKER: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn is_worker() -> bool
{
	IN_WORKER.with(Cell::get)
}

enum Command
{
	Trace(OneTrace),
	ContextExit(Context),
	Register(Vec<WeakModule>),
	Flush(mpsc::Sender<ControlResult>),
	Shutdown,
}

#[derive(Default)]
struct Admission
{
	sender: Option<mpsc::Sender<Command>>,
	started: bool,
	closed: bool,
}

#[derive(Default)]
struct Shared
{
	accepted: AtomicU64,
	processed: AtomicU64,
	rejected: AtomicU64,
	moduleErrors: AtomicU64,
	workerErrors: AtomicU64,
	queued: AtomicU64,
	finished: AtomicBool,
	completion: Mutex<Option<ControlResult>>,
	completed: Condvar,
}

/// Owns admission and a detached worker. The unbounded queue never blocks on I/O.
/// A successful send while holding admission is the acceptance point; closing
/// admission and sending Shutdown under that same lock excludes later traces.
#[derive(Default)]
pub(super) struct Runtime
{
	admission: Mutex<Admission>,
	shared: Arc<Shared>,
}

impl Runtime
{
	pub(super) fn start(&self, modules: Vec<WeakModule>) -> ControlResult
	{
		let mut admission = self.admission.lock();
		if admission.closed
		{
			return Err(ControlError::Closed);
		}
		if admission.started
		{
			drop(admission);
			self.register(modules);
			return Ok(());
		}

		let (sender, receiver) = mpsc::channel();
		let shared = self.shared.clone();
		let spawned = thread::Builder::new()
			.name("Htrace-writer".into())
			.spawn(move || {
				IN_WORKER.with(|active| active.set(true));
				let result = catch_unwind(AssertUnwindSafe(|| {
					Worker::new(shared.clone(), modules).run(receiver)
				}));
				let result = match result
				{
					Ok(result) => result,
					Err(_) =>
					{
						shared.workerErrors.fetch_add(1, Ordering::Relaxed);
						Err(ControlError::Unavailable)
					}
				};
				*shared.completion.lock() = Some(result);
				shared.finished.store(true, Ordering::Release);
				shared.completed.notify_all();
				IN_WORKER.with(|active| active.set(false));
			});
		admission.started = true;
		match spawned
		{
			Ok(handle) =>
			{
				// Completion is observed separately: no join can extend a timeout.
				drop(handle);
				admission.sender = Some(sender);
				Ok(())
			}
			Err(_) =>
			{
				admission.closed = true;
				self.shared.workerErrors.fetch_add(1, Ordering::Relaxed);
				*self.shared.completion.lock() = Some(Err(ControlError::Unavailable));
				self.shared.finished.store(true, Ordering::Release);
				Err(ControlError::Unavailable)
			}
		}
	}

	pub(super) fn is_accepting(&self) -> bool
	{
		!is_worker()
			&& !self.shared.finished.load(Ordering::Acquire)
			&& self.admission.lock().sender.is_some()
	}

	pub(super) fn reject(&self)
	{
		self.shared.rejected.fetch_add(1, Ordering::Relaxed);
	}

	pub(super) fn send_trace(&self, trace: OneTrace) -> bool
	{
		if is_worker()
		{
			self.reject();
			return false;
		}
		let admission = self.admission.lock();
		let Some(sender) = &admission.sender
		else
		{
			drop(admission);
			self.reject();
			return false;
		};
		self.shared.queued.fetch_add(1, Ordering::Relaxed);
		let result = sender.send(Command::Trace(trace));
		if result.is_ok()
		{
			self.shared.accepted.fetch_add(1, Ordering::Relaxed);
		}
		else
		{
			self.shared.queued.fetch_sub(1, Ordering::Relaxed);
			self.shared.workerErrors.fetch_add(1, Ordering::Relaxed);
			self.reject();
		}
		drop(admission);
		// SendError owns the rejected trace; its user-owned context is dropped
		// only after releasing admission.
		result.is_ok()
	}

	pub(super) fn context_exit(&self, context: Context)
	{
		if is_worker()
		{
			self.reject();
			return;
		}
		self.send_auxiliary(Command::ContextExit(context));
	}

	fn register(&self, modules: Vec<WeakModule>)
	{
		self.send_auxiliary(Command::Register(modules));
	}

	fn send_auxiliary(&self, command: Command)
	{
		let admission = self.admission.lock();
		let available = admission.sender.is_some();
		let result = match &admission.sender
		{
			Some(sender) => sender.send(command),
			None => Err(mpsc::SendError(command)),
		};
		drop(admission);
		if available && result.is_err()
		{
			self.shared.workerErrors.fetch_add(1, Ordering::Relaxed);
		}
	}

	pub(super) fn flush(&self, timeout: Duration) -> ControlResult
	{
		if is_worker()
		{
			return Err(ControlError::Reentrant);
		}
		let begin = Instant::now();
		let Some(admission) = self.admission.try_lock_for(timeout)
		else
		{
			return Err(ControlError::Timeout);
		};
		let Some(sender) = &admission.sender
		else
		{
			return Err(
				if admission.closed
				{
					ControlError::Closed
				}
				else
				{
					ControlError::NotInitialized
				},
			);
		};
		let (ack, receiver) = mpsc::channel();
		let sent = sender.send(Command::Flush(ack));
		drop(admission);
		if sent.is_err()
		{
			self.shared.workerErrors.fetch_add(1, Ordering::Relaxed);
			return Err(ControlError::Unavailable);
		}
		match receiver.recv_timeout(timeout.saturating_sub(begin.elapsed()))
		{
			Ok(result) => result,
			Err(mpsc::RecvTimeoutError::Timeout) => Err(ControlError::Timeout),
			Err(mpsc::RecvTimeoutError::Disconnected) => Err(ControlError::Unavailable),
		}
	}

	pub(super) fn shutdown(&self, timeout: Duration) -> ControlResult
	{
		if is_worker()
		{
			return Err(ControlError::Reentrant);
		}
		let begin = Instant::now();
		let Some(mut admission) = self.admission.try_lock_for(timeout)
		else
		{
			return Err(ControlError::Timeout);
		};
		admission.closed = true;
		let sender = admission.sender.take();
		let started = admission.started;
		let sendFailed = sender
			.as_ref()
			.is_some_and(|sender| sender.send(Command::Shutdown).is_err());
		drop(admission);
		drop(sender);
		if !started
		{
			return Ok(());
		}
		if sendFailed
		{
			self.shared.workerErrors.fetch_add(1, Ordering::Relaxed);
		}
		let Some(mut completion) = self
			.shared
			.completion
			.try_lock_for(timeout.saturating_sub(begin.elapsed()))
		else
		{
			return Err(ControlError::Timeout);
		};
		loop
		{
			if let Some(result) = *completion
			{
				return result;
			}
			let remaining = timeout.saturating_sub(begin.elapsed());
			if remaining.is_zero()
			{
				return Err(ControlError::Timeout);
			}
			self.shared.completed.wait_for(&mut completion, remaining);
		}
	}

	pub(super) fn stats(&self) -> Stats
	{
		Stats {
			accepted: self.shared.accepted.load(Ordering::Relaxed),
			processed: self.shared.processed.load(Ordering::Relaxed),
			rejected: self.shared.rejected.load(Ordering::Relaxed),
			moduleErrors: self.shared.moduleErrors.load(Ordering::Relaxed),
			workerErrors: self.shared.workerErrors.load(Ordering::Relaxed),
			queued: self.shared.queued.load(Ordering::Relaxed),
			..Stats::default()
		}
	}
}

struct Worker
{
	shared: Arc<Shared>,
	pending: HashMap<usize, Module>,
	registry: HashMap<usize, WeakModule>,
}

impl Worker
{
	fn new(shared: Arc<Shared>, modules: Vec<WeakModule>) -> Self
	{
		let mut worker = Self {
			shared,
			pending: HashMap::new(),
			registry: HashMap::new(),
		};
		worker.register(modules);
		for module in worker.registry.values().filter_map(Weak::upgrade)
		{
			worker.callback(|| module.event_onStartup());
			worker.callback(|| {
				drop(module);
				Ok(())
			});
		}
		worker
	}

	fn register(&mut self, modules: Vec<WeakModule>)
	{
		self.registry.retain(|_, module| module.strong_count() > 0);
		for module in modules
		{
			self.registry
				.insert(module.as_ptr() as *const () as usize, module);
		}
	}

	fn remember(&mut self, module: &Module)
	{
		let identity = Arc::as_ptr(module) as *const () as usize;
		self.pending
			.entry(identity)
			.or_insert_with(|| module.clone());
		self.registry.insert(identity, Arc::downgrade(module));
	}

	fn callback(&self, callback: impl FnOnce() -> anyhow::Result<()>)
	{
		if !matches!(catch_unwind(AssertUnwindSafe(callback)), Ok(Ok(())))
		{
			self.shared.moduleErrors.fetch_add(1, Ordering::Relaxed);
		}
	}

	fn result(&self) -> ControlResult
	{
		if self.shared.moduleErrors.load(Ordering::Relaxed) > 0
		{
			Err(ControlError::ModuleFailed)
		}
		else
		{
			Ok(())
		}
	}

	fn flush(&mut self)
	{
		for (_, module) in self.pending.drain().collect::<Vec<_>>()
		{
			self.callback(|| module.flush());
			self.callback(|| {
				drop(module);
				Ok(())
			});
		}
		self.registry.retain(|_, module| module.strong_count() > 0);
	}

	fn run(mut self, receiver: mpsc::Receiver<Command>) -> ControlResult
	{
		while let Ok(command) = receiver.recv()
		{
			match command
			{
				Command::Trace(trace) =>
				{
					self.shared.queued.fetch_sub(1, Ordering::Relaxed);
					for module in trace.context.modules_get().values().flatten()
					{
						self.remember(module);
						self.callback(|| {
							Level::launchModuleFunc(module, &trace);
							Ok(())
						});
					}
					self.shared.processed.fetch_add(1, Ordering::Relaxed);
					self.callback(|| {
						drop(trace);
						Ok(())
					});
				}
				Command::ContextExit(context) =>
				{
					for module in context.modules_get().values().flatten()
					{
						self.remember(module);
						self.callback(|| {
							module.event_onContextExit(&context);
							Ok(())
						});
					}
					self.callback(|| {
						drop(context);
						Ok(())
					});
				}
				Command::Register(modules) => self.register(modules),
				Command::Flush(ack) =>
				{
					self.flush();
					let _ = ack.send(self.result());
				}
				Command::Shutdown =>
				{
					self.flush();
					for module in self.registry.values().filter_map(Weak::upgrade)
					{
						self.callback(|| {
							module.event_onGlobalExit();
							Ok(())
						});
						self.callback(|| {
							drop(module);
							Ok(())
						});
					}
					return self.result();
				}
			}
		}
		Err(ControlError::Unavailable)
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use std::sync::Barrier;
	use std::sync::atomic::AtomicUsize;
	use time::OffsetDateTime;

	const WAIT: Duration = Duration::from_secs(3);
	type TraceCallback = Box<dyn Fn(&OneTrace) + Send + Sync>;
	type FlushCallback = Box<dyn Fn() -> anyhow::Result<()> + Send + Sync>;

	#[derive(Default)]
	struct TestModule
	{
		onStartup: Option<FlushCallback>,
		onTrace: Option<TraceCallback>,
		onFlush: Option<FlushCallback>,
		onExit: Option<Box<dyn Fn() + Send + Sync>>,
		onGlobalExit: Option<Box<dyn Fn() + Send + Sync>>,
		// The field is a lifetime sentinel: its Drop implementation is the assertion.
		#[allow(dead_code)]
		onDrop: Option<DropCallback>,
	}

	struct DropCallback(Box<dyn Fn() + Send + Sync>);

	impl Drop for DropCallback
	{
		fn drop(&mut self)
		{
			(self.0)();
		}
	}

	impl ModuleAbstract for TestModule
	{
		fn name_set(&mut self, _: String) -> anyhow::Result<()>
		{
			Ok(())
		}
		fn name_get(&self) -> anyhow::Result<String>
		{
			Ok("test".into())
		}
		fn event_onStartup(&self) -> anyhow::Result<()>
		{
			match &self.onStartup
			{
				Some(callback) => callback(),
				None => Ok(()),
			}
		}
		fn event_onDebug(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onDebugErr(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onNormal(&self, trace: &OneTrace)
		{
			if let Some(callback) = &self.onTrace
			{
				callback(trace);
			}
		}
		fn event_onNotice(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onNoticeErr(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onWarning(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onError(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onFatal(&self, trace: &OneTrace)
		{
			self.event_onNormal(trace);
		}
		fn event_onContextExit(&self, _: &Context)
		{
			if let Some(callback) = &self.onExit
			{
				callback();
			}
		}
		fn event_onGlobalExit(&self)
		{
			if let Some(callback) = &self.onGlobalExit
			{
				callback();
			}
		}
		fn flush(&self) -> anyhow::Result<()>
		{
			match &self.onFlush
			{
				Some(callback) => callback(),
				None => Ok(()),
			}
		}
	}

	fn trace(context: &Context, message: impl Into<String>) -> OneTrace
	{
		OneTrace {
			message: message.into(),
			date: OffsetDateTime::now_utc(),
			level: Level::NORMAL,
			context: context.clone(),
			filename: "producer.rs".into(),
			fileline: 42,
			backtraces: vec![],
		}
	}

	fn configured(module: TestModule) -> (Arc<Runtime>, Context)
	{
		let mut context = Context::default();
		context.module_add("test", module);
		let runtime = Arc::new(Runtime::default());
		runtime
			.start(
				context
					.modules_get()
					.values()
					.flatten()
					.map(Arc::downgrade)
					.collect(),
			)
			.unwrap();
		(runtime, context)
	}

	#[test]
	fn startup_runs_on_worker_before_first_trace()
	{
		let events = Arc::new(Mutex::new(Vec::new()));
		let startupEvents = events.clone();
		let traceEvents = events.clone();
		let (runtime, context) = configured(TestModule {
			onStartup: Some(Box::new(move || {
				assert!(is_worker());
				assert_eq!(thread::current().name(), Some("Htrace-writer"));
				startupEvents.lock().push("startup");
				Ok(())
			})),
			onTrace: Some(Box::new(move |_| traceEvents.lock().push("trace"))),
			..Default::default()
		});
		assert!(runtime.send_trace(trace(&context, "first")));
		runtime.shutdown(WAIT).unwrap();
		assert_eq!(*events.lock(), ["startup", "trace"]);
	}

	#[test]
	fn startup_runs_without_traces_before_flush_or_shutdown()
	{
		for flushFirst in [false, true]
		{
			let starts = Arc::new(AtomicUsize::new(0));
			let output = starts.clone();
			let (runtime, _context) = configured(TestModule {
				onStartup: Some(Box::new(move || {
					output.fetch_add(1, Ordering::Relaxed);
					Ok(())
				})),
				..Default::default()
			});
			if flushFirst
			{
				runtime.flush(WAIT).unwrap();
				assert_eq!(starts.load(Ordering::Relaxed), 1);
			}
			runtime.shutdown(WAIT).unwrap();
			assert_eq!(starts.load(Ordering::Relaxed), 1);
			assert_eq!(runtime.stats().processed, 0);
		}
	}

	#[test]
	fn startup_deduplicates_initial_arcs_and_ignores_later_modules()
	{
		let initialStarts = Arc::new(AtomicUsize::new(0));
		let output = initialStarts.clone();
		let initial: Module = Arc::new(TestModule {
			onStartup: Some(Box::new(move || {
				output.fetch_add(1, Ordering::Relaxed);
				Ok(())
			})),
			..Default::default()
		});
		let lateStarts = Arc::new(AtomicUsize::new(0));
		let output = lateStarts.clone();
		let late: Module = Arc::new(TestModule {
			onStartup: Some(Box::new(move || {
				output.fetch_add(1, Ordering::Relaxed);
				Ok(())
			})),
			..Default::default()
		});
		let runtime = Runtime::default();
		runtime
			.start(vec![Arc::downgrade(&initial), Arc::downgrade(&initial)])
			.unwrap();
		runtime.flush(WAIT).unwrap();
		assert_eq!(initialStarts.load(Ordering::Relaxed), 1);
		// Repeated global configuration and explicit Register commands share this path.
		runtime
			.start(vec![Arc::downgrade(&initial), Arc::downgrade(&late)])
			.unwrap();
		let mut context = Context::default();
		context.module_add_arc("initial", initial);
		context.module_add_arc("late", late);
		assert!(runtime.send_trace(trace(&context, "later")));
		runtime.context_exit(context);
		runtime.flush(WAIT).unwrap();
		runtime.shutdown(WAIT).unwrap();
		assert_eq!(initialStarts.load(Ordering::Relaxed), 1);
		assert_eq!(lateStarts.load(Ordering::Relaxed), 0);
	}

	#[test]
	fn startup_errors_and_panics_are_isolated_and_visible_to_finite_controls()
	{
		let processed = Arc::new(AtomicUsize::new(0));
		let output = processed.clone();
		let healthyStarts = Arc::new(AtomicUsize::new(0));
		let startupOutput = healthyStarts.clone();
		let mut context = Context::default();
		context.module_add(
			"error",
			TestModule {
				onStartup: Some(Box::new(|| anyhow::bail!("injected startup error"))),
				..Default::default()
			},
		);
		context.module_add(
			"panic",
			TestModule {
				onStartup: Some(Box::new(|| panic!("injected startup panic"))),
				..Default::default()
			},
		);
		context.module_add(
			"healthy",
			TestModule {
				onStartup: Some(Box::new(move || {
					startupOutput.fetch_add(1, Ordering::Relaxed);
					Ok(())
				})),
				onTrace: Some(Box::new(move |_| {
					output.fetch_add(1, Ordering::Relaxed);
				})),
				..Default::default()
			},
		);
		let runtime = Runtime::default();
		runtime
			.start(
				context
					.modules_get()
					.values()
					.flatten()
					.map(Arc::downgrade)
					.collect(),
			)
			.unwrap();
		assert_eq!(runtime.flush(WAIT), Err(ControlError::ModuleFailed));
		assert_eq!(runtime.stats().moduleErrors, 2);
		assert_eq!(healthyStarts.load(Ordering::Relaxed), 1);
		assert!(runtime.send_trace(trace(&context, "after startup failures")));
		assert_eq!(runtime.shutdown(WAIT), Err(ControlError::ModuleFailed));
		assert_eq!(processed.load(Ordering::Relaxed), 1);
		assert_eq!(runtime.stats().processed, 1);
		assert_eq!(runtime.stats().moduleErrors, 2);
		assert_eq!(runtime.stats().workerErrors, 0);
	}

	#[test]
	fn unavailable_before_configuration_and_terminal_without_start()
	{
		let runtime = Runtime::default();
		assert!(!runtime.is_accepting());
		assert!(!runtime.send_trace(trace(&Context::default(), "early")));
		assert!(!runtime.admission.lock().started);
		assert_eq!(runtime.flush(WAIT), Err(ControlError::NotInitialized));
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
		assert_eq!(runtime.start(vec![]), Err(ControlError::Closed));
		assert_eq!(runtime.stats().rejected, 1);
	}

	#[test]
	fn preserves_insertion_order_and_producer_snapshot()
	{
		let (sender, receiver) = mpsc::channel();
		let (runtime, mut context) = configured(TestModule {
			onTrace: Some(Box::new(move |trace| {
				sender
					.send((
						trace.message.clone(),
						trace.date,
						trace.context.extra_get("source").cloned(),
						thread::current().name().map(str::to_owned),
					))
					.unwrap();
			})),
			..Default::default()
		});
		context.extra_set("source", "original");
		let mut first = trace(&context, "first");
		first.date = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(2);
		let mut second = trace(&context, "second");
		second.date = OffsetDateTime::UNIX_EPOCH;
		assert!(runtime.send_trace(first));
		assert!(runtime.send_trace(second));
		context.extra_set("source", "changed");
		runtime.shutdown(WAIT).unwrap();
		let first = receiver.recv_timeout(WAIT).unwrap();
		let second = receiver.recv_timeout(WAIT).unwrap();
		assert_eq!((&*first.0, &*second.0), ("first", "second"));
		assert!(first.1 > second.1, "dates must not determine queue order");
		assert_eq!(first.2.as_deref(), Some("original"));
		assert_eq!(second.2.as_deref(), Some("original"));
		assert_eq!(first.3.as_deref(), Some("Htrace-writer"));
		assert_eq!(runtime.stats().processed, 2);
	}

	#[test]
	fn forced_interproducer_order()
	{
		let received = Arc::new(Mutex::new(Vec::new()));
		let output = received.clone();
		let (runtime, context) = configured(TestModule {
			onTrace: Some(Box::new(move |trace| {
				output.lock().push(trace.message.clone())
			})),
			..Default::default()
		});
		let (firstDone, firstReceived) = mpsc::channel();
		let (secondDone, secondReceived) = mpsc::channel();
		let producerRuntime = runtime.clone();
		let producerContext = context.clone();
		let first = thread::spawn(move || {
			assert!(producerRuntime.send_trace(trace(&producerContext, "a1")));
			firstDone.send(()).unwrap();
			secondReceived.recv_timeout(WAIT).unwrap();
			assert!(producerRuntime.send_trace(trace(&producerContext, "a2")));
		});
		let producerRuntime = runtime.clone();
		let producerContext = context.clone();
		let second = thread::spawn(move || {
			firstReceived.recv_timeout(WAIT).unwrap();
			assert!(producerRuntime.send_trace(trace(&producerContext, "b1")));
			secondDone.send(()).unwrap();
		});
		first.join().unwrap();
		second.join().unwrap();
		runtime.shutdown(WAIT).unwrap();
		assert_eq!(*received.lock(), ["a1", "b1", "a2"]);
	}

	#[test]
	fn concurrent_burst_drains_once_and_preserves_each_producer_order()
	{
		let received = Arc::new(Mutex::new(Vec::new()));
		let output = received.clone();
		let (runtime, context) = configured(TestModule {
			onTrace: Some(Box::new(move |trace| {
				output.lock().push(trace.message.clone())
			})),
			..Default::default()
		});
		let barrier = Arc::new(Barrier::new(5));
		let producers: Vec<_> = (0..4)
			.map(|producer| {
				let runtime = runtime.clone();
				let context = context.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					for index in 0..128
					{
						assert!(runtime.send_trace(trace(&context, format!("{producer}:{index}"))));
					}
				})
			})
			.collect();
		barrier.wait();
		for producer in producers
		{
			producer.join().unwrap();
		}
		runtime.shutdown(WAIT).unwrap();
		let received = received.lock();
		assert_eq!(received.len(), 512);
		for producer in 0..4
		{
			let prefix = format!("{producer}:");
			let sequence: Vec<_> = received
				.iter()
				.filter_map(|message| message.strip_prefix(&prefix))
				.map(|index| index.parse::<usize>().unwrap())
				.collect();
			assert_eq!(sequence, (0..128).collect::<Vec<_>>());
		}
		assert_eq!(runtime.stats().accepted, 512);
		assert_eq!(runtime.stats().processed, 512);
		assert_eq!(runtime.stats().queued, 0);
	}

	#[test]
	fn flush_barrier_does_not_wait_for_later_producers()
	{
		let (flushEntered, flushObserved) = mpsc::channel();
		let (flushRelease, flushGate) = mpsc::channel();
		let flushGate = Mutex::new(flushGate);
		let firstFlush = AtomicBool::new(true);
		let (laterEntered, laterObserved) = mpsc::channel();
		let (laterRelease, laterGate) = mpsc::channel();
		let laterGate = Mutex::new(laterGate);
		let (runtime, context) = configured(TestModule {
			onTrace: Some(Box::new(move |trace| {
				if trace.message == "after"
				{
					laterEntered.send(()).unwrap();
					laterGate.lock().recv_timeout(WAIT).unwrap();
				}
			})),
			onFlush: Some(Box::new(move || {
				if firstFlush.swap(false, Ordering::Relaxed)
				{
					flushEntered.send(()).unwrap();
					flushGate.lock().recv_timeout(WAIT).unwrap();
				}
				Ok(())
			})),
			..Default::default()
		});
		assert!(runtime.send_trace(trace(&context, "before")));
		let flushing = runtime.clone();
		let (done, completed) = mpsc::channel();
		let control = thread::spawn(move || done.send(flushing.flush(WAIT)).unwrap());
		flushObserved.recv_timeout(WAIT).unwrap();
		assert_eq!(runtime.stats().processed, 1);
		assert!(runtime.send_trace(trace(&context, "after")));
		flushRelease.send(()).unwrap();
		laterObserved.recv_timeout(WAIT).unwrap();
		assert_eq!(completed.recv_timeout(WAIT).unwrap(), Ok(()));
		laterRelease.send(()).unwrap();
		control.join().unwrap();
		runtime.shutdown(WAIT).unwrap();
		assert_eq!(runtime.stats().processed, 2);
	}

	#[test]
	fn shutdown_race_rejects_late_traces_and_repeated_calls_share_completion()
	{
		let received = Arc::new(AtomicUsize::new(0));
		let output = received.clone();
		let exited = Arc::new(AtomicUsize::new(0));
		let globalExited = exited.clone();
		let (runtime, context) = configured(TestModule {
			onTrace: Some(Box::new(move |_| {
				output.fetch_add(1, Ordering::Relaxed);
			})),
			onGlobalExit: Some(Box::new(move || {
				globalExited.fetch_add(1, Ordering::Relaxed);
			})),
			..Default::default()
		});
		let barrier = Arc::new(Barrier::new(5));
		let (ready, started) = mpsc::channel();
		let producers: Vec<_> = (0..4)
			.map(|producer| {
				let runtime = runtime.clone();
				let context = context.clone();
				let barrier = barrier.clone();
				let ready = ready.clone();
				thread::spawn(move || {
					assert!(runtime.send_trace(trace(&context, format!("{producer}:first"))));
					ready.send(()).unwrap();
					barrier.wait();
					(0..200)
						.filter(|index| {
							runtime.send_trace(trace(&context, format!("{producer}:{index}")))
						})
						.count()
						+ 1
				})
			})
			.collect();
		for _ in 0..4
		{
			started.recv_timeout(WAIT).unwrap();
		}
		barrier.wait();
		let stopping = runtime.clone();
		let repeated = thread::spawn(move || stopping.shutdown(WAIT));
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
		assert_eq!(repeated.join().unwrap(), Ok(()));
		let successes: usize = producers
			.into_iter()
			.map(|producer| producer.join().unwrap())
			.sum();
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
		assert!(!runtime.send_trace(trace(&context, "late")));
		assert_eq!(successes as u64, runtime.stats().accepted);
		assert_eq!(runtime.stats().accepted, runtime.stats().processed);
		assert_eq!(received.load(Ordering::Relaxed), successes);
		assert_eq!(exited.load(Ordering::Relaxed), 1);
		assert!(runtime.stats().rejected > 0);
	}

	#[test]
	fn callback_panics_and_flush_errors_are_isolated_and_visible()
	{
		let received = Arc::new(AtomicUsize::new(0));
		let output = received.clone();
		let (runtime, mut context) = configured(TestModule {
			onTrace: Some(Box::new(|_| panic!("injected callback failure"))),
			onFlush: Some(Box::new(|| anyhow::bail!("injected flush failure"))),
			..Default::default()
		});
		context.module_add(
			"healthy",
			TestModule {
				onTrace: Some(Box::new(move |_| {
					output.fetch_add(1, Ordering::Relaxed);
				})),
				..Default::default()
			},
		);
		assert!(runtime.send_trace(trace(&context, "first")));
		assert!(runtime.send_trace(trace(&context, "second")));
		assert_eq!(runtime.flush(WAIT), Err(ControlError::ModuleFailed));
		assert_eq!(received.load(Ordering::Relaxed), 2);
		assert_eq!(runtime.stats().processed, 2);
		assert_eq!(runtime.stats().moduleErrors, 3);
		assert_eq!(runtime.shutdown(WAIT), Err(ControlError::ModuleFailed));
		assert_eq!(runtime.shutdown(WAIT), Err(ControlError::ModuleFailed));
	}

	#[test]
	fn blocked_worker_times_out_then_finishes_after_release()
	{
		let (entered, observed) = mpsc::channel();
		let (release, gate) = mpsc::channel();
		let gate = Mutex::new(gate);
		let (runtime, context) = configured(TestModule {
			onTrace: Some(Box::new(move |_| {
				entered.send(()).unwrap();
				gate.lock().recv_timeout(WAIT).unwrap();
			})),
			..Default::default()
		});
		assert!(runtime.send_trace(trace(&context, "blocked")));
		observed.recv_timeout(WAIT).unwrap();
		let begin = Instant::now();
		assert_eq!(
			runtime.flush(Duration::from_millis(15)),
			Err(ControlError::Timeout)
		);
		assert_eq!(
			runtime.shutdown(Duration::from_millis(15)),
			Err(ControlError::Timeout)
		);
		assert!(begin.elapsed() < Duration::from_secs(1));
		assert!(runtime.shared.completion.lock().is_none());
		assert!(!runtime.send_trace(trace(&context, "late")));
		release.send(()).unwrap();
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
		assert_eq!(runtime.stats().processed, 1);
		assert_eq!(runtime.shutdown(WAIT), Ok(()));
	}

	#[test]
	fn control_deadline_includes_admission_lock()
	{
		let (runtime, _) = configured(TestModule::default());
		let admission = runtime.admission.lock();
		let controlRuntime = runtime.clone();
		let (sent, observed) = mpsc::channel();
		let control = thread::spawn(move || {
			let begin = Instant::now();
			let flush = controlRuntime.flush(Duration::from_millis(15));
			let shutdown = controlRuntime.shutdown(Duration::from_millis(15));
			sent.send((flush, shutdown, begin.elapsed())).unwrap();
		});
		let (flush, shutdown, elapsed) = observed.recv_timeout(WAIT).unwrap();
		assert_eq!(flush, Err(ControlError::Timeout));
		assert_eq!(shutdown, Err(ControlError::Timeout));
		assert!(elapsed < Duration::from_secs(1));
		drop(admission);
		control.join().unwrap();
		runtime.shutdown(WAIT).unwrap();
	}

	#[test]
	fn reentrant_controls_and_traces_do_not_wait_or_recurse()
	{
		let runtime = Arc::new(Runtime::default());
		let weak = Arc::downgrade(&runtime);
		let (sent, observed) = mpsc::channel();
		let mut context = Context::default();
		context.module_add(
			"reentrant",
			TestModule {
				onTrace: Some(Box::new(move |_| {
					let runtime = weak.upgrade().unwrap();
					sent.send((
						runtime.flush(WAIT),
						runtime.shutdown(WAIT),
						runtime.send_trace(trace(&Context::default(), "recursive")),
					))
					.unwrap();
				})),
				..Default::default()
			},
		);
		runtime.start(vec![]).unwrap();
		assert!(runtime.send_trace(trace(&context, "outer")));
		assert_eq!(
			observed.recv_timeout(WAIT).unwrap(),
			(
				Err(ControlError::Reentrant),
				Err(ControlError::Reentrant),
				false
			)
		);
		runtime.shutdown(WAIT).unwrap();
		assert_eq!(runtime.stats().accepted, 1);
		assert_eq!(runtime.stats().rejected, 1);
	}

	#[test]
	fn barrier_retains_modules_until_flush_then_releases_them()
	{
		let (events, observed) = mpsc::channel();
		let flushed = events.clone();
		let globalExited = events.clone();
		let (runtime, context) = configured(TestModule {
			onFlush: Some(Box::new(move || {
				flushed.send("flush").unwrap();
				Ok(())
			})),
			onGlobalExit: Some(Box::new(move || {
				globalExited.send("global").unwrap();
			})),
			onDrop: Some(DropCallback(Box::new(move || {
				events.send("drop").unwrap();
			}))),
			..Default::default()
		});
		assert!(runtime.send_trace(trace(&context, "temporary")));
		drop(context);
		runtime.flush(WAIT).unwrap();
		assert_eq!(observed.recv_timeout(WAIT).unwrap(), "flush");
		assert_eq!(observed.recv_timeout(WAIT).unwrap(), "drop");
		runtime.shutdown(WAIT).unwrap();
		assert!(
			observed.try_recv().is_err(),
			"weak registry must not retain dead modules"
		);
	}

	#[test]
	fn context_exit_is_delivered_once_and_flushed()
	{
		let exits = Arc::new(AtomicUsize::new(0));
		let output = exits.clone();
		let flushes = Arc::new(AtomicUsize::new(0));
		let flushOutput = flushes.clone();
		let (runtime, context) = configured(TestModule {
			onExit: Some(Box::new(move || {
				output.fetch_add(1, Ordering::Relaxed);
			})),
			onFlush: Some(Box::new(move || {
				flushOutput.fetch_add(1, Ordering::Relaxed);
				Ok(())
			})),
			..Default::default()
		});
		runtime.context_exit(context);
		runtime.flush(WAIT).unwrap();
		assert_eq!(exits.load(Ordering::Relaxed), 1);
		assert_eq!(flushes.load(Ordering::Relaxed), 1);
		runtime.shutdown(WAIT).unwrap();
	}

	#[test]
	fn rejected_payload_destruction_runs_after_unlocking_admission()
	{
		let runtime = Arc::new(Runtime::default());
		let (sender, receiver) = mpsc::channel();
		runtime.admission.lock().sender = Some(sender);
		drop(receiver);
		let weak = Arc::downgrade(&runtime);
		let (sent, observed) = mpsc::channel();
		let mut context = Context::default();
		context.module_add(
			"drop",
			TestModule {
				onDrop: Some(DropCallback(Box::new(move || {
					let runtime = weak.upgrade().unwrap();
					sent.send(runtime.admission.try_lock().is_some()).unwrap();
				}))),
				..Default::default()
			},
		);
		let payload = trace(&context, "lost");
		drop(context);
		assert!(!runtime.send_trace(payload));
		assert!(observed.recv_timeout(WAIT).unwrap());
		assert_eq!(runtime.stats().rejected, 1);
		assert_eq!(runtime.stats().queued, 0);
	}
}
