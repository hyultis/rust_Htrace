#![cfg(any(feature = "log_consumer", feature = "tracing_consumer"))]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::{Errors, HTrace};

#[cfg(feature = "log_consumer")]
struct ForeignLogger;

#[cfg(feature = "log_consumer")]
impl log::Log for ForeignLogger
{
	fn enabled(&self, _: &log::Metadata) -> bool
	{
		false
	}
	fn log(&self, _: &log::Record) {}
	fn flush(&self) {}
}

#[cfg(feature = "log_consumer")]
static FOREIGN: ForeignLogger = ForeignLogger;

fn assert_foreign(result: Result<(), Errors>)
{
	#[cfg(feature = "log_consumer")]
	assert!(
		matches!(result, Err(Errors::LoggerAlreadySet)),
		"{result:?}"
	);
	#[cfg(not(feature = "log_consumer"))]
	assert!(
		matches!(result, Err(Errors::TracingSubscriberAlreadySet)),
		"{result:?}"
	);
}

// Single test: the log logger and tracing subscriber are process-wide.
#[test]
fn foreign_global_consumers_are_reported()
{
	#[cfg(feature = "log_consumer")]
	{
		log::set_logger(&FOREIGN).unwrap();
		log::set_max_level(log::LevelFilter::Error);
	}
	#[cfg(feature = "tracing_consumer")]
	tracing::subscriber::set_global_default(tracing_subscriber::registry()).unwrap();

	let capture = Capture::new();
	let mut context = Context::default();
	context.module_add("capture", capture.clone());
	assert_foreign(HTracer::globalContext_set(context, HtraceBridge::default()));
	// The foreign log filter is left untouched.
	#[cfg(feature = "log_consumer")]
	assert_eq!(log::max_level(), log::LevelFilter::Error);

	// The context itself is applied.
	HTrace!("context applied");
	HTracer::flush().unwrap();
	assert!(
		capture
			.find(|trace| trace.message == "context applied")
			.is_some()
	);

	// Nothing was installed, so no bridge is fixed: the same error is reported.
	let other = HtraceBridge {
		min_level_backtrace: None,
		..HtraceBridge::default()
	};
	assert_foreign(HTracer::globalContext_set(Context::default(), other));
	HTracer::shutdown().unwrap();
}
