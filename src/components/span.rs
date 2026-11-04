use crate::components::context::Context;
use crate::context_manager::ContextManager;
use std::thread::{self, ThreadId};

/// Guard defining the logical lifetime of a context on its creating thread.
/// If the process exhausts its unique span identifiers, new guards remain inactive.
///
/// Moving a guard keeps it attached to that thread; dropping it removes only
/// its own context. Dropping guards out of order leaves other spans active.
/// The exit snapshot contains this span and its surviving ancestors, excluding
/// its children. Exit callbacks run on the trace worker after earlier accepted
/// traces and before a later flush/shutdown barrier.
///
/// Spans follow threads and do not propagate across asynchronous task switches.
/// Keep the guard within the synchronous scope whose traces it should affect.
pub struct Span
{
	_contextId: Option<u64>,
	_threadId: ThreadId,
}

impl Span
{
	pub fn new(context: Context) -> Self
	{
		let id = ContextManager::singleton().add(context);
		Self {
			_contextId: id,
			_threadId: thread::current().id(),
		}
	}
}

impl Drop for Span
{
	fn drop(&mut self)
	{
		if let Some(contextId) = self._contextId
		{
			ContextManager::singleton().remove(self._threadId, contextId);
		}
	}
}

/// Define a context for subtrace
/// Use a short-lived Span to install a context and remove it when the Span is dropped.
/// You can set a context (see "Context") like this :
/// ```
///  use htrace::components::context::Context;
///  use htrace::Spaned;
///  let mut context = Context::default();
///  context.name_set("span lvl1");
///  Spaned!(context);
/// ```
///
/// or using a str/string to directly set the thread name
/// `Spaned!("thread context");`
#[macro_export]
macro_rules! Spaned {
	() => {
		let _span =
			$crate::components::span::Span::new($crate::components::context::Context::default());
	};
	($a:expr) => {
		let _span = $crate::components::span::Span::new($a.into());
	};
}
