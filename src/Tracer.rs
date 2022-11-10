use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fmt::{Display};
use std::string::ToString;
use std::sync::RwLock;
use once_cell::sync::OnceCell;
use crate::ModuleAbstract::ModuleAbstract;
use anyhow::{Result, anyhow, Context};
use Hconfig::Config::Config;
use Hconfig::Utils::Utils;
use crate::CommandLine::CommandLine;
use crate::File::File;
use backtrace::{Backtrace, BacktraceSymbol, resolve, resolve_frame, Symbol};
use chrono::Local;
use crate::OneLog::OneLog;
use crate::Type::Type;
use std::error::Error;
use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::ops::DerefMut;
use std::thread;
use Hconfig::Manager::Manager;

pub struct Tracer
{
	_config: RwLock<Option<String>>,
	_modules: RwLock<HashMap<String, HashMap<u64,Box<dyn ModuleAbstract>> >>
}

static SINGLETON: OnceCell<Tracer> = OnceCell::new();

impl Tracer
{
	fn new() -> Tracer {
		return Tracer {
			_config: RwLock::new(None),
			_modules: RwLock::new(HashMap::new())
		};
	}
	
	pub fn singleton() -> &'static Tracer
	{
		return SINGLETON.get_or_init(|| {
			Tracer::new()
		});
	}
	
	pub fn setConfig(configName : &str) -> Result<()>
	{
		let tracerC = Tracer::singleton();
		let mut tmpconfig = tracerC._config.write().unwrap();
		*tmpconfig = Some(configName.to_string());
		
		return Ok(());
	}
	
	// add a ModuleAbstract to tracing system
	pub fn appendModule(modulename: &str,newmodule : Box<dyn ModuleAbstract>) -> Result<()>
	{
		return Tracer::appendModuleToThread(modulename,newmodule,0);
	}
	
	// add a ModuleAbstract to tracing system but only for a specific thread (use Tracer::getThread to get a id)
	pub fn appendModuleToThread(modulename: &str,mut newmodule : Box<dyn ModuleAbstract>, threadId: u64) -> Result<()>
	{
		let modulename = modulename.to_string();
		let tracerC = Tracer::singleton();
		let tmp = tracerC._config
						 .read()
						 .or_else(|_truc| {Err(anyhow!("no config defined."))})?;
		let tmp = tmp.clone()
					 .context("no config defined.")?;
		let tmp = Manager::singleton().get(tmp.as_str())?;
		let moduleconfig = tmp.get("module/file").context("\"module/file\" not found in config")?;
		newmodule.setConfig(Utils::jsonIntoHashMap(moduleconfig))?;
		newmodule.setModuleName(modulename.clone())?;
		if(!tracerC._modules.write().unwrap().contains_key(modulename.as_str()))
		{
			tracerC._modules.write().unwrap().insert(modulename.clone(), HashMap::new());
		}
		tracerC._modules.write().unwrap().get_mut(modulename.as_str()).unwrap().insert(threadId,newmodule);
		return Ok(());
	}
	
	pub fn getThread() -> u64
	{
		let mut hasher = DefaultHasher::new();
		thread::current().id().hash(&mut hasher);
		return hasher.finish();
	}
	
	pub fn log<T>(tmp : T, level: Type) where T: Display
	{
		let tmp = format!("{}", tmp);
		let thisThreadId = Tracer::getThread();
		
		let mut resolvedSymbol = "".to_string();
		let mut resolvedFile = "".to_string();
		let mut resolvedFileLine: u32 = 0;
		Tracer::getBacktraceInfos(&mut resolvedSymbol,&mut resolvedFile,&mut resolvedFileLine);
		
		thread::spawn(move || {
			let tracerC = Tracer::singleton();
			if (tracerC._config.read().unwrap().is_none())
			{
				println!("Htrace : no config");
				return;
			}
			
			tracerC.internal_log(tmp, level, thisThreadId, resolvedSymbol, resolvedFile, resolvedFileLine);
		});
		
		//println!("{} {} (on {}:l{}) : {}",log.date.format("%H:%M:%S"),log.level,log.filename,log.fileline,log.message);
		//println!("--end--");
		//println!("{:?}",tmp);
		//let bt = Backtrace::new();
		//println!("{:?}", bt);
	}
	
	fn getBacktraceInfos(resolvedSymbol: &mut String, resolvedFile: &mut String, resolvedFileLine: &mut u32)
	{
		backtrace::trace(|thisframe|
			{
				let mut nextframe = false;
				backtrace::resolve_frame(thisframe,|symbol|
					{
						if(symbol.name().is_none() || symbol.filename().is_none())
						{
							return ();
						}
						let tmp = symbol.name().unwrap().to_string();
						
						if(tmp.starts_with("Htrace::") || tmp.starts_with("backtrace::"))
						{
							nextframe = true;
							return ();
						}
						
						*resolvedSymbol = tmp;
						if(symbol.filename().is_some())
						{
							*resolvedFile = symbol.filename().unwrap().to_str().unwrap_or_else(|| { "" }).to_string();
						}
						*resolvedFileLine = symbol.lineno().unwrap_or_else(|| { 0 });
					});
				
				nextframe
			});
	}
	
	//// PRIVATE ////
	fn internal_log(&self,tmp : String, level: Type, thisThreadId:u64, _resolvedSymbol: String, resolvedFile: String, resolvedFileLine: u32)
	{
		let log = OneLog {
			message: tmp,
			date: Local::now(),
			level,
			filename: resolvedFile,
			fileline: resolvedFileLine
		};
		
		let globalThreadid: u64 = 0;
		for thismodule in self._modules.read().unwrap().iter()
		{
			if(thismodule.1.contains_key(&globalThreadid))
			{
				Type::launchModuleFunc(thismodule.1.get(&globalThreadid).unwrap(),log.clone());
			}
			
			if(thismodule.1.contains_key(&thisThreadId))
			{
				Type::launchModuleFunc(thismodule.1.get(&thisThreadId).unwrap(),log.clone());
			}
		}
		
		//Type::launchModuleFunc(&self._module_file,log);
	}
	
	
}
