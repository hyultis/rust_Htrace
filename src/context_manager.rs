use crate::components::context::Context;
use crate::thread_manager::{ThreadManager, MAIN_THREAD};
use dashmap::DashMap;
use std::sync::OnceLock;

pub(crate) struct ContextManager
{
	data: DashMap<String, Vec<Context>>,
}

static SINGLETON: OnceLock<ContextManager> = OnceLock::new();

impl ContextManager
{
	/// get singleton
	pub fn singleton() -> &'static Self
	{
		return SINGLETON.get_or_init(|| Self::new());
	}

	pub fn add(&self, context: Context) -> u64
	{
		let threadName = ThreadManager::local_getName().unwrap_or(MAIN_THREAD.to_string());
		return match self.data.get_mut(&threadName)
		{
			None =>
			{
				self.data.insert(threadName, vec![context.clone()]);
				0
			}
			Some(mut inner) =>
			{
				let size = inner.len();
				inner.push(context.clone());
				size as u64
			}
		};
	}

	pub fn remove(&self, contextId: u64)
	{
		let threadName = ThreadManager::local_getName().unwrap_or(MAIN_THREAD.to_string());
		if let Some(mut inner) = self.data.get_mut(&threadName)
		{
			if(inner.iter().len()==1 && threadName==MAIN_THREAD)
			{
				return;
			}
			// remove last element until the context is removed (normally it's always the last, but this is resilient of something wrong happened)
			while (contextId < inner.iter().len() as u64)
			{
				inner.pop();
			}
		}
	}

	/// reset the global context
	pub(crate) fn global_set(&self, context: Context)
	{
		let globalName = MAIN_THREAD.to_string();
		self.data.insert(globalName,vec![context]);
	}

	/// resolve a trace context
	pub fn resolve(&self) -> Context
	{
		let Some(threadName) = ThreadManager::local_getName()
		else
		{
			return self.resolve_internal(MAIN_THREAD.to_string());
		};

		return self.resolve_internal(threadName);
	}

	////////////// PRIVATE ///////////

	fn new() -> Self
	{
		return ContextManager {
			data: DashMap::new(),
		};
	}

	/// resolve the context
	fn resolve_internal(&self, threadName: String) -> Context
	{
		// we always start from the global context
		let mut resolvedContext = self.resolve_main();

		// if this threadname have no existing context, we're using the global one
		let Some(contextArray) = self.data.get(&threadName)
		else
		{
			return resolvedContext;
		};

		// going throw each context
		contextArray.iter().for_each(|oneContext| {
			oneContext
				.modules_get()
				.iter()
				.for_each(|(key, context)| {
					match context
					{
						None => resolvedContext.module_discard(key),
						Some(inner) => resolvedContext.module_add_arc(key, inner.clone()),
					}
				});
			resolvedContext.level_setMin(oneContext.level_getMin().cloned());
			if let Some(name) = oneContext.threadName_get()
			{
				resolvedContext.threadName_set(name);
			}
		});

		return resolvedContext;
	}

	/// resolve the main context
	fn resolve_main(&self) -> Context
	{
		let Some(contextArray) = self.data.get(&MAIN_THREAD.to_string())
		else
		{
			let mut context = Context::default();
			if let Some(name) = ThreadManager::local_getName()
			{
				context.threadName_set(name);
			}
			return context;
		};

		let Some(context) = contextArray.get(0)
		else
		{
			let mut context = Context::default();
			if let Some(name) = ThreadManager::local_getName()
			{
				context.threadName_set(name);
			}
			return context;
		};

		return context.clone();
	}
}
