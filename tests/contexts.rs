#![allow(non_snake_case)]

use htrace::components::{context::Context, level::Level, span::Span, trace::OneTrace};
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::module_abstract::ModuleAbstract;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct Record
{
	kind: &'static str,
	message: String,
	context: Context,
}

#[derive(Clone, Default)]
struct Recorder
{
	records: Arc<Mutex<Vec<Record>>>,
}

impl Recorder
{
	fn trace(&self, trace: &OneTrace)
	{
		self.records.lock().unwrap().push(Record {
			kind: "trace",
			message: trace.message.clone(),
			context: trace.context.clone(),
		});
	}

	fn snapshot(&self) -> Vec<Record>
	{
		self.records.lock().unwrap().clone()
	}
}

macro_rules! record_levels
{
	($($method:ident),*) => {
		$(fn $method(&self, trace: &OneTrace) { self.trace(trace); })*
	};
}

impl ModuleAbstract for Recorder
{
	fn name_set(&mut self, _: String) -> anyhow::Result<()>
	{
		Ok(())
	}
	fn name_get(&self) -> anyhow::Result<String>
	{
		Ok("recorder".into())
	}
	record_levels!(
		event_onDebug,
		event_onDebugErr,
		event_onNormal,
		event_onNotice,
		event_onNoticeErr,
		event_onWarning,
		event_onError,
		event_onFatal
	);

	fn event_onContextExit(&self, context: &Context)
	{
		self.records.lock().unwrap().push(Record {
			kind: "exit",
			message: String::new(),
			context: context.clone(),
		});
	}

	fn event_onGlobalExit(&self) {}
}

fn install(context: Context)
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default());
}

fn trace(message: &'static str)
{
	HTracer::trace(&message, Level::NORMAL, file!(), line!(), Vec::new());
}

fn exit_names(recorder: &Recorder) -> Vec<String>
{
	recorder
		.snapshot()
		.iter()
		.filter(|record| record.kind == "exit")
		.map(|record| record.context.name_get().clone().unwrap_or_default())
		.collect()
}

#[test]
fn contexts_are_snapshots_and_spans_own_logical_exits()
{
	let unused = Recorder::default();
	let mut neverInstalled = Context::default();
	neverInstalled.module_add("unused", unused.clone());
	drop(neverInstalled.clone());
	drop(neverInstalled);
	// A guard may also be created and destroyed safely before initialization.
	drop(Span::new("pre-init".into()));
	assert!(unused.snapshot().is_empty());

	let primary = Recorder::default();
	let masked = Recorder::default();
	let local = Recorder::default();
	let mut global: Context = "global".into();
	global.module_add("primary", primary.clone());
	global.module_add("masked", masked.clone());
	global.extra_set("shared", "global");
	global.level_setMin(Some(Level::WARNING));
	let retainedGlobal = global.clone();
	install(global);

	// None inherits WARNING, while an explicit child level enables NORMAL.
	let inherited = Span::new(Context::default());
	trace("filtered");
	drop(inherited);
	HTracer::flush().unwrap();
	assert!(
		!primary
			.snapshot()
			.iter()
			.any(|record| record.kind == "trace")
	);
	assert_eq!(exit_names(&primary), vec!["global"]);
	primary.records.lock().unwrap().clear();
	masked.records.lock().unwrap().clear();

	let mut parentContext: Context = "parent".into();
	parentContext.level_setMin(Some(Level::NORMAL));
	parentContext.extra_set("parent", "present");
	let parent = Span::new(parentContext);
	let mut childContext: Context = "child".into();
	childContext.level_setMin(Some(Level::NORMAL));
	childContext.module_discard("masked");
	childContext.module_add("local", local.clone());
	childContext.extra_set("shared", "child");
	let clone = childContext.clone();
	let child = Span::new(childContext);
	drop(clone);
	trace("inside-child");
	trace("another-snapshot");
	HTracer::flush().unwrap();
	assert!(exit_names(&primary).is_empty());
	assert!(exit_names(&local).is_empty());

	// Removing an outer guard leaves its child alive and removes inherited data.
	drop(parent);
	trace("child-without-parent");
	let mut nextContext: Context = "next".into();
	nextContext.level_setMin(Some(Level::NORMAL));
	let next = Span::new(nextContext);
	drop(child);
	trace("next-without-child");
	drop(next);
	HTracer::flush().unwrap();
	assert_eq!(exit_names(&primary), vec!["parent", "child", "next"]);
	assert_eq!(exit_names(&masked), vec!["parent", "next"]);
	assert_eq!(exit_names(&local), vec!["child"]);
	let records = primary.snapshot();
	let inside = records
		.iter()
		.find(|record| record.message == "inside-child")
		.unwrap();
	assert_eq!(
		inside.context.extra_get("shared").map(String::as_str),
		Some("child")
	);
	assert_eq!(
		inside.context.extra_get("parent").map(String::as_str),
		Some("present")
	);
	assert!(
		inside
			.context
			.modules_get()
			.get("masked")
			.unwrap()
			.is_none()
	);
	let after = records
		.iter()
		.find(|record| record.message == "child-without-parent")
		.unwrap();
	assert!(after.context.extra_get("parent").is_none());
	let after = records
		.iter()
		.find(|record| record.message == "next-without-child")
		.unwrap();
	assert_eq!(
		after.context.extra_get("shared").map(String::as_str),
		Some("global")
	);
	assert!(after.context.modules_get().get("local").is_none());
	let parentExit = records.iter().find(|record| record.kind == "exit").unwrap();
	assert_eq!(parentExit.context.name_get().as_deref(), Some("parent"));
	assert!(parentExit.context.modules_get().get("local").is_none());

	let unwinding = std::panic::catch_unwind(|| {
		let _span = Span::new("unwind".into());
		panic!("exercise span unwinding");
	});
	assert!(unwinding.is_err());
	let moved = Span::new("moved".into());
	std::thread::spawn(move || drop(moved)).join().unwrap();
	HTracer::flush().unwrap();
	assert_eq!(
		exit_names(&primary),
		vec!["parent", "child", "next", "unwind", "moved"]
	);

	// Both anonymous and identically named producers keep independent stacks.
	for threadName in [None, Some("same-producer")]
	{
		let barrier = Arc::new(std::sync::Barrier::new(2));
		let mut handles = Vec::new();
		for index in 0..2
		{
			let barrier = barrier.clone();
			let builder = match threadName
			{
				Some(name) => std::thread::Builder::new().name(name.to_owned()),
				None => std::thread::Builder::new(),
			};
			handles.push(
				builder
					.spawn(move || {
						let current = std::thread::current();
						let producerName = current
							.name()
							.map(str::to_owned)
							.unwrap_or_else(|| format!("{:?}", current.id()));
						let message = format!("producer-{:?}-{index}", current.id());
						let mut context: Context = message.clone().into();
						context.level_setMin(Some(Level::NORMAL));
						let _span = Span::new(context);
						barrier.wait();
						HTracer::trace(&message, Level::NORMAL, file!(), line!(), Vec::new());
						barrier.wait();
						(message, producerName)
					})
					.unwrap(),
			);
		}
		let producers: Vec<_> = handles
			.into_iter()
			.map(|handle| handle.join().unwrap())
			.collect();
		HTracer::flush().unwrap();
		let records = primary.snapshot();
		for (message, producerName) in producers
		{
			let record = records
				.iter()
				.find(|record| record.message == message)
				.unwrap();
			assert_eq!(record.context.name_get().as_deref(), Some(message.as_str()));
			assert_eq!(
				record.context.threadName_get().as_deref(),
				Some(producerName.as_str())
			);
			assert_eq!(
				exit_names(&primary)
					.iter()
					.filter(|name| *name == &message)
					.count(),
				1
			);
		}
	}

	// Old snapshots and replacement callbacks keep the previous configuration.
	HTracer::trace(
		&"before-replace",
		Level::ERROR,
		file!(),
		line!(),
		Vec::new(),
	);
	let replacement = Recorder::default();
	let mut global: Context = "replacement".into();
	global.module_add("replacement", replacement.clone());
	install(global);
	trace("after-replace");
	drop(retainedGlobal);
	HTracer::flush().unwrap();
	let records = primary.snapshot();
	let beforePosition = records
		.iter()
		.position(|record| record.message == "before-replace")
		.unwrap();
	let exitPosition = records
		.iter()
		.position(|record| {
			record.kind == "exit" && record.context.name_get().as_deref() == Some("global")
		})
		.unwrap();
	assert!(beforePosition < exitPosition);
	assert_eq!(
		exit_names(&primary)
			.iter()
			.filter(|name| name.as_str() == "global")
			.count(),
		1
	);
	assert_eq!(
		replacement
			.snapshot()
			.iter()
			.filter(|record| record.kind == "trace")
			.count(),
		1
	);
	assert!(exit_names(&replacement).is_empty());
	HTracer::shutdown().unwrap();
}
