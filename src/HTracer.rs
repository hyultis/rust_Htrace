use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use crate::ModuleAbstract::ModuleAbstract;
use chrono::Local;
use crate::OneLog::OneLog;
use crate::Type::Type;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use Hconfig::HConfigManager::HConfigManager;
use Hconfig::serde_json;
use Hconfig::serde_json::Value;
use parking_lot::{RwLock};
use singletonThread::SingletonThread;
use crate::backtrace::Hbacktrace;
use crate::Errors;

pub struct HTracer
{
	_modules: DashMap<String, HashMap<u64,Box<dyn ModuleAbstract + Sync + Send>> >,
	_deferredLog: RwLock<Vec<OneLog>>,
	_threadNames: DashMap<u64,String>,
	_minlvl: ArcSwap<u8>,
	_threadWriting: RwLock<SingletonThread>
}

static SINGLETON: OnceLock<HTracer> = OnceLock::new();

impl HTracer
{
	fn new() -> HTracer {
		let tmp = DashMap::new();
		tmp.insert(0,"main".to_string());
		
		let mut thread = SingletonThread::new(||{
			Self::singleton().internal_writestuff();
		});
		thread.setDuration(Duration::from_nanos(1));
		
		return HTracer {
			_modules: DashMap::new(),
			_deferredLog: RwLock::new(vec![]),
			_threadNames: tmp,
			_minlvl: ArcSwap::new(Arc::new(0)),
			_threadWriting: RwLock::new(thread),
		};
	}
	
	pub fn singleton() -> &'static HTracer
	{
		return SINGLETON.get_or_init(|| {
			HTracer::new()
		});
	}
	
	// add a ModuleAbstract to tracing system
	pub fn appendModule(modulename: &str,newmodule : impl ModuleAbstract + Send + Sync + 'static) -> Result<(),Errors>
	{
		return HTracer::appendModuleToThread(modulename,newmodule,0);
	}
	
	// add a ModuleAbstract to tracing system but only for a specific thread (use Tracer::getThread to get a id)
	pub fn appendModuleToThread(modulename: &str,mut newmodule : impl ModuleAbstract + Send + Sync + 'static, threadId: u64) -> Result<(),Errors>
	{
		let modulename = modulename.to_string();
		let modulepath = format!("module/{}",modulename);
		let mut tracerC = HConfigManager::singleton().get("htrace");
		tracerC.getOrSetDefault(modulepath.as_str(),Value::Object(serde_json::Map::new()));
		
		let mut tmp = Ok(());
		if let Some(node) = tracerC.get_mut(modulepath.as_str())
		{
			tmp = newmodule.setConfig(node);
		};
		tracerC.save().unwrap();
		tmp.map_err(|err|Errors::ModuleConfigError(modulename.to_string(),err))?;
		newmodule.setModuleName(modulename.clone()).map_err(|err|Errors::ModuleConfigError(modulename.to_string(),err))?;
		
		let module = &HTracer::singleton()._modules;
		if(!module.contains_key(modulename.as_str()))
		{
			module.insert(modulename.clone(), HashMap::new());
		}
		module.get_mut(modulename.as_str()).unwrap().insert(threadId,Box::new(newmodule));
		return Ok(());
	}
	
	pub fn getThread() -> u64
	{
		let mut hasher = DefaultHasher::new();
		thread::current().id().hash(&mut hasher);
		return hasher.finish();
	}
	
	pub fn log<T>(rawEntry : &T, level: Type, file: &str, line: u32)
		where T: Any + Debug // + ?Display
	{
		Self::logWithBacktrace(rawEntry,level,file,line,vec![]);
	}
	
	pub fn logWithBacktrace<T>(rawEntry : &T, level: Type, file: &str, line: u32, backtraces: Vec<Hbacktrace>)
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
		
		/*let tmp: rawlog = tmp.into();
		let tmp: String = tmp.into();*/
		let file = file.to_string();
		let thisThreadId = HTracer::getThread();
		let time = Local::now();
		
		thread::spawn(move ||{
			Self::singleton()._deferredLog.write().push(OneLog {
				message: tmp,
				date: time,
				level,
				threadId: thisThreadId,
				filename: file,
				fileline: line,
				backtraces,
			});
			
			Self::singleton()._threadWriting.write().thread_launch_delayabe();
		});
		
	}
	
	/// need to be call before the app exit.
	/// sync all remaining thread and launch "OnExit" event of modules
	pub fn drop()
	{
		thread::sleep(Duration::from_millis(10));
		
		for thismodule in HTracer::singleton()._modules.iter()
		{
			if let Some(tmp) = thismodule.get(&0)
			{
				tmp.Event_onExit();
			}
		}
		
		Self::singleton().internal_writestuff();
	}
	
	/// set a name for a thread, this is just visual
	pub fn threadSetName(name: impl Into<String>)
	{
		HTracer::singleton()._threadNames.insert(HTracer::getThread(),name.into());
	}
	
	pub fn threadGetName(id: u64) -> String
	{
		return match HTracer::singleton()._threadNames.get(&id) {
			None => id.to_string(),
			Some(x) => x.value().clone()
		};
	}
	
	pub fn minlvl_default(minlvl: Type)
	{
		Self::singleton()._minlvl.swap(Arc::new(minlvl.tou8()));
		
		let mut tracerC = HConfigManager::singleton().get("htrace");
		if tracerC.get("minlvl").is_none()
		{
			tracerC.set("minlvl",minlvl.tou8());
			let _ = tracerC.save();
		}
		
	}
	
	pub fn backtrace() -> Vec<Hbacktrace>
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
			
			if(name.eq("Htrace::HTracer::HTracer::backtrace()"))
			{
				internal = false;
			}
			else if !internal && solvable
			{
				returning.push(Hbacktrace{
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
	
	/*fn getBacktraceInfos(resolvedSymbol: &mut String, resolvedFile: &mut String, resolvedFileLine: &mut u32)
	{
		backtrace::trace(|thisframe|
			{
				let mut nextframe = true;
				backtrace::resolve_frame(thisframe,|symbol|
					{
						
						println!("symbol : {:?}",symbol);
						if(symbol.name().is_none() || symbol.filename().is_none())
						{
							return ();
						}
						let tmp = symbol.name().unwrap().to_string();
						
						if(tmp.starts_with("Htrace::") || tmp.starts_with("backtrace::"))
						{
							return ();
						}
						
						*resolvedSymbol = tmp;
						if(symbol.filename().is_some())
						{
							*resolvedFile = symbol.filename().unwrap().to_str().unwrap_or_else(|| { "" }).to_string();
						}
						*resolvedFileLine = symbol.lineno().unwrap_or_else(|| { 0 });
						nextframe = false;
					});
				
				nextframe
			});
	}*/
	
	//// PRIVATE ////
	fn internal_log(&self, log : OneLog)
	{
		for thismodule in self._modules.iter()
		{
			if let Some(tmp) = thismodule.get(&0)
			{
				Type::launchModuleFunc(tmp,&log);
			}
			
			if let Some(tmp) = thismodule.get(&log.threadId)
			{
				Type::launchModuleFunc(tmp,&log);
			}
		}
	}
	
	fn internal_writestuff(&self)
	{
		let mut binding = Self::singleton()._deferredLog.write();
		let getWritingStuff = binding.drain(0..).collect::<Vec<_>>();
		
		for x in getWritingStuff {
			HTracer::singleton().internal_log(x);
		}
	}
}

/* ---- for specialization / chalk ?

struct rawlog
{
	data: String
}

/*impl<T> From<T> for rawlog
	where T: Display
{
	fn from(raw: T) -> Self {
		rawlog{
			data: format!("{}",raw)
		}
	}
}*/

impl<T: Debug> From<T> for rawlog
	where T: Debug
{
	fn from(raw: T) -> Self {
		rawlog{
			data: format!("{:?}",raw)
		}
	}
}

impl From<rawlog> for String
{
	fn from(raw: rawlog) -> Self {
		raw.data
	}
}
*/
