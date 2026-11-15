#![cfg(feature = "tracing_consumer")]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::crates::bridge::{HtraceBridge, HtraceBridgeSpanFilterList};
use htrace::htracer::HTracer;
use tracing_subscriber::prelude::*;

fn bridge(list: HtraceBridgeSpanFilterList) -> HtraceBridge
{
	HtraceBridge {
		min_level_backtrace: None,
		tracing_min_level: Level::WARNING,
		tracing_filter_span: list,
		..HtraceBridge::default()
	}
}

// Single test: the global configuration is a process-wide singleton.
#[test]
fn spans_below_the_event_threshold_stay_visible()
{
	let capture = Capture::new();
	let mut global = Context::default();
	global.level_setMin(Some(Level::DEBUG));
	global.module_add("capture", capture.clone());
	HTracer::globalContext_set(global, bridge(HtraceBridgeSpanFilterList::AllowAll)).unwrap();

	tracing::info_span!("request").in_scope(|| {
		tracing::warn!("slow request");
		tracing::info!("info below threshold");
		tracing::debug!("debug below threshold");
	});
	HTracer::flush().unwrap();
	let slow = capture
		.find(|trace| trace.message == "slow request")
		.unwrap();
	assert_eq!(slow.contextName, Some("request".to_string()));
	assert_eq!(capture.count(), 1);

	// The span list still sees an INFO span with a WARNING threshold.
	capture.clear();
	let subscriber = tracing_subscriber::registry()
		.with(bridge(HtraceBridgeSpanFilterList::Deny(vec!["request".into()])).filtered());
	tracing::subscriber::with_default(subscriber, || {
		tracing::info_span!("request").in_scope(|| tracing::warn!("denied"));
		tracing::info_span!("other").in_scope(|| {
			tracing::warn!("allowed");
			tracing::debug!("debug below threshold");
		});
	});
	HTracer::flush().unwrap();
	let received = capture.snapshot();
	assert_eq!(received.len(), 1, "{received:?}");
	assert_eq!(received[0].message, "allowed");
	assert_eq!(received[0].contextName, Some("other".to_string()));
	assert_eq!(capture.contextExits(), 0);
	HTracer::shutdown().unwrap();
}
