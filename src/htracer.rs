use std::any::Any;
use std::fmt::{Debug, Display};
use crate::components::trace::OneTrace;
use crate::components::level::Level;
use std::sync::{OnceLock};
use std::{thread};
use parking_lot::{RwLock};
use singletonThread::SingletonThread;
use time::OffsetDateTime;
use crate::components::backtrace::Backtrace as hbacktrace;
use crate::components::context::Context;
use crate::context_manager::ContextManager;
use crate::thread_manager::{ThreadManager, MAIN_THREAD};

pub struct HTracer
{
	_deferredTraces: RwLock<Vec<OneTrace>>,
	_threadWriting: RwLock<SingletonThread>
}

static SINGLETON: OnceLock<HTracer> = OnceLock::new();

impl HTracer
{
	pub fn singleton() -> &'static HTracer
	{
		return SINGLETON.get_or_init(|| {
			HTracer::new()
		});
	}
	
	/// set (or override) the global context
	/// rename the local thread to MAIN_THREAD
	/// should only be call one time on the main thread
	pub fn globalContext_set(mut context: Context)
	{
		if(context.threadName_get().is_none())
		{
			context.threadName_set(MAIN_THREAD);
		}
		ContextManager::singleton().global_set(context);
		ThreadManager::local_setName(MAIN_THREAD);
	}
	
	pub fn trace<T>(rawEntry : &T, level: Level, file: &str, line: u32, backtraces: Vec<hbacktrace>)
		where T: Any + Debug // + ?Display
	{
		let anyEntry = rawEntry as &dyn Any;
		let tmp = if let Some(content) = anyEntry.downcast_ref::<String>() {
			content.to_string()
		}
		else
		{
			if let Some(content) = anyEntry.downcast_ref::<&str>()
			{
				content.to_string()
			}
			else
			{
				if let Some(content)= anyEntry.downcast_ref::<Box<dyn Display>>()
				{
					format!("{}", content)
				}
				else
				{
					format!("{:?}", rawEntry)
				}
			}
		};

		let file = file.to_string();
		let time = OffsetDateTime::now_utc();
		let context = ContextManager::singleton().resolve();
		let trace = OneTrace {
			message: tmp,
			date: time,
			level,
			context,
			filename: file,
			fileline: line,
			backtraces,
		};

		thread::spawn(move ||{
			Self::singleton()._deferredTraces.write().push(trace);
			Self::singleton()._threadWriting.write().thread_launch_delayabe();
		});
	}
	
	pub fn backtrace() -> Vec<hbacktrace>
	{
		let mut internal = true;
		let mut returning = Vec::new();
		backtrace::trace(|x|{
			let mut name = "".to_string();
			let mut filename = "".to_string();
			let mut line = 0;
			let mut solvable = false;
			
			backtrace::resolve_frame(x, |symbol| {
				if let Some(inname) = symbol.name()
				{
					if let Some((splitted,_)) = inname.to_string().rsplit_once("::")
					{
						solvable = true;
						name = format!("{}()",splitted);
					}
				}
				if let Some(infilename) = symbol.filename()
				{
					filename = infilename.to_str().unwrap_or_default().to_string();
				}
				if let Some(inline) = symbol.lineno()
				{
					line = inline;
				}
			});

			if(name.eq("Htrace::htracer::HTracer::backtrace()"))
			{
				internal = false;
			}
			else if !internal && solvable
			{
				returning.push(hbacktrace {
					funcName: name,
					fileName: filename,
					line,
				});
			}
			true
		});
		
		return returning;
	}
	
	//////////// PRIVATE ///////////

	fn new() -> HTracer {

		let thread = SingletonThread::new(||{
			Self::singleton().internal_writeTraces();
		});

		return HTracer {
			_deferredTraces: RwLock::new(vec![]),
			_threadWriting: RwLock::new(thread),
		};
	}
	
	fn internal_writeTraces(&self)
	{
		let mut binding = Self::singleton()._deferredTraces.write();
		let getWritingStuff = binding.drain(0..).collect::<Vec<_>>();
		
		for x in getWritingStuff {
			x.emit();
		}
	}
}
