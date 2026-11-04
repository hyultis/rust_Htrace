#![allow(unused_parens)]

use htrace::components::context::Context;
use htrace::components::formater::{FormaterCompile, FormaterParamBuilder};
use htrace::components::level::Level;
use htrace::components::trace::OneTrace;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use time::macros::datetime;

#[test]
fn formater()
{
	#[cfg(all(not(feature = "tracing_consumer"), not(feature = "log_consumer")))]
	HTracer::globalContext_set(Context::default());
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(Context::default(), HtraceBridge::default());

	let compiled = FormaterCompile(
		&"{time} {lvl} ({thread:>, }{file}:l{line} |{extra}{extra[test]}|) : {msg}".to_string(),
	);

	let mut context = Context::default();
	context.extra_set("test", "cake");

	let parameters = FormaterParamBuilder(
		&OneTrace {
			message: "message line".to_string(),
			date: datetime!(1900-01-01 0:00 UTC),
			level: Level::DEBUG,
			context,
			filename: "file.rs".to_string(),
			fileline: 42,
			backtraces: vec![],
		},
		&" | ".to_string(),
	);
	assert_eq!(
		compiled.render(parameters),
		"00:00:00.000000 DBUG (MAIN, file.rs:l42 |{extra}cake|) : message line",
		"simpleFormater format changed"
	);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
}
