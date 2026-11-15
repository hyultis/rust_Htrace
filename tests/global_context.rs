#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::components::level::Level;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::{Errors, HTrace};

#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
fn bridge() -> HtraceBridge
{
	HtraceBridge {
		min_level_backtrace: None,
		#[cfg(feature = "log_consumer")]
		log_min_level: Level::NOTICEDERR,
		..HtraceBridge::default()
	}
}

fn install(context: Context) -> Result<(), Errors>
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	return HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	return HTracer::globalContext_set(context, bridge());
}

fn captured(name: &str) -> (Context, Capture)
{
	let capture = Capture::new();
	let mut context: Context = name.into();
	context.module_add("capture", capture.clone());
	(context, capture)
}

// Single test: the global configuration and bridges are process-wide.
#[test]
fn global_context_reports_worker_and_bridge_errors()
{
	let (context, first) = captured("first");
	install(context).unwrap();

	// The log filter matches the bridge threshold, set only once installed.
	#[cfg(feature = "log_consumer")]
	{
		assert_eq!(log::max_level(), log::LevelFilter::Warn);
		log::info!("log below bridge threshold");
		log::warn!("log at bridge threshold");
	}
	HTrace!("first message");
	HTracer::flush().unwrap();
	assert!(
		first
			.find(|trace| trace.message == "first message")
			.is_some()
	);
	#[cfg(feature = "log_consumer")]
	{
		assert!(
			first
				.find(|trace| trace.message == "log at bridge threshold")
				.is_some()
		);
		assert!(
			first
				.find(|trace| trace.message == "log below bridge threshold")
				.is_none()
		);
	}

	// An equal bridge only replaces the context.
	let (context, second) = captured("second");
	install(context).unwrap();
	HTrace!("second message");
	HTracer::flush().unwrap();
	assert!(
		second
			.find(|trace| trace.message == "second message")
			.is_some()
	);

	// A different bridge is refused and nothing is applied.
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	{
		let (context, third) = captured("third");
		let other = HtraceBridge {
			min_level_backtrace: Some(Level::FATAL),
			..bridge()
		};
		let result = HTracer::globalContext_set(context, other);
		assert!(
			matches!(result, Err(Errors::BridgeAlreadyInstalled)),
			"{result:?}"
		);
		#[cfg(feature = "log_consumer")]
		assert_eq!(log::max_level(), log::LevelFilter::Warn);
		HTrace!("after refused bridge");
		HTracer::flush().unwrap();
		assert!(
			second
				.find(|trace| trace.message == "after refused bridge")
				.is_some()
		);
		assert_eq!(third.count(), 0);
	}

	// A completed shutdown is terminal: configuration reports it.
	HTracer::shutdown().unwrap();
	let (context, _) = captured("after-shutdown");
	let result = install(context);
	assert!(
		matches!(result, Err(Errors::WorkerUnavailable)),
		"{result:?}"
	);
}
