use std::collections::HashMap;

pub static MAIN_THREAD: &str = "MAIN";

#[cfg(feature = "threading")]
use std::cell::RefCell;
#[cfg(feature = "threading")]
use std::thread::AccessError;

#[cfg(not(feature = "threading"))]
use std::sync::{LazyLock, Mutex};
#[cfg(not(feature = "threading"))]
static NAME: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
#[cfg(not(feature = "threading"))]
static EXTRADATAS: LazyLock<Mutex<HashMap<String,String>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// used for storing thread name, useless for wasm
pub struct ThreadManager
{
}

impl ThreadManager
{
	#[cfg(feature = "threading")]
	thread_local!{
        static NAME: RefCell<Option<String>> = RefCell::new(None);
        static EXTRADATAS: RefCell<HashMap<String,String>> = RefCell::new(HashMap::new());
	}

	/// get local thread name
	pub fn local_getName() -> Option<String>
	{
		#[cfg(feature = "threading")]
		{
			return Self::NAME.try_with(|a| a.borrow().clone()).unwrap_or_else(|_| None)
		}
		#[cfg(not(feature = "threading"))]
		{
			return NAME.lock().unwrap().clone();
		}
	}

	/// set local thread name
	pub fn local_setName(name: impl Into<String>)
	{
		#[cfg(feature = "threading")]
		{
			Self::NAME.set(Some(name.into()));
		}
		#[cfg(not(feature = "threading"))]
		{
			*NAME.lock().unwrap() = Some(name.into());
		}
	}

	/// get local thread extra data
	pub fn local_getExtraDatas(extraDataName: impl Into<String>) -> Option<String>
	{
		let extraDataName= extraDataName.into();
		#[cfg(feature = "threading")]
		{
			return Self::EXTRADATAS.try_with(|a| a.borrow().get(&extraDataName).cloned()).unwrap_or_else(|_| None);
		}
		#[cfg(not(feature = "threading"))]
		{
			return EXTRADATAS.lock().unwrap().get(&extraDataName).cloned();
		}
	}

	#[cfg(feature = "threading")]
	/// set local thread extra data
	pub fn local_setExtraDatas(extraDataName: impl Into<String>, content: impl Into<String>) -> Result<Option<String>, AccessError>
	{
		let extraDataName = extraDataName.into();
		let content = content.into();
		return Self::EXTRADATAS.try_with(|a| a.borrow_mut().insert(extraDataName,content));
	}

	#[cfg(not(feature = "threading"))]
	/// set local thread extra data
	pub fn local_setExtraDatas(extraDataName: impl Into<String>, content: impl Into<String>) -> Option<String>
	{
		let extraDataName = extraDataName.into();
		let content = content.into();
		return EXTRADATAS.lock().unwrap().insert(extraDataName,content);
	}
}