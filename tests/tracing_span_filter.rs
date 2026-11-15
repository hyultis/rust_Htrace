#![cfg(feature = "tracing_consumer")]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::crates::bridge::{HtraceBridge, HtraceBridgeSpanFilterList};
use htrace::htracer::HTracer;
use tracing_subscriber::prelude::*;

const ALL: [&str; 10] = [
	"outside",
	"a",
	"a>b",
	"a>b>c",
	"b",
	"c",
	"c>a",
	"explicit a",
	"explicit a from c",
	"explicit root from c",
];

/// Emit events in nested spans through a local subscriber using `list`.
fn received(capture: &Capture, list: HtraceBridgeSpanFilterList) -> Vec<String>
{
	capture.clear();
	let bridge = HtraceBridge {
		min_level_backtrace: None,
		tracing_filter_span: list,
		..HtraceBridge::default()
	};
	let subscriber = tracing_subscriber::registry().with(bridge.filtered());
	tracing::subscriber::with_default(subscriber, || {
		tracing::info!("outside");
		tracing::info_span!("a").in_scope(|| {
			tracing::info!("a");
			tracing::info_span!("b").in_scope(|| {
				tracing::info!("a>b");
				tracing::info_span!("c").in_scope(|| tracing::info!("a>b>c"));
			});
		});
		tracing::info_span!("b").in_scope(|| tracing::info!("b"));
		tracing::info_span!("c").in_scope(|| {
			tracing::info!("c");
			tracing::info_span!("a").in_scope(|| tracing::info!("c>a"));
		});
		// An explicit parent replaces the current span.
		let explicit = tracing::info_span!("a");
		tracing::info!(parent: &explicit, "explicit a");
		tracing::info_span!("c").in_scope(|| {
			tracing::info!(parent: &explicit, "explicit a from c");
			tracing::info!(parent: None, "explicit root from c");
		});
	});
	HTracer::flush().unwrap();
	let mut messages = capture
		.snapshot()
		.into_iter()
		.map(|trace| trace.message)
		.collect::<Vec<_>>();
	messages.sort();
	messages
}

fn expected(messages: &[&str]) -> Vec<String>
{
	let mut messages = messages
		.iter()
		.map(|message| message.to_string())
		.collect::<Vec<_>>();
	messages.sort();
	messages
}

fn names(names: &[&str]) -> Vec<String>
{
	names.iter().map(|name| name.to_string()).collect()
}

// Single test: the global configuration is a process-wide singleton.
#[test]
fn span_filter_checks_every_ancestor()
{
	let capture = Capture::new();
	let mut global = Context::default();
	global.module_add("capture", capture.clone());
	HTracer::globalContext_set(global, HtraceBridge::default()).unwrap();

	assert_eq!(
		HtraceBridgeSpanFilterList::default(),
		HtraceBridgeSpanFilterList::AllowAll
	);
	for (list, passing) in [
		(HtraceBridgeSpanFilterList::AllowAll, ALL.to_vec()),
		(
			HtraceBridgeSpanFilterList::DenyAll,
			vec!["outside", "explicit root from c"],
		),
		(
			HtraceBridgeSpanFilterList::Allow(names(&["a"])),
			vec![
				"outside",
				"a",
				"a>b",
				"a>b>c",
				"c>a",
				"explicit a",
				"explicit a from c",
				"explicit root from c",
			],
		),
		(
			HtraceBridgeSpanFilterList::Allow(names(&["b"])),
			vec!["outside", "a>b", "a>b>c", "b", "explicit root from c"],
		),
		(
			HtraceBridgeSpanFilterList::Allow(Vec::new()),
			vec!["outside", "explicit root from c"],
		),
		(
			HtraceBridgeSpanFilterList::Deny(names(&["a"])),
			vec!["outside", "b", "c", "explicit root from c"],
		),
		(
			HtraceBridgeSpanFilterList::Deny(names(&["b"])),
			vec![
				"outside",
				"a",
				"c",
				"c>a",
				"explicit a",
				"explicit a from c",
				"explicit root from c",
			],
		),
		(HtraceBridgeSpanFilterList::Deny(Vec::new()), ALL.to_vec()),
	]
	{
		assert_eq!(
			received(&capture, list.clone()),
			expected(&passing),
			"{list:?}"
		);
	}
	HTracer::shutdown().unwrap();
}
