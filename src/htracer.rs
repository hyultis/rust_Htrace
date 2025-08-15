use std::any::Any;
use std::fmt::{Debug, Display};
use crate::components::trace::OneTrace;
use crate::components::level::Level;
use std::sync::{OnceLock};
use time::OffsetDateTime;
use crate::components::backtrace::Backtrace as hbacktrace;
use crate::components::context::Context;
use crate::context_manager::ContextManager;
use crate::thread_manager::{ThreadManager, MAIN_THREAD};

use std::{mem, thread};
use std::thread::sleep;
use std::time::Duration;
use singletonThread::SingletonThread;
use parking_lot::RwLock;
use crate::crates::bridge::HtraceBridge;

pub struct HTracer
{
	_deferredTraces: RwLock<Vec<OneTrace>>,
	_threadWriting: RwLock<SingletonThread>
}

static CONTEXTSET: OnceLock<RwLock<bool>> = OnceLock::new();
static SINGLETON: OnceLock<HTracer> = OnceLock::new();

impl HTracer
{
	pub fn singleton() -> &'static HTracer
	{
		if(*CONTEXTSET.get_or_init(|| RwLock::new(false)).read() == false) {
			panic!("[Htrace] globalContext_set() must be called before singleton()");
		}

		return SINGLETON.get_or_init(|| {
			HTracer::new()
		});
	}
	
	/// set (or override) the global context
	/// rename the local thread to MAIN_THREAD
	/// should only be call one time on the main thread before calling singleton() (another call will reset context)
	/// in case of "log_consumer" or "tracing_consumer" features, define HtraceBridge between Htrace and log or tracing (only the first time, do not change if recalled).
	pub fn globalContext_set(mut context: Context,
	                         #[cfg(any(feature = "tracing_consumer",feature = "log_consumer"))]
	                         bridge: HtraceBridge)
	{
		let contextSet = CONTEXTSET.get_or_init(|| RwLock::new(false));

		if(context.threadName_get().is_none())
		{
			context.threadName_set(MAIN_THREAD);
		}

		// auto define of the bridge is only done one time
		if(*contextSet.read() == false)
		{
			#[cfg(feature = "log_consumer")]
			{
				if let Some(minlevel) = context.level_getMin()
				{
					log::set_max_level(crate::crates::log::LogHtraceToLogLevelMapper(minlevel).to_level_filter());
				}

				if let Err(err) = log::set_boxed_logger(Box::new(bridge.clone()))
				{
					log::warn!("[Htrace] global default tracing-subscriber is already set : {}",err);
				}
			}

			#[cfg(feature = "tracing_consumer")]
			{
				use tracing_subscriber::layer::SubscriberExt;

				let subscriber = tracing_subscriber::Registry::default()
					.with(bridge);

				if let Err(err) = tracing::subscriber::set_global_default(subscriber)
				{
					tracing::warn!("[Htrace] global default tracing-subscriber is already set : {}",err);
				}
			}
		}

		ContextManager::singleton().global_set(context);
		ThreadManager::local_setName(MAIN_THREAD);
		*contextSet.write() = true;

		// one call to atleast initiate the singleton
		HTracer::singleton();

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

		let context = ContextManager::singleton().resolve();
		if(level.tou8() < context.level_getMin().unwrap_or(&Level::DEBUG).tou8()) {
			return;
		}

		let trace = OneTrace {
			message: tmp.clone(),
			date: OffsetDateTime::now_utc(),
			level,
			context,
			filename: file.to_string(),
			fileline: line,
			backtraces,
		};

		thread::spawn(move ||{
			Self::singleton()._deferredTraces.write().push(trace);
			Self::singleton()._threadWriting.write().thread_launch_delayabe();
		});
	}
	
	pub fn backtrace(base: &str) -> Vec<hbacktrace>
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

			if(filename.ends_with(base))
			{
				internal = false;
			}

			// probably the /rustc/ part is not multiplatform safe
			if !internal && solvable && !filename.starts_with("/rustc/")
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

	pub fn drop()
	{
		sleep(Duration::from_millis(1));

		Self::singleton()._threadWriting.write().loop_set(false);

		// TODO : better handling of the error ?
		let _ = Self::singleton()._threadWriting.write().wait();

		let remainTraces = Self::singleton()._deferredTraces.read().len();
		if(remainTraces > 0)
		{
			Self::singleton().internal_writeTraces();
		}
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
		let mut getWritingStuff = {
			let mut binding = Self::singleton()._deferredTraces.write();
			mem::replace(&mut *binding,vec![])
		};

		getWritingStuff.sort_by(|a,b| a.date.cmp(&b.date));
		
		for x in getWritingStuff {
			x.emit();
		}
	}
}