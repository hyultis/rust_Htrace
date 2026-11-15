#![cfg(panic = "unwind")]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
use htrace::components::trace::OneTrace;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::{ControlError, HTracer};
use htrace::modules::module_abstract::ModuleAbstract;
use htrace::{Errors, HTrace};

const PANIC_MESSAGE: &str = "injected module panic";
const PANIC_TRIGGER: &str = "panic in module";

fn install(context: Context) -> Result<(), Errors>
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	return HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	return HTracer::globalContext_set(context, HtraceBridge::default());
}

/// Module panicking in its trace callback for one specific message.
struct Failing;

impl Failing
{
	fn check(&self, trace: &OneTrace)
	{
		if trace.message == PANIC_TRIGGER
		{
			panic!("{PANIC_MESSAGE}");
		}
	}
}

impl ModuleAbstract for Failing
{
	fn name_set(&mut self, _: String) -> anyhow::Result<()>
	{
		Ok(())
	}

	fn name_get(&self) -> anyhow::Result<String>
	{
		Ok("failing".to_string())
	}

	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onError(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.check(trace);
	}
	fn event_onContextExit(&self, _: &Context) {}
	fn event_onGlobalExit(&self) {}
}

fn messages(capture: &Capture) -> Vec<String>
{
	capture
		.snapshot()
		.into_iter()
		.map(|trace| trace.message)
		.collect()
}

// Single test: the global configuration is a process-wide singleton.
#[test]
fn module_panic_is_isolated_and_reported_sticky()
{
	// Keep the expected panic out of the test output; others are still printed.
	let defaultHook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |info| {
		if info.payload_as_str() != Some(PANIC_MESSAGE)
		{
			defaultHook(info);
		}
	}));

	let capture = Capture::new();
	let mut context: Context = "module-failure".into();
	context.module_add("capture", capture.clone());
	context.module_add("failing", Failing);
	install(context).unwrap();

	HTrace!("before failure");
	assert_eq!(HTracer::flush(), Ok(()));
	assert_eq!(HTracer::stats().moduleErrors, 0);

	// The panic is isolated: the other module still receives the trace, and
	// the next barrier reports the failure.
	HTrace!("{}", PANIC_TRIGGER);
	assert_eq!(HTracer::flush(), Err(ControlError::ModuleFailed));
	assert_eq!(HTracer::stats().moduleErrors, 1);
	assert_eq!(messages(&capture), ["before failure", PANIC_TRIGGER]);

	// Sticky: later barriers keep reporting it, while traces are still delivered.
	HTrace!("after failure");
	assert_eq!(HTracer::flush(), Err(ControlError::ModuleFailed));
	assert_eq!(HTracer::flush(), Err(ControlError::ModuleFailed));
	assert_eq!(
		messages(&capture),
		["before failure", PANIC_TRIGGER, "after failure"]
	);
	assert_eq!(HTracer::stats().moduleErrors, 1);
	assert_eq!(HTracer::stats().workerErrors, 0);

	// Shutdown still drains and notifies modules, then reports the failure;
	// repeated shutdowns observe the same completion.
	assert_eq!(HTracer::shutdown(), Err(ControlError::ModuleFailed));
	assert_eq!(capture.globalExits(), 1);
	assert_eq!(HTracer::shutdown(), Err(ControlError::ModuleFailed));
	assert_eq!(capture.globalExits(), 1);
	assert_eq!(HTracer::stats().moduleErrors, 1);
	assert_eq!(HTracer::stats().processed, 3);
}
