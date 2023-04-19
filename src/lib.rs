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
mod tests;

use thiserror::Error;


#[derive(Error, Debug)]
pub enum Errors
{
	#[error("wierd error")]
	Default,
	#[error("'module/{0}' not found in config")]
	CannotFoundConfigNode(String),
	#[error("module '{0}' configuration returned a error : {1}")]
	ModuleConfigError(String,#[source] anyhow::Error)
}

#[macro_export]
macro_rules! HTrace
{
    ($a:expr) => {
         crate::HTracer::HTracer::log(&$a, Type::Type::NORMAL, file!(), line!())
    };
    ($a:expr,$b:expr) => {{
         crate::HTracer::HTracer::log(&$a, $b, file!(), line!())
    }};
}
