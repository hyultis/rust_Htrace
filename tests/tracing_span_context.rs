#![cfg(feature = "tracing_consumer")]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::Spaned;
use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use tracing::level_filters::LevelFilter;

// Single test: the global configuration is a process-wide singleton.
#[test]
fn tracing_span_path_is_the_context_name_without_context_exit()
{
	let capture = Capture::new();
	let mut global = Context::default();
	global.level_setMin(Some(Level::DEBUG));
	global.module_add("capture", capture.clone());
	let bridge = HtraceBridge {
		min_level_backtrace: None,
		tracing_min_level: Level::NOTICE,
		..HtraceBridge::default()
	};
	HTracer::globalContext_set(global, bridge).unwrap();
	// No level hint: spans of any level stay enabled; only event callsites
	// below the threshold are disabled (tracing INFO maps to NOTICE).
	assert_eq!(LevelFilter::current(), LevelFilter::TRACE);

	tracing::info!("outside");
	let root = tracing::info_span!("root");
	root.in_scope(|| {
		tracing::info!("in root");
		tracing::debug!("below threshold");
		tracing::info_span!("child").in_scope(|| {
			tracing::warn!(count = 3, "in child");
			tracing::info!(parent: &root, "explicit root");
			tracing::info!(parent: None, "explicit none");
		});
	});
	HTracer::flush().unwrap();
	assert_eq!(capture.contextExits(), 0);

	let contextOf = |message: &str| {
		capture
			.find(|trace| trace.message == message)
			.unwrap_or_else(|| panic!("missing {message}"))
			.contextName
	};
	assert_eq!(contextOf("outside"), None);
	assert_eq!(contextOf("in root"), Some("root".to_string()));
	assert_eq!(
		contextOf("in child {count=3}"),
		Some("root::child".to_string())
	);
	assert_eq!(contextOf("explicit root"), Some("root".to_string()));
	assert_eq!(contextOf("explicit none"), None);
	assert!(
		capture
			.find(|trace| trace.message == "below threshold")
			.is_none()
	);
	assert_eq!(capture.count(), 5);

	// The rest of the resolved Htrace context is kept.
	{
		let mut local = Context::default();
		local.threadName_set("worker");
		local.extra_set("request", "42");
		Spaned!(local);
		tracing::info_span!("handler").in_scope(|| tracing::info!("in htrace span"));
		HTracer::flush().unwrap();
		assert_eq!(capture.contextExits(), 0);
	}
	HTracer::flush().unwrap();
	// Only the Htrace span exit is notified.
	assert_eq!(capture.contextExits(), 1);
	let inside = capture
		.find(|trace| trace.message == "in htrace span")
		.unwrap();
	assert_eq!(inside.contextName, Some("handler".to_string()));
	assert_eq!(inside.threadName, Some("worker".to_string()));
	assert_eq!(inside.extras.get("request"), Some(&"42".to_string()));
	HTracer::shutdown().unwrap();
}
