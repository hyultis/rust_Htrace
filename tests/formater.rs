#![allow(unused_parens)]

use std::fs;
use time::macros::datetime;
use Htrace::components::context::Context;
use Htrace::components::formater::{FormaterCompile, FormaterParamBuilder};
use Htrace::components::level::Level;
use Htrace::components::trace::OneTrace;
use Htrace::crates::bridge::HtraceBridge;
use Htrace::htracer::HTracer;

#[test]
fn formater()
{
	let _ = fs::remove_dir_all("./traces");
	#[cfg(all(not(feature = "tracing_consumer"),not(feature = "log_consumer")))]
	HTracer::globalContext_set(Context::default());
	#[cfg(any(feature = "tracing_consumer",feature = "log_consumer"))]
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
	HTracer::drop();
}