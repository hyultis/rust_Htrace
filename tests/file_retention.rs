#![cfg(feature = "default_module")]
#![allow(non_snake_case)]

use htrace::components::{context::Context, level::Level};
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::{file::File, file_config::FileConfig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn temporaryDirectory() -> PathBuf
{
	let path = std::env::temp_dir().join(format!(
		"htrace-retention-{}-{}",
		std::process::id(),
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&path).unwrap();
	path
}

fn output(path: &Path) -> Arc<File>
{
	Arc::new(File::new(FileConfig {
		path: path.to_string_lossy().into_owned(),
		lineFormat: "{msg}".to_string(),
		byThreadId: false,
		forceInOneFile: Some("all".to_string()),
		purgeOnStartup: true,
		..FileConfig::default()
	}))
}

fn install(context: Context)
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default());
}

#[test]
fn startup_without_traces_and_reconfiguration()
{
	let root = temporaryDirectory();
	std::fs::write(root.join("old arbitrary name.trc"), "old").unwrap();
	std::fs::write(root.join("keep.TRC"), "upper").unwrap();
	std::fs::write(root.join("keep.trc.bak"), "backup").unwrap();
	std::fs::create_dir(root.join("directory.trc")).unwrap();
	std::fs::write(root.join("directory.trc/keep.trc"), "nested").unwrap();
	let files = output(&root);
	let mut context = Context::default();
	context.module_add_arc("files", files.clone());
	// Aliasing the same module must not run startup twice.
	context.module_add_arc("alias", files.clone());
	install(context);
	HTracer::flush_timeout(Duration::from_secs(5)).unwrap();
	assert!(!root.join("old arbitrary name.trc").exists());
	assert_eq!(
		std::fs::read_to_string(root.join("keep.TRC")).unwrap(),
		"upper"
	);
	assert_eq!(
		std::fs::read_to_string(root.join("keep.trc.bak")).unwrap(),
		"backup"
	);
	assert_eq!(
		std::fs::read_to_string(root.join("directory.trc/keep.trc")).unwrap(),
		"nested"
	);
	assert_eq!(files.stats().purgeRuns, 1);
	assert_eq!(files.stats().purgedFiles, 1);
	assert_eq!(files.stats().linesWritten, 0);
	assert_eq!(HTracer::stats().accepted, 0);

	std::fs::write(root.join("new.trc"), "current session").unwrap();
	let latePath = root.join("late");
	std::fs::create_dir(&latePath).unwrap();
	std::fs::write(latePath.join("preserved.trc"), "late module").unwrap();
	let late = output(&latePath);
	let mut context = Context::default();
	context.module_add_arc("files", files.clone());
	context.module_add_arc("late", late.clone());
	install(context);
	HTracer::trace(&"new trace", Level::NORMAL, file!(), line!(), Vec::new());
	HTracer::shutdown_timeout(Duration::from_secs(5)).unwrap();
	assert_eq!(
		std::fs::read_to_string(root.join("new.trc")).unwrap(),
		"current session"
	);
	assert_eq!(
		std::fs::read_to_string(latePath.join("preserved.trc")).unwrap(),
		"late module"
	);
	assert_eq!(files.stats().purgeRuns, 1);
	assert_eq!(late.stats().purgeRuns, 0);
	assert_eq!(files.stats().linesWritten, 1);
	assert_eq!(late.stats().linesWritten, 1);
	assert_eq!(files.stats().purgeErrors, 0);
	drop((files, late));
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn startup_runs_again_in_new_process()
{
	const DIRECTORY: &str = "HTRACE_RETENTION_PROCESS_DIRECTORY";
	if let Some(directory) = std::env::var_os(DIRECTORY)
	{
		let path = Path::new(&directory);
		let files = output(path);
		let mut context = Context::default();
		context.module_add_arc("files", files.clone());
		install(context);
		HTracer::trace(&"session line", Level::NORMAL, file!(), line!(), Vec::new());
		HTracer::shutdown_timeout(Duration::from_secs(5)).unwrap();
		assert_eq!(files.stats().purgedFiles, 1);
		assert_eq!(files.stats().linesWritten, 1);
		return;
	}
	let root = temporaryDirectory();
	std::fs::write(root.join("legacy.trc"), "old session").unwrap();
	for _ in 0..2
	{
		let result = std::process::Command::new(std::env::current_exe().unwrap())
			.args([
				"--exact",
				"startup_runs_again_in_new_process",
				"--nocapture",
			])
			.env(DIRECTORY, &root)
			.output()
			.unwrap();
		assert!(result.status.success(), "child process failed: {result:?}");
		let files = std::fs::read_dir(&root)
			.unwrap()
			.map(|entry| entry.unwrap().path())
			.collect::<Vec<_>>();
		assert_eq!(files.len(), 1);
		assert_eq!(
			std::fs::read_to_string(&files[0]).unwrap(),
			"session line\n"
		);
	}
	std::fs::remove_dir_all(root).unwrap();
}
