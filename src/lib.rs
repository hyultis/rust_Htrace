#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_parens)]

extern crate core;

pub mod HTracer;
pub mod File;
pub mod CommandLine;
pub mod ModuleAbstract;
pub mod Type;
mod OneLog;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors
{
	#[error("'module/{0}' not found in config")]
	CannotFoundConfigNode(String),
	#[error("module '{0}' configuration returned a error : {1}")]
	ModuleConfigError(String,#[source] anyhow::Error)
}

/// shortcut for the log fonction (default to Type::NORMAL)
/// can be use like that :
/// ```
/// use Htrace::HTrace;
/// use Htrace::Type;
///
/// let myvar = 42;
/// HTrace!(myvar);
/// HTrace!("this is : {}",myvar);
/// HTrace!((Type::DEBUG) myvar);
/// HTrace!((Type::DEBUG) "this is : {}",myvar);
///
/// ```
///
/// Note : actually, the data need to be a string or have "Debug" trait (adding "Display" when [chalk](https://rust-lang.github.io/chalk/book/) is added in stable)
#[macro_export]
macro_rules! HTrace
{
    ($a:expr) => {
	    $crate::HTracer::HTracer::log(&$a, $crate::Type::Type::NORMAL, file!(), line!())
    };
	(($b:expr) $a:expr) => {
	    $crate::HTracer::HTracer::log(&$a, $b, file!(), line!())
    };
	($a:expr $(,$arg:expr)*) => {
	    $crate::HTracer::HTracer::log(&format!($a,$($arg),*), $crate::Type::Type::NORMAL, file!(), line!())
    };
	(($b:expr) $a:expr $(,$arg:expr)*) => {
	    $crate::HTracer::HTracer::log(&format!($a,$($arg),*), $b, file!(), line!())
    };
}

/// shortcut for the log function for Result>Error (default to Type::ERROR)
/// take a result, and if it in error, trace it.
/// do nothing if result is ok
/// this only make sense if you want to receive the error information for debugging
/// can be use like that :
/// ```
/// use Htrace::HTraceError;
/// use Htrace::Type;
///
/// let testerror = std::fs::File::open(std::path::Path::new("idontexist.muahahah"));
/// HTraceError!(testerror);
/// HTraceError!("this is : {}",testerror);
/// HTraceError!((Type::DEBUG) testerror);
/// HTraceError!((Type::DEBUG) "this is : {}",testerror);
///
/// ```
#[macro_export]
macro_rules! HTraceError
{
	($a:expr) => {
		match $a {
			Ok(_) => {}
			Err(ref errorToTrace) => {
	    		$crate::HTracer::HTracer::log(&errorToTrace.to_string(), $crate::Type::Type::ERROR, file!(), line!())
			}
		}
    };
	($desc:expr,$a:expr) => {
		match $a {
			Ok(_) => {}
			Err(ref errorToTrace) => {
	    		$crate::HTracer::HTracer::log(&format!($desc,errorToTrace.to_string()), $crate::Type::Type::ERROR, file!(), line!())
			}
		}
    };
	(($b:expr) $a:expr) => {
		match $a {
			Ok(_) => {}
			Err(ref errorToTrace) => {
	    		$crate::HTracer::HTracer::log(&errorToTrace.to_string(), $b, file!(), line!())
			}
		}
    };
	(($b:expr) $desc:expr,$a:expr) => {
		match $a {
			Ok(_) => {}
			Err(ref errorToTrace) => {
	    		$crate::HTracer::HTracer::log(&format!($desc,errorToTrace.to_string()), $b, file!(), line!())
			}
		}
    };
}

/// spawn a thread with a specific name "{filename}_{line}" by default, or a string on the first argument
/// automatically set threadSetName inside
#[macro_export]
macro_rules! TSpawner
{
	($a:expr) => {
		{
			let filename = file!();
			let filename = filename.split('/').last().unwrap_or(filename);
			let name = format!("{}/{}",filename,line!());
		    std::thread::Builder::new().name(name.clone()).spawn(move ||{
				$crate::HTracer::HTracer::threadSetName(name);
				$a()
			})
		}
    };
	($b:expr,$a:expr) => {
		{
			let name = $b.to_string();
		    std::thread::Builder::new().name(name.clone()).spawn(move ||{
				$crate::HTracer::HTracer::threadSetName(name);
				$a()
			})
		}
    };
}
