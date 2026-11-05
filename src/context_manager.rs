use crate::components::context::Context;
use crate::components::level::Level;
use crate::htracer::HTracer;
use crate::thread_manager::ThreadManager;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use parking_lot::RwLock;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, ThreadId};

#[derive(Clone)]
struct ContextEntry
{
	id: u64,
	context: Context,
	threadName: String,
}

pub(crate) struct ContextManager
{
	data: DashMap<ThreadId, Vec<ContextEntry>>,
	global: RwLock<Option<Context>>,
	nextId: AtomicU64,
}

static SINGLETON: OnceLock<ContextManager> = OnceLock::new();

impl ContextManager
{
	/// get singleton
	pub fn singleton() -> &'static Self
	{
		return SINGLETON.get_or_init(Self::new);
	}

	pub fn add(&self, context: Context) -> Option<u64>
	{
		// Keep compatibility with stable compilers predating the try_update rename.
		#[allow(deprecated)]
		let id = self
			.nextId
			.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
			.ok()?;
		self.data
			.entry(thread::current().id())
			.or_default()
			.push(ContextEntry {
				id,
				context,
				threadName: ThreadManager::local_producerName(),
			});
		return Some(id);
	}

	/// Remove only this span, even when another span was entered after it.
	/// Its exit snapshot includes the surviving ancestors and the span itself,
	/// but excludes children that remain active. Callbacks run on the trace worker.
	pub fn remove(&self, threadId: ThreadId, contextId: u64)
	{
		let global = self.global_get();
		let exiting = match self.data.entry(threadId)
		{
			Entry::Vacant(_) => None,
			Entry::Occupied(mut entry) =>
			{
				let Some(position) = entry.get().iter().position(|entry| entry.id == contextId)
				else
				{
					return;
				};
				let snapshot = entry.get()[..=position].to_vec();
				let removed = entry.get_mut().remove(position);
				if entry.get().is_empty()
				{
					entry.remove();
				}
				Some((snapshot, removed))
			}
		};

		if let Some((snapshot, removed)) = exiting
		{
			// Merging can release the last module Arc, whose user-defined Drop may
			// reenter Htrace. Keep both merging and removed entries outside the map lock.
			let context = Self::resolve_internal(global, &snapshot, removed.threadName.clone());
			HTracer::context_exit(context);
		}
	}

	/// Replace the global configuration and enqueue exactly one logical exit
	/// for the previous configuration, after previously accepted traces.
	pub(crate) fn global_set(&self, context: Context)
	{
		let previous = self.global.write().replace(context);
		if let Some(context) = previous
		{
			HTracer::context_exit(context);
		}
	}

	/// get the global context
	pub(crate) fn global_get(&self) -> Option<Context>
	{
		return self.global.read().clone();
	}

	/// Resolve a snapshot for the current producer, from global to innermost span.
	pub fn resolve(&self) -> Context
	{
		let global = self.global_get();
		let threadName = ThreadManager::local_producerName();
		let snapshot = self
			.data
			.get(&thread::current().id())
			.map(|stack| stack.value().clone())
			.unwrap_or_default();
		return Self::resolve_internal(global, &snapshot, threadName);
	}

	/// Read the effective threshold without cloning a context or its modules.
	pub(crate) fn minimum_level(&self) -> Level
	{
		let mut level = self
			.global
			.read()
			.as_ref()
			.and_then(|context| context.level_getMin().copied())
			.unwrap_or(Level::DEBUG);
		if let Some(stack) = self.data.get(&thread::current().id())
		{
			for entry in stack.iter()
			{
				if let Some(minimum) = entry.context.level_getMin()
				{
					level = *minimum;
				}
			}
		}
		return level;
	}

	////////////// PRIVATE ///////////

	fn new() -> Self
	{
		return Self {
			data: DashMap::new(),
			global: RwLock::new(None),
			nextId: AtomicU64::new(0),
		};
	}

	fn resolve_internal(
		global: Option<Context>,
		entries: &[ContextEntry],
		threadName: String,
	) -> Context
	{
		let mut resolvedContext = global.unwrap_or_default();
		resolvedContext.threadName_set(threadName);

		for entry in entries
		{
			let oneContext = &entry.context;
			if let Some(threadName) = oneContext.threadName_get()
			{
				resolvedContext.threadName_set(threadName);
			}
			for (key, module) in oneContext.modules_get()
			{
				match module
				{
					None => resolvedContext.module_discard(key),
					Some(inner) => resolvedContext.module_add_arc(key, inner.clone()),
				}
			}
			if let Some(level) = oneContext.level_getMin()
			{
				resolvedContext.level_setMin(Some(*level));
			}
			if let Some(name) = oneContext.name_get()
			{
				resolvedContext.name_set(name);
			}
			resolvedContext.extra_merge(oneContext.extra_getAll());
		}

		return resolvedContext;
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use std::sync::{Arc, Barrier};

	use crate::components::trace::OneTrace;
	use crate::modules::module_abstract::ModuleAbstract;
	use std::sync::Weak;
	use std::sync::atomic::AtomicUsize;
	use std::time::{Duration, Instant};

	struct ReentrantDrop
	{
		manager: Weak<ContextManager>,
		dropped: Arc<AtomicUsize>,
	}

	impl Drop for ReentrantDrop
	{
		fn drop(&mut self)
		{
			let manager = self.manager.upgrade().unwrap();
			let id = manager.add(Context::default()).unwrap();
			manager.minimum_level();
			manager.remove(thread::current().id(), id);
			self.dropped.fetch_add(1, Ordering::Relaxed);
		}
	}

	macro_rules! empty_levels
	{
		($($method:ident),*) => {
			$(fn $method(&self, _: &OneTrace) {})*
		};
	}

	impl ModuleAbstract for ReentrantDrop
	{
		fn name_set(&mut self, _: String) -> anyhow::Result<()>
		{
			Ok(())
		}
		fn name_get(&self) -> anyhow::Result<String>
		{
			Ok("reentrant".into())
		}
		empty_levels!(
			event_onDebug,
			event_onDebugErr,
			event_onNormal,
			event_onNotice,
			event_onNoticeErr,
			event_onWarning,
			event_onError,
			event_onFatal
		);
		fn event_onContextExit(&self, _: &Context) {}
		fn event_onGlobalExit(&self) {}
	}

	#[test]
	fn exhausted_identifiers_leave_new_contexts_uninstalled()
	{
		let manager = ContextManager::new();
		manager.nextId.store(u64::MAX - 1, Ordering::Relaxed);
		let last = manager.add(Context::default()).unwrap();
		assert_eq!(last, u64::MAX - 1);
		assert_eq!(manager.add(Context::default()), None);
		assert_eq!(manager.add(Context::default()), None);
		assert_eq!(manager.nextId.load(Ordering::Relaxed), u64::MAX);
		manager.remove(thread::current().id(), last);
		assert!(manager.data.is_empty());
	}

	#[test]
	fn removed_module_destructor_can_reenter_context_manager()
	{
		let dropped = Arc::new(AtomicUsize::new(0));
		let completed = dropped.clone();
		let (sender, receiver) = std::sync::mpsc::channel();
		let handle = thread::spawn(move || {
			let manager = Arc::new(ContextManager::new());
			let mut context = Context::default();
			context.module_add(
				"reentrant",
				ReentrantDrop {
					manager: Arc::downgrade(&manager),
					dropped,
				},
			);
			let id = manager.add(context).unwrap();
			manager.remove(thread::current().id(), id);
			assert!(manager.data.is_empty());
			sender.send(()).unwrap();
		});
		receiver
			.recv_timeout(Duration::from_secs(3))
			.expect("module destructor deadlocked during span removal");
		handle.join().unwrap();
		assert_eq!(completed.load(Ordering::Relaxed), 1);
	}

	#[test]
	fn masked_global_module_destructor_can_reenter_after_replacement()
	{
		let manager = Arc::new(ContextManager::new());
		let dropped = Arc::new(AtomicUsize::new(0));
		let module = Arc::new(ReentrantDrop {
			manager: Arc::downgrade(&manager),
			dropped: dropped.clone(),
		});
		let mut global = Context::default();
		global.module_add_arc("reentrant", module.clone());
		manager.global_set(global);
		let gate = Arc::new(Barrier::new(2));
		let (startedSender, startedReceiver) = std::sync::mpsc::channel();
		let (finishedSender, finishedReceiver) = std::sync::mpsc::channel();
		let producerManager = manager.clone();
		let producerGate = gate.clone();
		let handle = thread::spawn(move || {
			let mut local = Context::default();
			local.module_discard("reentrant");
			let id = producerManager.add(local).unwrap();
			startedSender.send(thread::current().id()).unwrap();
			producerGate.wait();
			drop(producerManager.resolve());
			producerManager.remove(thread::current().id(), id);
			finishedSender.send(()).unwrap();
		});
		let producerId = startedReceiver
			.recv_timeout(Duration::from_secs(3))
			.unwrap();
		// Pause resolution at the map read, after it has cloned the global snapshot.
		let stack = manager.data.get_mut(&producerId).unwrap();
		gate.wait();
		let deadline = Instant::now() + Duration::from_secs(3);
		while Arc::strong_count(&module) != 3
		{
			assert!(
				Instant::now() < deadline,
				"producer did not capture global context"
			);
			thread::yield_now();
		}
		manager.global_set(Context::default());
		drop(module);
		drop(stack);
		// Applying the local mask now destroys the last global module Arc.
		finishedReceiver
			.recv_timeout(Duration::from_secs(3))
			.expect("module destructor deadlocked during resolution");
		handle.join().unwrap();
		assert_eq!(dropped.load(Ordering::Relaxed), 1);
		assert!(manager.data.is_empty());
	}

	#[test]
	fn threshold_inherits_and_restores()
	{
		let manager = ContextManager::new();
		let mut global = Context::default();
		global.level_setMin(Some(Level::WARNING));
		manager.global_set(global);
		let threadId = thread::current().id();
		let parent = manager.add(Context::default()).unwrap();
		assert_eq!(manager.minimum_level(), Level::WARNING);
		assert_eq!(manager.resolve().level_getMin(), Some(&Level::WARNING));
		let mut child = Context::default();
		child.level_setMin(Some(Level::NORMAL));
		let child = manager.add(child).unwrap();
		assert_eq!(manager.minimum_level(), Level::NORMAL);
		manager.remove(threadId, child);
		assert_eq!(manager.minimum_level(), Level::WARNING);
		manager.remove(threadId, parent);
		assert_eq!(manager.minimum_level(), Level::WARNING);
	}

	#[test]
	fn removing_out_of_order_preserves_live_spans_and_ids()
	{
		let manager = ContextManager::new();
		let threadId = thread::current().id();
		let mut parentContext: Context = "parent".into();
		parentContext.extra_set("parent", "present");
		let parent = manager.add(parentContext).unwrap();
		let child = manager.add("child".into()).unwrap();
		manager.remove(threadId, parent);
		assert_eq!(manager.resolve().name_get().as_deref(), Some("child"));
		assert!(manager.resolve().extra_get("parent").is_none());
		let next = manager.add("next".into()).unwrap();
		assert_ne!(next, child);
		manager.remove(threadId, parent);
		assert_eq!(manager.resolve().name_get().as_deref(), Some("next"));
		manager.remove(threadId, child);
		assert_eq!(manager.resolve().name_get().as_deref(), Some("next"));
		manager.remove(threadId, next);
		assert!(manager.data.is_empty());
	}

	#[test]
	fn anonymous_and_same_named_threads_have_separate_stacks()
	{
		for threadName in [None, Some("same-name")]
		{
			let manager = Arc::new(ContextManager::new());
			let barrier = Arc::new(Barrier::new(2));
			let mut handles = Vec::new();
			for index in 0..2
			{
				let manager = manager.clone();
				let barrier = barrier.clone();
				let builder = match threadName
				{
					Some(name) => thread::Builder::new().name(name.to_owned()),
					None => thread::Builder::new(),
				};
				handles.push(
					builder
						.spawn(move || {
							let name = format!("span-{index}");
							let id = manager.add(name.clone().into()).unwrap();
							barrier.wait();
							let snapshot = manager.resolve();
							assert_eq!(snapshot.name_get().as_deref(), Some(name.as_str()));
							assert_eq!(
								snapshot.threadName_get().as_deref(),
								Some(ThreadManager::local_producerName().as_str())
							);
							barrier.wait();
							manager.remove(thread::current().id(), id);
						})
						.unwrap(),
				);
			}
			for handle in handles
			{
				handle.join().unwrap();
			}
			assert!(manager.data.is_empty());
		}
	}
}
