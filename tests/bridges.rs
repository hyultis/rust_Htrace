#![cfg(any(feature = "log_consumer", feature = "tracing_consumer"))]
#![allow(non_snake_case)]

use htrace::Spaned;
use htrace::components::{context::Context, level::Level, trace::OneTrace};
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::module_abstract::ModuleAbstract;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Capture(Arc<Mutex<Vec<String>>>);
macro_rules! events {
    ($($method:ident),*) => {
        $(fn $method(&self, trace: &OneTrace) { self.0.lock().unwrap().push(trace.message.clone()); })*
    };
}
impl ModuleAbstract for Capture
{
	fn name_set(&mut self, _: String) -> anyhow::Result<()>
	{
		Ok(())
	}
	fn name_get(&self) -> anyhow::Result<String>
	{
		Ok("capture".into())
	}
	events!(
		event_onDebug,
		event_onDebugErr,
		event_onNormal,
		event_onNotice,
		event_onNoticeErr,
		event_onWarning,
		event_onError,
		event_onFatal
	);
	fn event_onContextExit(&self, _: &Context) {}
	fn event_onGlobalExit(&self) {}
}
struct FormatCount<'a>(&'a AtomicUsize);
impl std::fmt::Debug for FormatCount<'_>
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
	{
		self.0.fetch_add(1, Ordering::Relaxed);
		f.write_str("counted")
	}
}

#[test]
fn bridge_filters_precede_formatting_and_follow_local_thresholds()
{
	let messages = Arc::new(Mutex::new(Vec::new()));
	let mut context = Context::default();
	context.module_add("capture", Capture(messages.clone()));
	context.level_setMin(Some(Level::NOTICE));
	let bridge = HtraceBridge {
		min_level_backtrace: None,
		..HtraceBridge::default()
	};
	HTracer::globalContext_set(context, bridge);
	let formats = AtomicUsize::new(0);
	#[cfg(feature = "log_consumer")]
	log::debug!("{:?}", FormatCount(&formats));
	#[cfg(feature = "tracing_consumer")]
	tracing::debug!(field = ?FormatCount(&formats), "disabled");
	assert_eq!(formats.load(Ordering::Relaxed), 0);
	{
		let mut local = Context::default();
		local.level_setMin(Some(Level::DEBUG));
		Spaned!(local);
		#[cfg(feature = "log_consumer")]
		log::debug!("log {:?}", FormatCount(&formats));
		#[cfg(feature = "tracing_consumer")]
		tracing::debug!(field = ?FormatCount(&formats), "tracing enabled");
	}
	let mut expected = 0;
	#[cfg(feature = "log_consumer")]
	{
		expected += 1;
	}
	#[cfg(feature = "tracing_consumer")]
	{
		expected += 1;
	}
	assert_eq!(formats.load(Ordering::Relaxed), expected);
	#[cfg(feature = "tracing_consumer")]
	{
		use htrace::crates::bridge::HtraceBridgeSpanFilterList;
		use tracing_subscriber::prelude::*;
		let bridge = HtraceBridge {
			min_level_backtrace: None,
			tracing_filter_span: HtraceBridgeSpanFilterList::Deny(vec!["blocked".into()]),
			..HtraceBridge::default()
		};
		let subscriber = tracing_subscriber::registry().with(bridge.filtered());
		tracing::subscriber::with_default(subscriber, || {
			tracing::info_span!("blocked").in_scope(|| tracing::info!("must not appear"));
			tracing::info_span!("allowed").in_scope(|| tracing::info!("allowed event"));
		});
	}
	HTracer::shutdown().unwrap();
	let messages = messages.lock().unwrap();
	#[cfg(feature = "log_consumer")]
	assert!(messages.iter().any(|message| message == "log counted"));
	#[cfg(feature = "tracing_consumer")]
	{
		assert!(
			messages
				.iter()
				.any(|message| message.contains("tracing enabled") && message.contains("counted"))
		);
		assert!(messages.iter().any(|message| message == "allowed event"));
		assert!(!messages.iter().any(|message| message == "must not appear"));
	}
}
