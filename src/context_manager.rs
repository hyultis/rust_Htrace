use crate::components::context::Context;
use crate::components::level::Level;
use crate::htracer::HTracer;
use crate::thread_manager::MAIN_THREAD;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use parking_lot::RwLock;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, ThreadId};

/// One installed span. `resolved` is computed once at entry (and recomputed
/// only when an ancestor exits out of order or the global context changes):
/// global base, surviving ancestors and this span, without its children.
struct ContextEntry
{
	id: u64,
	/// The span's own context, as given by the user.
	delta: Context,
	resolved: Arc<Context>,
}

/// Span stack of one producer thread.
struct ThreadStack
{
	entries: Vec<ContextEntry>,
	/// Global generation the resolved chain was computed with.
	globalGeneration: u64,
	/// Shared with the owning thread's cache; bumped by every add/remove, which
	/// may run on another thread when a span is moved before being dropped.
	generation: Arc<AtomicU64>,
}

/// Global context and its resolved base (producer name forced to `MAIN`).
struct GlobalState
{
	context: Option<Context>,
	resolved: Arc<Context>,
	generation: u64,
}

/// Resolution cached by a producer thread, valid while the manager, the global
/// generation and the thread's stack generation are unchanged.
struct CachedResolution
{
	manager: u64,
	globalGeneration: u64,
	stackGeneration: u64,
	context: Arc<Context>,
	level: Level,
}

struct Producer
{
	threadId: ThreadId,
	stackGeneration: Arc<AtomicU64>,
	cache: RefCell<Option<CachedResolution>>,
}

thread_local! {
	static PRODUCER: Producer = Producer {
		threadId: thread::current().id(),
		stackGeneration: Arc::new(AtomicU64::new(0)),
		cache: RefCell::new(None),
	};
}

pub(crate) struct ContextManager
{
	/// Distinguishes managers in the thread caches (tests build several).
	id: u64,
	data: DashMap<ThreadId, ThreadStack>,
	global: RwLock<GlobalState>,
	/// Mirror of `GlobalState::generation`, read without locking.
	globalGeneration: AtomicU64,
	nextId: AtomicU64,
}

static SINGLETON: OnceLock<ContextManager> = OnceLock::new();
static NEXT_MANAGER: AtomicU64 = AtomicU64::new(0);

impl ContextManager
{
	/// get singleton
	pub fn singleton() -> &'static Self
	{
		return SINGLETON.get_or_init(Self::new);
	}

	/// Install a span on the current thread and resolve it against its parent
	/// (or the global base when it is the first span).
	pub fn add(&self, context: Context) -> Option<u64>
	{
		// Keep compatibility with stable compilers predating the try_update rename.
		#[allow(deprecated)]
		let id = self
			.nextId
			.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
			.ok()?;
		let threadId = thread::current().id();
		let mut base: Option<(u64, Arc<Context>)> = None;
		let mut released = Vec::new();
		loop
		{
			let current = self.globalGeneration.load(Ordering::Acquire);
			let mut stack = self.data.entry(threadId).or_insert_with(ThreadStack::new);
			if stack.entries.is_empty() || stack.globalGeneration != current
			{
				match &base
				{
					Some((generation, base)) if *generation == current =>
					{
						Self::rechain(base, &mut stack.entries, &mut released);
						stack.globalGeneration = current;
					}
					_ =>
					{
						drop(stack);
						base = Some(self.global_base());
						continue;
					}
				}
			}
			let resolved = match (stack.entries.last(), &base)
			{
				(Some(parent), _) => parent.resolved.resolve_child(&context),
				(None, Some((_, base))) => base.resolve_child(&context),
				(None, None) =>
				{
					drop(stack);
					base = Some(self.global_base());
					continue;
				}
			};
			stack.entries.push(ContextEntry {
				id,
				delta: context,
				resolved: Arc::new(resolved),
			});
			stack.generation.fetch_add(1, Ordering::Release);
			break;
		}
		// Replaced snapshots may hold the last module Arc: release them unlocked.
		drop(released);
		return Some(id);
	}

	/// Remove only this span, even when another span was entered after it.
	/// Its exit snapshot is its resolved context: the surviving ancestors and the
	/// span itself, excluding children that remain active. Callbacks run on the
	/// trace worker.
	pub fn remove(&self, threadId: ThreadId, contextId: u64)
	{
		if let Some(exit) = self.detach(threadId, contextId)
		{
			HTracer::context_exit(exit);
		}
	}

	/// Remove a span and return its exit snapshot, without notifying modules.
	fn detach(&self, threadId: ThreadId, contextId: u64) -> Option<Arc<Context>>
	{
		let mut base: Option<(u64, Arc<Context>)> = None;
		let mut released = Vec::new();
		let removed = loop
		{
			let current = self.globalGeneration.load(Ordering::Acquire);
			let Entry::Occupied(mut entry) = self.data.entry(threadId)
			else
			{
				break None;
			};
			let stack = entry.get_mut();
			let Some(position) = stack.entries.iter().position(|entry| entry.id == contextId)
			else
			{
				break None;
			};
			let stale = stack.globalGeneration != current;
			let needsBase = stale || (position == 0 && stack.entries.len() > 1);
			let base = match &base
			{
				Some((generation, base)) if *generation == current => Some(base),
				_ if needsBase =>
				{
					drop(entry);
					base = Some(self.global_base());
					continue;
				}
				_ => None,
			};
			if stale && let Some(base) = base
			{
				Self::rechain(base, &mut stack.entries, &mut released);
				stack.globalGeneration = current;
			}
			let removed = stack.entries.remove(position);
			// Spans entered after it had included it: resolve them again.
			let (below, above) = stack.entries.split_at_mut(position);
			match below.last()
			{
				Some(parent) => Self::rechain(&parent.resolved, above, &mut released),
				None =>
				{
					if let Some(base) = base
					{
						Self::rechain(base, above, &mut released);
						stack.globalGeneration = current;
					}
				}
			}
			stack.generation.fetch_add(1, Ordering::Release);
			if stack.entries.is_empty()
			{
				entry.remove();
			}
			break Some(removed);
		};

		// Releasing a snapshot can drop the last module Arc, whose user-defined
		// Drop may reenter Htrace: everything is released outside the map lock.
		drop(released);
		if PRODUCER
			.try_with(|producer| producer.threadId == threadId)
			.unwrap_or(false)
		{
			self.cache_release();
		}
		drop(base);
		let ContextEntry {
			resolved, delta, ..
		} = removed?;
		drop(delta);
		Some(resolved)
	}

	/// Replace the global configuration and enqueue exactly one logical exit
	/// for the previous configuration, after previously accepted traces.
	#[cfg(test)]
	pub(crate) fn global_set(&self, context: Context)
	{
		if let Some(previous) = self.global_replace(context)
		{
			HTracer::context_exit(Arc::new(previous));
		}
	}

	/// Replace the global configuration and return the previous one. The caller
	/// must enqueue its logical exit, outside any lock its destructors may need.
	/// Producers see the new configuration at their next trace.
	pub(crate) fn global_replace(&self, context: Context) -> Option<Context>
	{
		let resolved = Arc::new(Self::base_of(Some(&context)));
		let (previous, previousResolved) = {
			let mut global = self.global.write();
			let generation = global.generation.wrapping_add(1);
			global.generation = generation;
			let previousResolved = std::mem::replace(&mut global.resolved, resolved);
			let previous = global.context.replace(context);
			self.globalGeneration.store(generation, Ordering::Release);
			(previous, previousResolved)
		};
		drop(previousResolved);
		self.cache_release();
		return previous;
	}

	/// Resolved snapshot for the current producer, from global to innermost span.
	/// Cached per thread: unchanged spans and global cost no lock nor merge.
	pub fn resolve(&self) -> Arc<Context>
	{
		return self.lookup(|cached| cached.context.clone());
	}

	/// Effective threshold of the current producer, from the same cache.
	pub(crate) fn minimum_level(&self) -> Level
	{
		return self.lookup(|cached| cached.level);
	}

	////////////// PRIVATE ///////////

	fn new() -> Self
	{
		return Self {
			id: NEXT_MANAGER.fetch_add(1, Ordering::Relaxed),
			data: DashMap::new(),
			global: RwLock::new(GlobalState {
				context: None,
				resolved: Arc::new(Self::base_of(None)),
				generation: 0,
			}),
			globalGeneration: AtomicU64::new(0),
			nextId: AtomicU64::new(0),
		};
	}

	/// The producer name is `MAIN` unless a span sets one; the innermost wins.
	/// The global context name and the native OS thread name are ignored.
	fn base_of(global: Option<&Context>) -> Context
	{
		let mut base = global.cloned().unwrap_or_default();
		base.threadName_set(MAIN_THREAD);
		base
	}

	fn global_base(&self) -> (u64, Arc<Context>)
	{
		let global = self.global.read();
		(global.generation, global.resolved.clone())
	}

	/// Resolve `entries` again, each from the previous one, the first from
	/// `parent`. Replaced snapshots are moved to `released`.
	fn rechain(parent: &Context, entries: &mut [ContextEntry], released: &mut Vec<Arc<Context>>)
	{
		for index in 0..entries.len()
		{
			let resolved = match index
				.checked_sub(1)
				.and_then(|previous| entries.get(previous))
			{
				Some(previous) => previous.resolved.resolve_child(&entries[index].delta),
				None => parent.resolve_child(&entries[index].delta),
			};
			released.push(std::mem::replace(
				&mut entries[index].resolved,
				Arc::new(resolved),
			));
		}
	}

	/// Hot path: one thread-local access and two atomic loads when the cache is
	/// valid. Otherwise resolve once and refresh the cache. During thread-local
	/// destruction, resolve without caching.
	fn lookup<T>(&self, read: impl Fn(&CachedResolution) -> T) -> T
	{
		let globalGeneration = self.globalGeneration.load(Ordering::Acquire);
		let cached = PRODUCER.try_with(|producer| {
			let stackGeneration = producer.stackGeneration.load(Ordering::Acquire);
			let hit = match producer.cache.try_borrow()
			{
				Ok(cache) => cache
					.as_ref()
					.filter(|cached| {
						cached.manager == self.id
							&& cached.globalGeneration == globalGeneration
							&& cached.stackGeneration == stackGeneration
					})
					.map(&read),
				Err(_) => None,
			};
			(producer.threadId, stackGeneration, hit)
		});
		match cached
		{
			Ok((_, _, Some(value))) => value,
			Ok((threadId, stackGeneration, None)) =>
			{
				let fresh = self.compute(threadId, globalGeneration, stackGeneration);
				let value = read(&fresh);
				let previous = PRODUCER.try_with(|producer| {
					producer
						.cache
						.try_borrow_mut()
						.ok()
						.and_then(|mut cache| cache.replace(fresh))
				});
				// The previous snapshot may hold the last module Arc.
				drop(previous);
				value
			}
			Err(_) =>
			{
				let fresh = self.compute(thread::current().id(), globalGeneration, 0);
				read(&fresh)
			}
		}
	}

	/// Generations are read by the caller before the stack, so a concurrent
	/// change can only make the result look outdated, never stale-but-valid.
	fn compute(
		&self,
		threadId: ThreadId,
		globalGeneration: u64,
		stackGeneration: u64,
	) -> CachedResolution
	{
		let context = self.resolve_uncached(threadId, globalGeneration);
		CachedResolution {
			manager: self.id,
			globalGeneration,
			stackGeneration,
			level: context.level_getMin().copied().unwrap_or(Level::DEBUG),
			context,
		}
	}

	fn resolve_uncached(&self, threadId: ThreadId, current: u64) -> Arc<Context>
	{
		if let Some(stack) = self.data.get(&threadId)
			&& stack.globalGeneration == current
			&& let Some(top) = stack.entries.last()
		{
			return top.resolved.clone();
		}
		// The chain was resolved with an older global: resolve it again, once.
		let (generation, base) = self.global_base();
		let mut released = Vec::new();
		let top = match self.data.get_mut(&threadId)
		{
			Some(mut stack) if !stack.entries.is_empty() =>
			{
				if stack.globalGeneration != generation
				{
					Self::rechain(&base, &mut stack.entries, &mut released);
					stack.globalGeneration = generation;
				}
				stack.entries.last().map(|top| top.resolved.clone())
			}
			_ => None,
		};
		drop(released);
		top.unwrap_or(base)
	}

	/// Drop the current thread's cached snapshot, outside its borrow.
	fn cache_release(&self)
	{
		let previous = PRODUCER.try_with(|producer| {
			producer
				.cache
				.try_borrow_mut()
				.ok()
				.and_then(|mut cache| cache.take())
		});
		drop(previous);
	}
}

impl ThreadStack
{
	fn new() -> Self
	{
		let generation = PRODUCER
			.try_with(|producer| producer.stackGeneration.clone())
			.unwrap_or_else(|_| Arc::new(AtomicU64::new(0)));
		Self {
			entries: Vec::new(),
			globalGeneration: 0,
			generation,
		}
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
	use std::time::Duration;

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

	/// Runs `producer` on a thread holding a span over a global that owns a
	/// reentrant module, replaces the global meanwhile, and checks that the
	/// producer releasing the last module Arc can reenter the manager.
	fn replace_global_while_producer_holds_module(mask: bool)
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
		let (finishedSender, finishedReceiver) = std::sync::mpsc::channel();
		let producerManager = manager.clone();
		let producerGate = gate.clone();
		let handle = thread::spawn(move || {
			// Cache the global base (and its module) in this thread.
			drop(producerManager.resolve());
			let mut local = Context::default();
			if mask
			{
				local.module_discard("reentrant");
			}
			let id = producerManager.add(local).unwrap();
			producerGate.wait();
			producerGate.wait();
			// The next resolution sees the new global and releases the old
			// snapshots: the last module Arc is dropped here.
			let resolved = producerManager.resolve();
			assert!(
				resolved
					.modules_get()
					.get("reentrant")
					.cloned()
					.flatten()
					.is_none()
			);
			drop(resolved);
			producerManager.remove(thread::current().id(), id);
			finishedSender.send(()).unwrap();
		});
		gate.wait();
		manager.global_set(Context::default());
		drop(module);
		assert_eq!(dropped.load(Ordering::Relaxed), 0);
		gate.wait();
		finishedReceiver
			.recv_timeout(Duration::from_secs(3))
			.expect("module destructor deadlocked during resolution");
		handle.join().unwrap();
		assert_eq!(dropped.load(Ordering::Relaxed), 1);
		assert!(manager.data.is_empty());
	}

	#[test]
	fn masked_global_module_destructor_can_reenter_after_replacement()
	{
		replace_global_while_producer_holds_module(true);
	}

	#[test]
	fn inherited_global_module_destructor_can_reenter_after_replacement()
	{
		replace_global_while_producer_holds_module(false);
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
							assert_eq!(snapshot.threadName_get().as_deref(), Some(MAIN_THREAD));
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

	/// Both maps are the same shared allocation.
	fn shares_maps(left: &Context, right: &Context) -> bool
	{
		std::ptr::eq(left.modules_get(), right.modules_get())
			&& std::ptr::eq(left.extra_getAll(), right.extra_getAll())
	}

	#[test]
	fn cache_follows_span_entry_and_exit()
	{
		let manager = ContextManager::new();
		let threadId = thread::current().id();
		let base = manager.resolve();
		assert!(Arc::ptr_eq(&base, &manager.resolve()));
		assert_eq!(base.threadName_get().as_deref(), Some(MAIN_THREAD));
		let mut spanContext: Context = "span".into();
		spanContext.level_setMin(Some(Level::ERROR));
		let span = manager.add(spanContext).unwrap();
		assert_eq!(manager.minimum_level(), Level::ERROR);
		let inside = manager.resolve();
		assert_eq!(inside.name_get().as_deref(), Some("span"));
		assert!(Arc::ptr_eq(&inside, &manager.resolve()));
		manager.remove(threadId, span);
		assert_eq!(manager.minimum_level(), Level::DEBUG);
		assert!(manager.resolve().name_get().is_none());
		assert!(manager.data.is_empty());
	}

	#[test]
	fn cached_thread_sees_global_replaced_elsewhere()
	{
		let manager = Arc::new(ContextManager::new());
		let span = manager.add("span".into()).unwrap();
		assert!(manager.resolve().extra_get("global").is_none());
		assert_eq!(manager.minimum_level(), Level::DEBUG);
		let other = manager.clone();
		thread::spawn(move || {
			let mut global = Context::default();
			global.extra_set("global", "new");
			global.level_setMin(Some(Level::WARNING));
			global.threadName_set("ignored");
			other.global_set(global);
		})
		.join()
		.unwrap();
		let resolved = manager.resolve();
		assert_eq!(
			resolved.extra_get("global").map(String::as_str),
			Some("new")
		);
		assert_eq!(resolved.name_get().as_deref(), Some("span"));
		assert_eq!(resolved.threadName_get().as_deref(), Some(MAIN_THREAD));
		assert_eq!(manager.minimum_level(), Level::WARNING);
		let exit = manager.detach(thread::current().id(), span).unwrap();
		assert_eq!(exit.extra_get("global").map(String::as_str), Some("new"));
	}

	#[test]
	fn span_dropped_on_another_thread_leaves_creator_cache()
	{
		let manager = Arc::new(ContextManager::new());
		let threadId = thread::current().id();
		let span = manager.add("moved".into()).unwrap();
		assert_eq!(manager.resolve().name_get().as_deref(), Some("moved"));
		let other = manager.clone();
		let exit = thread::spawn(move || other.detach(threadId, span))
			.join()
			.unwrap()
			.unwrap();
		assert_eq!(exit.name_get().as_deref(), Some("moved"));
		assert!(manager.resolve().name_get().is_none());
		assert!(manager.data.is_empty());
	}

	#[test]
	fn out_of_order_exit_snapshot_and_survivor()
	{
		let manager = ContextManager::new();
		let threadId = thread::current().id();
		let mut global = Context::default();
		global.extra_set("global", "g");
		manager.global_set(global);
		let mut first: Context = "A".into();
		first.extra_set("a", "a");
		let first = manager.add(first).unwrap();
		let mut second = Context::default();
		second.extra_set("b", "b");
		let second = manager.add(second).unwrap();
		assert_eq!(
			manager.resolve().extra_get("a").map(String::as_str),
			Some("a")
		);
		let exit = manager.detach(threadId, first).unwrap();
		assert_eq!(exit.name_get().as_deref(), Some("A"));
		assert_eq!(exit.extra_get("global").map(String::as_str), Some("g"));
		assert_eq!(exit.extra_get("a").map(String::as_str), Some("a"));
		assert!(exit.extra_get("b").is_none());
		let survivor = manager.resolve();
		assert!(survivor.name_get().is_none());
		assert!(survivor.extra_get("a").is_none());
		assert_eq!(survivor.extra_get("b").map(String::as_str), Some("b"));
		assert_eq!(survivor.extra_get("global").map(String::as_str), Some("g"));
		let exit = manager.detach(threadId, second).unwrap();
		assert!(exit.extra_get("a").is_none());
		assert!(manager.data.is_empty());
	}

	#[test]
	fn child_without_modules_or_extras_shares_parent_maps()
	{
		let manager = ContextManager::new();
		let threadId = thread::current().id();
		let mut global = Context::default();
		global.extra_set("global", "g");
		manager.global_set(global);
		let base = manager.global_base().1;
		let named = manager.add("named".into()).unwrap();
		let namedResolved = manager.resolve();
		assert!(shares_maps(&namedResolved, &base));
		let mut extra = Context::default();
		extra.extra_set("child", "c");
		let extra = manager.add(extra).unwrap();
		let extraResolved = manager.resolve();
		assert!(!shares_maps(&extraResolved, &namedResolved));
		assert!(std::ptr::eq(
			extraResolved.modules_get(),
			namedResolved.modules_get()
		));
		let mut leveled = Context::default();
		leveled.level_setMin(Some(Level::ERROR));
		let leveled = manager.add(leveled).unwrap();
		assert!(shares_maps(&manager.resolve(), &extraResolved));
		for id in [leveled, extra, named]
		{
			manager.remove(threadId, id);
		}
	}

	struct LateTrace
	{
		manager: Arc<ContextManager>,
		sender: std::sync::mpsc::Sender<Option<String>>,
	}

	impl Drop for LateTrace
	{
		fn drop(&mut self)
		{
			let id = self.manager.add("late".into());
			self.manager.minimum_level();
			let name = self.manager.resolve().name_get().clone();
			if let Some(id) = id
			{
				self.manager.remove(thread::current().id(), id);
			}
			let _ = self.sender.send(name);
		}
	}

	thread_local! {
		static LATE: RefCell<Option<LateTrace>> = const { RefCell::new(None) };
	}

	#[test]
	fn trace_from_thread_local_destructor_does_not_panic()
	{
		// Both registration orders: the producer cache destroyed before or
		// after the destructor that traces.
		for cacheFirst in [false, true]
		{
			let manager = Arc::new(ContextManager::new());
			let (sender, receiver) = std::sync::mpsc::channel();
			let producerManager = manager.clone();
			thread::spawn(move || {
				if cacheFirst
				{
					producerManager.resolve();
				}
				LATE.with(|late| {
					*late.borrow_mut() = Some(LateTrace {
						manager: producerManager.clone(),
						sender,
					});
				});
				producerManager.resolve();
			})
			.join()
			.unwrap();
			assert_eq!(
				receiver
					.recv_timeout(Duration::from_secs(3))
					.unwrap()
					.as_deref(),
				Some("late")
			);
			assert!(manager.data.is_empty());
		}
	}
}
