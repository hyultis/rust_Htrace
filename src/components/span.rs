use crate::components::context::Context;
use crate::context_manager::ContextManager;

/// "short" lived structure, used to define the lifetime of a context
pub struct Span
{
	_contextId: u64
}

impl Span
{
	pub fn new(context: Context) -> Self
	{
		let id = ContextManager::singleton().add(context);
		Self {
			_contextId: id
		}
	}
}

impl Drop for Span
{
	fn drop(&mut self) {
		ContextManager::singleton().remove(self._contextId);
	}
}

/// Define a context for subtrace
/// Use a "short" lived "Span" that crate a context en delete it when "Span" drop
/// You can set a context (see "Context") like this :
/// ```
///  use Htrace::components::context::Context;
///  use Htrace::Spaned;
///  let mut context = Context::default();
///  context.name_set("span lvl1");
///  Spaned!(context);
/// ```
///
/// or using a str/string to directly set the thread name
/// `Spaned!("thread context");`
#[macro_export]
macro_rules! Spaned
{
	() => {
		let _span = $crate::components::span::Span::new($crate::components::context::Context::default());
	};
	($a:expr) => {
		let _span = $crate::components::span::Span::new($a.into());
    };
}
