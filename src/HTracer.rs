use std::any::Any;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use once_cell::sync::OnceCell;
use crate::ModuleAbstract::ModuleAbstract;
use chrono::Local;
use crate::OneLog::OneLog;
use crate::Type::Type;
use std::hash::{Hash, Hasher};
use std::sync::RwLock;
use std::thread;
use std::thread::JoinHandle;
use dashmap::DashMap;
use Hconfig::HConfig::HConfig;
use Hconfig::HConfigManager::HConfigManager;
use HArcMut::HArcMut;
use json::JsonValue;
use crate::Errors;

pub struct HTracer
{
	_config: HArcMut<HConfig>,
	_modules: DashMap<String, HashMap<u64,Box<dyn ModuleAbstract + Sync + Send>> >,
	_thread: RwLock<Vec<JoinHandle<()>>>,
	_threadNames: DashMap<u64,String>
}

static SINGLETON: OnceCell<HTracer> = OnceCell::new();

impl HTracer
{
	fn new() -> HTracer {
		return HTracer {
			_config: HConfigManager::singleton().get_mut("htrace").unwrap(),
			_modules: DashMap::new(),
			_thread: RwLock::new(Vec::new()),
			_threadNames: DashMap::new(),
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
		let tracerC = HTracer::singleton();
		if tracerC._config.get().get("module/file").is_none()
		{
			tracerC._config.update(|conf|{
				conf.set("module/file",|node|{
					*node = JsonValue::new_object();
				});
				conf.save().unwrap();
			});
		}
		
		let mut tmp = Ok(());
		tracerC._config.update(|conf|{
			conf.set("module/file", |node|
				{
					tmp = newmodule.setConfig(node);
				});
			conf.save().unwrap();
		});
		tmp.map_err(|err|Errors::ModuleConfigError(modulename.to_string(),err))?;
		newmodule.setModuleName(modulename.clone()).map_err(|err|Errors::ModuleConfigError(modulename.to_string(),err))?;
		
		if(!tracerC._modules.contains_key(modulename.as_str()))
		{
			tracerC._modules.insert(modulename.clone(), HashMap::new());
		}
		tracerC._modules.get_mut(modulename.as_str()).unwrap().insert(threadId,Box::new(newmodule));
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
		let anyEntry = rawEntry as &dyn Any;
		let tmp = match anyEntry.downcast_ref::<String>() {
			None => {
				match anyEntry.downcast_ref::<&str>()
				{
					None => {
						match anyEntry.downcast_ref::<Box<dyn Display>>() {
							None => {
								format!("{:?}", rawEntry)
							}
							Some(content) => {
								
								format!("{}", content)
							}
						}
					}
					Some(content) => {
						content.to_string()
					}
				}
			}
			Some(content) => {
				content.to_string()
			}
		};
		
		/*let tmp: rawlog = tmp.into();
		let tmp: String = tmp.into();*/
		let file = file.to_string();
		let thisThreadId = HTracer::getThread();
		
		HTracer::singleton()._thread.write().unwrap().push(thread::spawn(move || {
			let tracerC = HTracer::singleton();
			tracerC.internal_log(tmp, level, thisThreadId, file, line);
		}));
		thread::spawn(|| {
			HTracer::threadPurge();
		});
	}
	
	/// need to be call before the app exit.
	/// sync all remaining thread and launch "OnExit" event of modules
	pub fn drop()
	{
		HTracer::singleton()._thread.write().unwrap().drain(..).for_each(|x|
			{
				if(!x.is_finished())
				{
					x.join().unwrap();
				}
			});
		
		for thismodule in HTracer::singleton()._modules.iter()
		{
			if let Some(tmp) = thismodule.get(&0)
			{
				tmp.Event_onExit();
			}
		}
	}
	
	pub fn threadSetName(name: &str)
	{
		HTracer::singleton()._threadNames.insert(HTracer::getThread(),name.to_string());
	}
	
	pub fn threadGetName(id: u64) -> String
	{
		return match HTracer::singleton()._threadNames.get(&id) {
			None => id.to_string(),
			Some(x) => x.value().to_string()
		};
	}
	
	//////////// PRIVATE ///////////
	
	fn getBacktraceInfos(resolvedSymbol: &mut String, resolvedFile: &mut String, resolvedFileLine: &mut u32)
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
	}
	
	//// PRIVATE ////
	fn internal_log(&self, tmp : String, level: Type, thisThreadId:u64, file: String, line: u32)
	{
		let log = OneLog {
			message: tmp,
			date: Local::now(),
			level,
			threadId: thisThreadId,
			filename: file,
			fileline: line
		};
		
		for thismodule in self._modules.iter()
		{
			if let Some(tmp) = thismodule.get(&0)
			{
				Type::launchModuleFunc(tmp,log.clone());
			}
			
			if let Some(tmp) = thismodule.get(&thisThreadId)
			{
				Type::launchModuleFunc(tmp,log.clone());
			}
		}
		
		//Type::launchModuleFunc(&self._module_file,log);
	}
	
	fn threadPurge()
	{
		HTracer::singleton()._thread.write().unwrap().retain_mut(|i|{
			!i.is_finished()
		});
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
