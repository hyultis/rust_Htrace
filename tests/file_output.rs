#![cfg(feature = "default_module")]
#![allow(non_snake_case)]

use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::components::trace::OneTrace;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::file::File;
use htrace::modules::file_config::FileConfig;
use htrace::modules::module_abstract::ModuleAbstract;
use htrace::{HTrace, Spaned};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use time::macros::{datetime, format_description};

fn config(path: &Path) -> FileConfig
{
	FileConfig {
		path: path.to_string_lossy().into_owned(),
		lineFormat: "{msg}".to_string(),
		byThreadId: false,
		forceInOneFile: Some("all".to_string()),
		..FileConfig::default()
	}
}

fn trace(message: &str, date: OffsetDateTime) -> OneTrace
{
	OneTrace {
		message: message.to_string(),
		date,
		level: Level::NORMAL,
		context: Arc::new(Context::default()),
		filename: "some/directory/source.rs".into(),
		fileline: 1,
		backtraces: Vec::new(),
	}
}

/// Current UTC day, as used in daily file names.
fn utcDay() -> String
{
	OffsetDateTime::now_utc()
		.format(format_description!("[year][month][day]"))
		.unwrap()
}

/// Sorted lines of every `<producer>_<day>.trc` file in `path`, grouped by
/// producer. Each file must be dated with one of `days`, so traces written
/// across midnight may be split between two files of the same producer.
fn filesByProducer(path: &Path, days: &[&str]) -> BTreeMap<String, Vec<String>>
{
	let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for entry in std::fs::read_dir(path).unwrap()
	{
		let entry = entry.unwrap();
		let name = entry.file_name().to_string_lossy().into_owned();
		let producer = days
			.iter()
			.find_map(|day| name.strip_suffix(&format!("_{day}.trc")))
			.unwrap_or_else(|| panic!("unexpected file name {name:?}, expected days {days:?}"));
		let contents = std::fs::read_to_string(entry.path()).unwrap();
		groups
			.entry(producer.to_string())
			.or_default()
			.extend(contents.lines().map(str::to_string));
	}
	for lines in groups.values_mut()
	{
		lines.sort();
	}
	groups
}

#[test]
fn append_across_processes()
{
	const DIRECTORY: &str = "HTRACE_TEST_APPEND_DIRECTORY";
	const MESSAGE: &str = "HTRACE_TEST_APPEND_MESSAGE";
	if let Some(directory) = std::env::var_os(DIRECTORY)
	{
		let output = File::new(config(Path::new(&directory))).unwrap();
		let message = std::env::var(MESSAGE).unwrap();
		output.event_onNormal(&trace(&message, datetime!(2026-01-01 00:00 UTC)));
		output.flush().unwrap();
		assert_eq!(output.stats().linesWritten, 1);
		assert_eq!(output.stats().writeErrors, 0);
		return;
	}

	let root = std::env::temp_dir().join(format!(
		"htrace-append-tests-{}-{}",
		std::process::id(),
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&root).unwrap();
	let path = root.join("all_20260101.trc");
	std::fs::write(&path, "existing content\n").unwrap();
	let mut expected = "existing content\n".to_string();
	for message in ["first execution", "second execution"]
	{
		let result = std::process::Command::new(std::env::current_exe().unwrap())
			.args(["--exact", "append_across_processes", "--nocapture"])
			.env(DIRECTORY, &root)
			.env(MESSAGE, message)
			.output()
			.unwrap();
		assert!(result.status.success(), "child process failed: {result:?}");
		expected.push_str(message);
		expected.push('\n');
		assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
	}
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_output()
{
	let root = std::env::temp_dir().join(format!(
		"htrace-file-tests-{}-{}",
		std::process::id(),
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&root).unwrap();

	// All destinations remain cumulative, and rotation follows trace.date even
	// when an older trace arrives after a newer one.
	let rotatedPath = root.join("rotation/nested");
	let mut rotatedConfig = config(&rotatedPath);
	rotatedConfig.bySrc = true;
	rotatedConfig.byThreadId = true;
	rotatedConfig.byHour = true;
	let rotated = File::new(rotatedConfig).unwrap();
	for (message, date) in [
		("first", datetime!(2026-01-01 23:59 UTC)),
		("second", datetime!(2026-01-02 00:01 UTC)),
		("late", datetime!(2026-01-01 23:58 UTC)),
	]
	{
		rotated.event_onNormal(&trace(message, date));
	}
	for prefix in ["all", "source", "MAIN"]
	{
		assert_eq!(
			std::fs::read_to_string(rotatedPath.join(format!("{prefix}_20260101_23.trc"))).unwrap(),
			"first\nlate\n"
		);
		assert_eq!(
			std::fs::read_to_string(rotatedPath.join(format!("{prefix}_20260102_00.trc"))).unwrap(),
			"second\n"
		);
	}
	assert_eq!(rotated.stats().linesWritten, 9);
	rotated.flush().unwrap();

	let dayPath = root.join("day");
	let daily = File::new(config(&dayPath)).unwrap();
	daily.event_onNormal(&trace("daily", datetime!(2026-01-02 12:34 UTC)));
	assert_eq!(
		std::fs::read_to_string(dayPath.join("all_20260102.trc")).unwrap(),
		"daily\n"
	);

	// Disabled outputs perform no filesystem operations, including mkdir.
	let disabledPath = root.join("disabled");
	let mut disabledConfig = config(&disabledPath);
	disabledConfig.forceInOneFile = None;
	let disabled = File::new(disabledConfig).unwrap();
	disabled.event_onNormal(&trace("disabled", datetime!(2026-01-01 00:00 UTC)));
	assert!(!disabledPath.exists());
	assert_eq!(disabled.stats(), Default::default());

	// Reproducible failures without chmod or privilege assumptions.
	let parentFile = root.join("parent-is-file");
	std::fs::write(&parentFile, "keep me").unwrap();
	let createFailure = File::new(config(&parentFile.join("nested"))).unwrap();
	createFailure.event_onNormal(&trace("fail", datetime!(2026-01-01 00:00 UTC)));
	assert_eq!(createFailure.stats().createErrors, 1);
	assert_eq!(createFailure.stats().openErrors, 0);
	assert_eq!(std::fs::read_to_string(&parentFile).unwrap(), "keep me");
	let openPath = root.join("open-failure");
	std::fs::create_dir_all(openPath.join("all_20260101.trc")).unwrap();
	let openFailure = File::new(config(&openPath)).unwrap();
	openFailure.event_onNormal(&trace("fail", datetime!(2026-01-01 00:00 UTC)));
	assert_eq!(openFailure.stats().openErrors, 1);
	assert_eq!(openFailure.stats().createErrors, 0);
	assert_eq!(openFailure.stats().linesWritten, 0);

	// Distinct dates force LRU eviction. Reopening an evicted file appends.
	let boundedPath = root.join("bounded");
	let bounded = File::new(config(&boundedPath)).unwrap();
	for day in 1..=24
	{
		let date = datetime!(2026-01-01 00:00 UTC) + time::Duration::days(day - 1);
		bounded.event_onNormal(&trace("bounded", date));
		assert!(bounded.stats().cachedHandles <= 16);
	}
	assert_eq!(bounded.stats().cachedHandles, 16);
	bounded.event_onNormal(&trace("again", datetime!(2026-01-01 00:00 UTC)));
	bounded.flush().unwrap();
	assert_eq!(
		std::fs::read_to_string(boundedPath.join("all_20260101.trc")).unwrap(),
		"bounded\nagain\n"
	);
	assert_eq!(bounded.stats().linesWritten, 25);

	// Direct calls are serialized per complete line, independently of the worker.
	let concurrentPath = root.join("concurrent");
	let concurrent = Arc::new(File::new(config(&concurrentPath)).unwrap());
	let barrier = Arc::new(Barrier::new(5));
	let mut workers = Vec::new();
	for producer in 0..4
	{
		let concurrent = concurrent.clone();
		let barrier = barrier.clone();
		workers.push(std::thread::spawn(move || {
			barrier.wait();
			for index in 0..100
			{
				concurrent.event_onNormal(&trace(
					&format!("{producer}:{index}"),
					datetime!(2026-01-01 00:00 UTC),
				));
			}
		}));
	}
	barrier.wait();
	for worker in workers
	{
		worker.join().unwrap();
	}
	concurrent.flush().unwrap();
	let contents = std::fs::read_to_string(concurrentPath.join("all_20260101.trc")).unwrap();
	let mut actual = contents.lines().map(str::to_owned).collect::<Vec<_>>();
	actual.sort();
	let mut expected = (0..4)
		.flat_map(|producer| (0..100).map(move |index| format!("{producer}:{index}")))
		.collect::<Vec<_>>();
	expected.sort();
	assert_eq!(actual, expected);
	assert_eq!(concurrent.stats().linesWritten, 400);

	// One singleton lifecycle in this binary. Producer names are captured before
	// enqueueing; the worker's name must never select the output file.
	let producerPath = root.join("producers");
	let mut producerConfig = config(&producerPath);
	producerConfig.forceInOneFile = None;
	producerConfig.byThreadId = true;
	let producerFiles = Arc::new(File::new(producerConfig).unwrap());
	let mut context = Context::default();
	context.module_add_arc("files", producerFiles.clone());
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context).unwrap();
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default()).unwrap();
	// Traces are dated when emitted: the day may change while they are written.
	let dayBefore = utcDay();
	// Producer names come from span contexts and are sanitized for filenames.
	for name in [
		"producer-one",
		"producer-two",
		"../escape\\name",
		"%2E%2E",
		"_2E_2E",
		"Async Compute Task Pool (0)",
		"Async%20Compute%20Task%20Pool%20%280%29",
		"Équipe 東京 (1)",
	]
	{
		std::thread::spawn(move || {
			let mut context = Context::default();
			context.threadName_set(name);
			Spaned!(context);
			HTrace!("producer payload");
		})
		.join()
		.unwrap();
	}
	// Native thread names are never used: named and unnamed producers share
	// MAIN, without exposing their numeric ThreadId.
	for name in [None, None, Some("Async Compute Task Pool (1)")]
	{
		let builder = match name
		{
			Some(name) => std::thread::Builder::new().name(name.to_string()),
			None => std::thread::Builder::new(),
		};
		builder
			.spawn(|| {
				HTrace!("native payload");
			})
			.unwrap()
			.join()
			.unwrap();
	}
	// Taskpool workers can group their files through an explicit span name.
	let barrier = Arc::new(Barrier::new(2));
	let mut groupedWorkers = Vec::new();
	for index in 1..=2
	{
		let barrier = barrier.clone();
		groupedWorkers.push(
			std::thread::Builder::new()
				.name(format!("Async Compute Task Pool ({index})"))
				.spawn(move || {
					let mut context = Context::default();
					context.threadName_set("taskpool-group");
					Spaned!(context);
					barrier.wait();
					HTrace!("grouped payload {}", index);
				})
				.unwrap(),
		);
	}
	for worker in groupedWorkers
	{
		worker.join().unwrap();
	}
	HTracer::flush_timeout(Duration::from_secs(5)).unwrap();
	let dayAfter = utcDay();
	assert_eq!(producerFiles.stats().linesWritten, 13);
	let groups = filesByProducer(&producerPath, &[&dayBefore, &dayAfter]);
	let mut expected = BTreeMap::new();
	for producer in [
		"producer-one",
		"producer-two",
		"___escape_name",
		"Async Compute Task Pool (0)",
		"Async_20Compute_20Task_20Pool_20_280_29",
		"Équipe 東京 (1)",
	]
	{
		expected.insert(producer.to_string(), vec!["producer payload".to_string()]);
	}
	expected.insert(
		"_2E_2E".to_string(),
		vec!["producer payload".to_string(); 2],
	);
	// Producers without a span name are MAIN, whatever their native name.
	expected.insert("MAIN".to_string(), vec!["native payload".to_string(); 3]);
	expected.insert(
		"taskpool-group".to_string(),
		vec![
			"grouped payload 1".to_string(),
			"grouped payload 2".to_string(),
		],
	);
	assert_eq!(groups, expected);
	HTracer::shutdown_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(producerFiles.stats().flushErrors, 0);

	// Close cached handles before deleting this test's own temporary tree.
	drop((
		rotated,
		daily,
		disabled,
		createFailure,
		openFailure,
		bounded,
		concurrent,
		producerFiles,
	));
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn shared_destination_is_written_once()
{
	let root = std::env::temp_dir().join(format!(
		"htrace-dedup-tests-{}-{}",
		std::process::id(),
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	// forceInOneFile, bySrc and byThreadId all resolve to MAIN_20260101.trc.
	let mut sharedConfig = config(&root);
	sharedConfig.forceInOneFile = Some("MAIN".to_string());
	sharedConfig.bySrc = true;
	sharedConfig.byThreadId = true;
	let shared = File::new(sharedConfig).unwrap();
	let mut duplicated = trace("dup", datetime!(2026-01-01 00:00 UTC));
	duplicated.filename = "some/directory/MAIN.rs".into();
	shared.event_onNormal(&duplicated);
	shared.flush().unwrap();
	assert_eq!(
		std::fs::read_to_string(root.join("MAIN_20260101.trc")).unwrap(),
		"dup\n"
	);
	assert_eq!(shared.stats().linesWritten, 1);
	assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
	drop(shared);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_path_is_rejected()
{
	for path in ["", "   ", "\t\n"]
	{
		let rejected = File::new(FileConfig {
			path: path.to_string(),
			..FileConfig::default()
		});
		assert!(
			matches!(rejected, Err(htrace::Errors::EmptyPath)),
			"{path:?} must be rejected"
		);
	}
	let valid = std::env::temp_dir().join(format!("htrace-valid-path-{}", std::process::id()));
	assert!(File::new(config(&valid)).is_ok());
	// Construction alone performs no filesystem operation.
	assert!(!valid.exists());
}
