#![cfg(all(feature = "tracing_consumer", feature = "default_module"))]
#![allow(unused_parens)]
#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
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

	let capture = Capture::new();
	let mut global_context = Context::default();
	global_context.name_set("tracing-global");
	global_context.module_add(
		"cmd",
		command_line::CommandLine::new(CommandLineConfig::default()),
	);
	global_context.module_add("file", file::File::new(fileConfig).unwrap());
	global_context.module_add("capture", capture.clone());
	global_context.level_setMin(Some(Level::DEBUG));

	HTracer::globalContext_set(global_context, HtraceBridge::default()).unwrap();

	tracing::trace!("test trace! from tracing crate");
	tracing::debug!("test debug! from tracing crate");
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

	HTracer::flush().unwrap();

	// tracing -> Htrace level mapping, message, fields and span path.
	let traces = capture.snapshot();
	assert_eq!(
		traces
			.iter()
			.map(|trace| (
				trace.level,
				trace.message.as_str(),
				trace.contextName.as_deref().unwrap_or("")
			))
			.collect::<Vec<_>>(),
		vec![
			(
				Level::DEBUG,
				"test trace! from tracing crate",
				"tracing-global"
			),
			(
				Level::NORMAL,
				"test debug! from tracing crate",
				"tracing-global"
			),
			(
				Level::NOTICE,
				"test info! from tracing crate",
				"tracing-global"
			),
			(
				Level::WARNING,
				"test warn! from tracing crate",
				"tracing-global"
			),
			(
				Level::ERROR,
				"test error! from tracing crate",
				"tracing-global"
			),
			(
				Level::NOTICE,
				"span withing tracing crate: start handling",
				"request"
			),
			(
				Level::NOTICE,
				"span withing tracing crate: db query done {count=3}",
				"request::route"
			),
			(
				Level::WARNING,
				"span withing tracing crate: handler a bit slow",
				"request"
			),
		]
	);
	for trace in &traces
	{
		assert!(trace.filename.ends_with("tracing.rs"), "{}", trace.filename);
		assert!(trace.fileline > 0);
		// Bridges capture backtraces from ERROR by default.
		match trace.level
		{
			Level::ERROR =>
			{
				assert!(trace.backtraceCount > 0, "{trace:?}");
				assert!(
					trace
						.backtraces
						.iter()
						.any(|frame| frame.contains("trace_from_tracing_crate")),
					"{:#?}",
					trace.backtraces
				);
			}
			_ => assert_eq!(trace.backtraceCount, 0, "{trace:?}"),
		}
	}
	// Events inside tracing spans install no Htrace span: no context exit.
	assert_eq!(capture.contextExits(), 0);

	// we need to wait all threads are done
	HTracer::shutdown().unwrap();
	assert_eq!(capture.globalExits(), 1);
	fs::remove_dir_all(&tracePath).unwrap();
}
