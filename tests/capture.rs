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

// Single test: the global configuration is a process-wide singleton.
#[test]
fn capture_records_trace_snapshots()
{
	let capture = Capture::new();
	let mut global: Context = "capture-global".into();
	global.level_setMin(Some(Level::DEBUG));
	global.extra_set("origin", "capture-test");
	global.module_add("capture", capture.clone());
	install(global);

	HTrace!("normal message");
	HTrace!((Level::ERROR) "error message");
	HTracer::flush().unwrap();

	let traces = capture.snapshot();
	assert_eq!(capture.count(), 2);
	assert_eq!(
		traces
			.iter()
			.map(|captured| (captured.level, captured.message.as_str()))
			.collect::<Vec<_>>(),
		vec![
			(Level::NORMAL, "normal message"),
			(Level::ERROR, "error message")
		]
	);

	let normal = capture
		.find(|captured| captured.level == Level::NORMAL)
		.unwrap();
	assert_eq!(normal.backtraceCount, 0);
	assert!(normal.backtraces.is_empty());
	assert_eq!(normal.contextName.as_deref(), Some("capture-global"));
	assert!(normal.threadName.is_some());
	assert_eq!(
		normal.extras.get("origin").map(String::as_str),
		Some("capture-test")
	);
	assert!(
		normal.filename.ends_with("capture.rs"),
		"{}",
		normal.filename
	);
	assert!(normal.fileline > 0);

	let error = capture
		.find(|captured| captured.level == Level::ERROR)
		.unwrap();
	assert!(error.backtraceCount > 0);
	assert_eq!(error.backtraceCount, error.backtraces.len());
	// Walked on the producer, resolved on the worker: the first frame is still
	// the caller's code, and Htrace/rustc internals are filtered out.
	assert!(
		error.backtraces[0].contains("capture.rs(")
			&& error.backtraces[0].contains("capture_records_trace_snapshots"),
		"{:#?}",
		error.backtraces
	);
	assert!(
		error
			.backtraces
			.iter()
			.all(|frame| !frame.contains("HTracer::backtrace") && !frame.starts_with("/rustc/")),
		"{:#?}",
		error.backtraces
	);

	capture.clear();
	assert_eq!(capture.count(), 0);
	{
		let mut span = Context::default();
		span.name_set("capture-span");
		Spaned!(span);
		HTrace!("span message");
	}
	HTracer::flush().unwrap();
	let spanTrace = capture
		.find(|captured| captured.message == "span message")
		.unwrap();
	assert_eq!(spanTrace.contextName.as_deref(), Some("capture-span"));
	assert_eq!(capture.contextExits(), 1);

	assert_eq!(capture.globalExits(), 0);
	HTracer::shutdown().unwrap();
	assert_eq!(capture.globalExits(), 1);
}
