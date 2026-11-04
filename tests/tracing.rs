#![cfg(all(feature = "tracing_consumer", feature = "default_module"))]
#![allow(unused_parens)]

use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::command_line_config::CommandLineConfig;
use htrace::modules::file_config::FileConfig;
use htrace::modules::{command_line, file};
use std::fs;

#[test]
fn trace_from_tracing_crate()
{
	let tracePath = std::env::temp_dir().join(format!(
		"htrace-tracing-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	fs::create_dir(&tracePath).unwrap();
	let mut fileConfig = FileConfig::default();
	fileConfig.path = tracePath.to_string_lossy().into_owned();

	let mut global_context = Context::default();
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::default()),
	);
	global_context.module_add("file", file::File::new(fileConfig));
	global_context.level_setMin(Some(Level::DEBUG));

	HTracer::globalContext_set(global_context, HtraceBridge::default());

	tracing::trace!("test trace! from tracing crate"); // should not be displayed // trace in tracing have upper level than debug, so its converted to DEBUG in htrace
	tracing::debug!("test debug! from tracing crate"); // debug in tracing have upper level than normal, so its converted to NORMAL in htrace
	tracing::info!("test info! from tracing crate");
	tracing::warn!("test warn! from tracing crate");
	tracing::error!("test error! from tracing crate");

	// with span
	let method = "POST";
	let user_id: u64 = 42;
	let path = "/api/items";
	let request = tracing::info_span!("request", method = %method, user_id = user_id, path = %path);

	request.in_scope(|| {
		tracing::info!("span withing tracing crate: start handling"); // visible (inside 'request')

		let route = tracing::info_span!("route", name = "create_item");
		route.in_scope(|| {
			tracing::info!(count = 3, "span withing tracing crate: db query done"); // visible (inside 'request' -> 'route')
		});

		tracing::warn!("span withing tracing crate: handler a bit slow"); // visible (inside 'request')
	});

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	fs::remove_dir_all(&tracePath).unwrap();
}
