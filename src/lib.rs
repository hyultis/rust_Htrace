#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(unused_parens)]

extern crate core;
extern crate alloc;

pub mod htracer;
pub mod modules;
pub mod components;
mod thread_manager;
mod context_manager;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors
{
	#[error("'module/{0}' not found in config")]
	CannotFoundConfigNode(String),
	#[error("module '{0}' configuration returned a error : {1}")]
	ModuleConfigError(String,#[source] anyhow::Error),
	#[error("Error with HConfig {0} on : {1}")]
	HConfigError(String,#[source] Hconfig::Errors)
}

/// Shortcut for the trace function (defaults to Type::NORMAL)
/// It can be used like this:
/// ```
/// use Htrace::HTrace;
/// use Htrace::components::level::Level;
///
/// let myvar = 42;
/// HTrace!(myvar);
/// HTrace!("this is : {}",myvar);
/// HTrace!((Level::DEBUG) myvar);
/// HTrace!((Level::DEBUG) "this is : {}",myvar);
///
/// ```
///
/// Note: Currently, the data needs to be a string, a &str, or something that implements "Display" or "Debug".
#[macro_export]
macro_rules! HTrace
{
    ($a:expr) => {
	    $crate::htracer::HTracer::trace(&$a, $crate::components::level::Level::NORMAL, file!(), line!(), vec![]);
    };
	(($b:expr) $a:expr) => {
		if($b.tou8() >= $crate::components::level::Level::ERROR.tou8())
		{
			println!("here");
	        $crate::htracer::HTracer::trace(&$a, $b, file!(), line!(),$crate::htracer::HTracer::backtrace());
		}
		else
		{
			println!("prout");
	        $crate::htracer::HTracer::trace(&$a, $b, file!(), line!(), vec![]);
		}
    };
	($a:expr $(,$arg:expr)*) => {
	    $crate::htracer::HTracer::trace(&format!($a,$($arg),*), $crate::components::level::Level::NORMAL, file!(), line!(), vec![])
    };
	(($b:expr) $a:expr $(,$arg:expr)*) => {
		if($b.tou8() >= $crate::components::level::Level::ERROR.tou8())
		{
	        $crate::htracer::HTracer::trace(&format!($a,$($arg),*), $b, file!(), line!(),$crate::htracer::HTracer::backtrace())
		}
		else
		{
	        $crate::htracer::HTracer::trace(&format!($a,$($arg),*), $b, file!(), line!(), vec![])
		}
    };
}

/// Shortcut for the trace function for Result>Error (defaults to Type::ERROR)
/// Takes a result, and if it is in error, traces it.
/// Does nothing if the result is OK.
/// This only makes sense if you want to receive the error information for debugging.
/// Works like HTrace!()
/// Can be used like this:
/// ```
/// use Htrace::HTraceError;
///
/// let testerror = std::fs::File::open(std::path::Path::new("idontexist.muahahah"));
/// HTraceError!("file opening error : {}",testerror);
///
/// ```
#[macro_export]
macro_rules! HTraceError
{
	($a:expr) => {
		if let Err(errorToTrace) = $a {
			$crate::HTrace!(($crate::components::level::Level::ERROR) errorToTrace);
		}
    };
	($desc:expr,$a:expr) => {
		if let Err(errorToTrace) = $a {
			$crate::HTrace!(($crate::components::level::Level::ERROR) $desc,errorToTrace);
		}
    };
	(($b:expr) $a:expr) => {
		if let Err(errorToTrace) = $a {
			$crate::HTrace!(($b) errorToTrace);
		}
    };
	(($b:expr) $desc:expr,$a:expr) => {
		if let Err(errorToTrace) = $a {
			$crate::HTrace!(($b) $desc, errorToTrace);
		}
    };
}
