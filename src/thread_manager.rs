use std::cell::RefCell;
use std::collections::HashMap;
use std::thread::AccessError;

pub static MAIN_THREAD: &str = "MAIN";

pub struct ThreadManager
{
}

impl ThreadManager
{
	thread_local!{
        static NAME: RefCell<Option<String>> = RefCell::new(None);
        static EXTRADATAS: RefCell<HashMap<String,String>> = RefCell::new(HashMap::new());
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

	/// get local thread extra data
	pub fn local_getExtraDatas(extraDataName: impl Into<String>) -> Option<String>
	{
		let extraDataName= extraDataName.into();
		return Self::EXTRADATAS.try_with(|a| a.borrow().get(&extraDataName).cloned()).unwrap_or_else(|_| None)
	}

	/// set local thread extra data
	pub fn local_setExtraDatas(extraDataName: impl Into<String>, content: impl Into<String>) -> Result<Option<String>, AccessError>
	{
		let extraDataName = extraDataName.into();
		let content = content.into();
		return Self::EXTRADATAS.try_with(|a| a.borrow_mut().insert(extraDataName,content));
	}
}