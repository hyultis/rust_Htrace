use std::cell::RefCell;

pub static MAIN_THREAD: &str = "MAIN";

pub struct ThreadManager
{
}

impl ThreadManager
{
	thread_local!{
        static NAME: RefCell<Option<String>> = RefCell::new(None);
	}

	/// get local thread name
	pub fn local_getName() -> Option<String>
	{
		return Self::NAME.try_with(|a| a.borrow().clone()).unwrap_or_else(|_| None)
	}

	/// set local thread name
	pub fn local_setName(name: impl Into<String>)
	{
		let name = name.into();
		Self::NAME.set(Some(name));
	}

	/*/// get local thread id
	pub fn local_getId() -> u64
	{
		let mut hasher = DefaultHasher::new();
		thread::current().id().hash(&mut hasher);
		return hasher.finish();
	}*/
}