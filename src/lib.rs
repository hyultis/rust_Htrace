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

/// shortcut for the log fonction
/// can be use like that :
/// ```
/// use Htrace::HTrace;
/// use Htrace::Type;
///
/// let myvar = 42;
/// HTrace!(myvar);
/// HTrace!(myvar, Type::DEBUG);
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
	($a:expr $(,$arg:tt)*) => {
	    $crate::HTracer::HTracer::log(&format!($a,$($arg),*), $crate::Type::Type::NORMAL, file!(), line!())
    };
	(($b:expr) $a:expr $(,$arg:tt)*) => {
	    $crate::HTracer::HTracer::log(&format!($a,$($arg),*), $b, file!(), line!())
    };
	/*($a:expr,$b:expr) => {{
	    $crate::HTracer::HTracer::log(&$a, $b, file!(), line!())
    }};*/
}

/*
#[macro_export]
macro_rules! HTraceError
{
	($a:expr) => {}
}
*/
