#![allow(non_snake_case)]

use htrace::components::{context::Context, level::Level, trace::OneTrace};
use htrace::htracer::{ControlError, HTracer};
use htrace::modules::module_abstract::ModuleAbstract;
use htrace::{HTrace, HTraceError, Spaned};
use std::cell::Cell;
use std::fmt::{self, Display};
use std::sync::{Arc, Mutex};

struct Capture(Arc<Mutex<Vec<String>>>);

macro_rules! capture_levels
{
	($($name:ident),*) => {
		$(fn $name(&self, trace: &OneTrace) { self.0.lock().unwrap().push(trace.message.clone()); })*
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
	capture_levels!(
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

struct DisplayOnly<'a>(&'a str);
impl Display for DisplayOnly<'_>
{
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
	{
		f.write_str(self.0)
	}
}

#[test]
fn lazy_macros_borrow_results_and_evaluate_arguments_once()
{
	let calls = Cell::new(0);
	HTrace!({
		calls.set(calls.get() + 1);
		"before initialization"
	});
	assert_eq!(calls.get(), 0);
	assert_eq!(HTracer::flush(), Err(ControlError::NotInitialized));
	let messages = Arc::new(Mutex::new(Vec::new()));
	let mut context = Context::default();
	context.module_add("capture", Capture(messages.clone()));
	context.level_setMin(Some(Level::FATAL));
	#[cfg(any(feature = "log_consumer", feature = "tracing_consumer"))]
	HTracer::globalContext_set(context, htrace::crates::bridge::HtraceBridge::default());
	#[cfg(not(any(feature = "log_consumer", feature = "tracing_consumer")))]
	HTracer::globalContext_set(context);

	let before = HTracer::stats().backtraces;
	let levels = Cell::new(0);
	HTrace!(({ levels.set(levels.get() + 1); Level::ERROR }) "{}", {
		calls.set(calls.get() + 1); "filtered"
	});
	assert_eq!(levels.get(), 1);
	assert_eq!(calls.get(), 0);
	assert_eq!(HTracer::stats().backtraces, before);
	let operations = Cell::new(0);
	HTraceError!({
		operations.set(operations.get() + 1);
		Err::<(), _>("filtered error")
	});
	HTraceError!({
		operations.set(operations.get() + 1);
		Err::<(), _>("filtered borrowed")
	});
	assert_eq!(operations.get(), 2);
	let filteredError: Result<String, String> = Err("filtered".into());
	HTraceError!(filteredError);
	HTraceError!("error = {}", filteredError);
	assert_eq!(filteredError.as_ref().unwrap_err(), "filtered");
	let okResult: Result<String, String> = Ok("still owned".into());
	HTraceError!(okResult);
	assert_eq!(okResult, Ok("still owned".to_owned()));
	assert_eq!(HTracer::stats().backtraces, before);

	{
		let mut context = Context::default();
		context.level_setMin(Some(Level::DEBUG));
		Spaned!(context);
		HTrace!(({ levels.set(levels.get() + 1); Level::NORMAL }) "{}", {
			calls.set(calls.get() + 1); "once"
		});
		assert_eq!(levels.get(), 2);
		assert_eq!(calls.get(), 1);
		let x = 7;
		HTrace!("val = {x}");
		HTrace!("val = {x}",);
		HTrace!("val = {}", x);
		HTrace!("val = {value}", value = x);
		HTrace!("{\"json\": true}");
		let owned = String::from("borrowed");
		let display = DisplayOnly(&owned);
		HTrace!("display = {}", display);
		HTracer::trace_display(&display, Level::NORMAL, file!(), line!(), vec![]);
		HTracer::trace_debug(&owned.as_str(), Level::NORMAL, file!(), line!(), vec![]);
		let result: Result<(), DisplayOnly<'_>> = Err(display);
		HTraceError!((Level::NORMAL) "error = {}", result);
		assert!(result.is_err());
		HTraceError!((Level::NORMAL) Err::<(), _>(&owned));
		let debugError: Result<(), String> = Err(owned.clone());
		let errorLevels = Cell::new(0);
		HTraceError!(({ errorLevels.set(errorLevels.get() + 1); Level::NORMAL }) debugError);
		assert_eq!(errorLevels.get(), 1);
		assert_eq!(debugError.as_ref().unwrap_err(), "borrowed");
		let okResult: Result<String, String> = Ok(owned.clone());
		HTraceError!((Level::NORMAL) okResult);
		assert_eq!(okResult, Ok("borrowed".to_owned()));
	}
	HTracer::flush().unwrap();
	assert_eq!(
		*messages.lock().unwrap(),
		[
			"once",
			"val = {x}",
			"val = 7",
			"val = 7",
			"val = 7",
			"{\"json\": true}",
			"display = borrowed",
			"borrowed",
			"\"borrowed\"",
			"error = borrowed",
			"\"borrowed\"",
			"\"borrowed\"",
		]
	);
	HTracer::shutdown().unwrap();
	HTrace!({
		calls.set(999);
		"after shutdown"
	});
	assert_eq!(calls.get(), 1);
	HTracer::trace(&"rejected", Level::NORMAL, file!(), line!(), vec![]);
	assert!(HTracer::stats().rejected >= 1);
	HTracer::shutdown().unwrap();
}
