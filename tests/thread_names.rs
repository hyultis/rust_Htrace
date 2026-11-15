#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::components::level::Level;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::{HTrace, Spaned};

fn install(context: Context)
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context).unwrap();
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default()).unwrap();
}

fn threadName(capture: &Capture, message: &str) -> Option<String>
{
	capture
		.find(|captured| captured.message == message)
		.unwrap_or_else(|| panic!("missing trace {message:?}"))
		.threadName
}

// Single test: the global configuration is a process-wide singleton.
#[test]
fn producers_are_main_unless_a_span_names_them()
{
	let capture = Capture::new();
	let mut global: Context = "thread-names".into();
	global.level_setMin(Some(Level::DEBUG));
	// The global context thread name is ignored.
	global.threadName_set("global-name-ignored");
	global.module_add("capture", capture.clone());
	install(global);

	// The native OS thread name is never used.
	HTrace!("installing thread");
	std::thread::Builder::new()
		.name("native-named".to_owned())
		.spawn(|| HTrace!("named thread"))
		.unwrap()
		.join()
		.unwrap();
	std::thread::Builder::new()
		.name("Async Compute Task Pool (0)".to_owned())
		.spawn(|| HTrace!("pool thread"))
		.unwrap()
		.join()
		.unwrap();
	std::thread::spawn(|| HTrace!("anonymous thread"))
		.join()
		.unwrap();

	// The innermost span name applies and is restored when the span exits.
	std::thread::Builder::new()
		.name("native-with-spans".to_owned())
		.spawn(|| {
			HTrace!("before span");
			{
				let mut outer = Context::default();
				outer.threadName_set("outer-group");
				Spaned!(outer);
				HTrace!("outer span");
				{
					Spaned!("unnamed-child");
					HTrace!("unnamed child span");
				}
				{
					let mut inner = Context::default();
					inner.threadName_set("inner-group");
					Spaned!(inner);
					HTrace!("inner span");
				}
				HTrace!("outer restored");
			}
			HTrace!("after span");
		})
		.unwrap()
		.join()
		.unwrap();
	{
		let mut context = Context::default();
		context.threadName_set("installing-group");
		Spaned!(context);
		HTrace!("installing span");
	}
	HTrace!("installing restored");
	HTracer::flush().unwrap();

	for (message, expected) in [
		("installing thread", "MAIN"),
		("named thread", "MAIN"),
		("pool thread", "MAIN"),
		("anonymous thread", "MAIN"),
		("before span", "MAIN"),
		("outer span", "outer-group"),
		("unnamed child span", "outer-group"),
		("inner span", "inner-group"),
		("outer restored", "outer-group"),
		("after span", "MAIN"),
		("installing span", "installing-group"),
		("installing restored", "MAIN"),
	]
	{
		assert_eq!(
			threadName(&capture, message).as_deref(),
			Some(expected),
			"{message}"
		);
	}
	HTracer::shutdown().unwrap();
}
