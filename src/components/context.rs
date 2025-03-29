use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use crate::components::level::Level;
use crate::modules::module_abstract::ModuleAbstract;
use crate::thread_manager::ThreadManager;

/// Context struct for trace.
/// A context describe how a trace should be emitted.
/// By default, a context are directly using its parent's data.
///
/// A root context in a new thread use the global context as parent.
/// The context withing the main thread as parent is the global context.
#[derive(Clone)]
pub struct Context
{
	_threadName: Option<String>,
	_modules: HashMap<String, Option<Arc<dyn ModuleAbstract>>>,
	_minlvl: Option<Level>,
}

impl Context
{
	/// add a module to this context (and its childs default)
	/// Overwrite if the name exist in this context.
	/// Also overwrite all parent context with same name.
	pub fn module_add(&mut self, name: impl Into<String>, module: impl ModuleAbstract + Sync + Send + 'static)
	{
		let name = name.into();
		self._modules.insert(name, Some(Arc::new(module)));
	}

	/// same as module_add, but with already and module in an "Arc"
	pub fn module_add_arc(&mut self, name: impl Into<String>, module: Arc<dyn ModuleAbstract + Sync + Send + 'static>)
	{
		let name = name.into();
		self._modules.insert(name, Some(module));
	}

	/// discard a parent module (discard it also for childs)
	pub fn module_discard(&mut self, name: impl Into<String>)
	{
		let name = name.into();
		self._modules.insert(name, None);
	}

	pub fn modules_get(&self) -> &HashMap<String, Option<Arc<dyn ModuleAbstract>>>
	{
		return &self._modules;
	}

	/// set minimum level for this context, overwriting parent one
	/// use NONE if you want to use the parent minimum level (default)
	pub fn level_setMin(&mut self, min: Option<Level>)
	{
		self._minlvl = min;
	}

	/// get minimum level for this context
	pub fn level_getMin(&self) -> Option<&Level>
	{
		self._minlvl.as_ref()
	}

	/// set thread name
	pub fn threadName_set(&mut self, threadName: impl Into<String>)
	{
		self._threadName = Some(threadName.into());
	}

	/// get thread name
	pub fn threadName_get(&self) -> &Option<String>
	{
		return &self._threadName;
	}
}

impl Default for Context
{
	fn default() -> Self {
		return Self {
			_threadName: ThreadManager::local_getName(),
			_modules: Default::default(),
			_minlvl: None,
		};
	}
}

impl Debug for Context
{
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Context")
			.field("modules", &self._modules.keys().collect::<Vec<_>>())
			.field("minlvl", &self._minlvl)
			.field("threadName", &self._threadName)
			.finish()
	}
}

impl Drop for Context
{
	fn drop(&mut self) {
		self._modules.iter().for_each(|(_, module)| {
			if let Some(module) = module {
				module.event_onContextExit(&self);
			}
		})
	}
}

impl Into<Context> for &str
{
	fn into(self) -> Context {
		let mut context = Context::default();
		context.threadName_set(self);
		context
	}
}

impl Into<Context> for String
{
	fn into(self) -> Context {
		let mut context = Context::default();
		context.threadName_set(self);
		context
	}
}