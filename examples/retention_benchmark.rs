#![allow(non_snake_case)]

#[cfg(not(feature = "default_module"))]
fn main()
{
	eprintln!("retention_benchmark requires default_module");
}

#[cfg(feature = "default_module")]
fn main() -> Result<(), Box<dyn std::error::Error>>
{
	use htrace::components::{context::Context, level::Level};
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	use htrace::crates::bridge::HtraceBridge;
	use htrace::htracer::HTracer;
	use htrace::modules::{file::File, file_config::FileConfig};
	use std::sync::Arc;
	use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

	fn install(context: Context)
	{
		#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
		HTracer::globalContext_set(context);
		#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
		HTracer::globalContext_set(context, HtraceBridge::default());
	}

	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.len() != 3 || !["off", "age", "startup"].contains(&args[0].as_str())
	{
		return Err("usage: retention_benchmark off|age|startup FILES TRACES".into());
	}
	let mode = &args[0];
	let oldFiles: usize = args[1].parse()?;
	let traces: usize = args[2].parse()?;
	if !(1..=2_000).contains(&oldFiles) || !(oldFiles..=20_000).contains(&traces)
	{
		return Err("require 1..=2000 files and FILES..=20000 traces".into());
	}
	let directory = std::env::temp_dir().join(format!(
		"htrace-retention-benchmark-{}-{}",
		std::process::id(),
		SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
	));
	std::fs::create_dir(&directory)?;
	let oldTime = SystemTime::now() - Duration::from_secs(3 * 86_400);
	for index in 0..oldFiles
	{
		let file = std::fs::File::create(directory.join(format!("old-{index}.trc")))?;
		file.set_modified(oldTime)?;
	}
	let files = Arc::new(File::new(FileConfig {
		path: directory.to_string_lossy().into_owned(),
		byThreadId: false,
		forceInOneFile: Some("all".to_string()),
		lineFormat: "{msg}".to_string(),
		purgeAfterDays: (mode == "age").then_some(1),
		purgeOnStartup: mode == "startup",
		..FileConfig::default()
	}));
	let mut context = Context::default();
	context.module_add_arc("file", files.clone());
	let startup = Instant::now();
	install(context);
	HTracer::flush_timeout(Duration::from_secs(30))?;
	let startupUs = startup.elapsed().as_micros();
	let mut samples = Vec::with_capacity(traces);
	let emission = Instant::now();
	for _ in 0..traces
	{
		let call = Instant::now();
		HTracer::trace(
			&"retention benchmark",
			Level::NORMAL,
			file!(),
			line!(),
			Vec::new(),
		);
		samples.push(call.elapsed().as_nanos());
	}
	let emissionUs = emission.elapsed().as_micros();
	HTracer::flush_timeout(Duration::from_secs(30))?;
	let drainedUs = emission.elapsed().as_micros();
	install(Context::default());
	HTracer::shutdown_timeout(Duration::from_secs(30))?;
	let stats = files.stats();
	assert_eq!(stats.linesWritten, traces as u64);
	assert_eq!(
		stats.purgedFiles,
		if mode == "off" { 0 } else { oldFiles as u64 }
	);
	assert_eq!(stats.purgeErrors + stats.writeErrors + stats.openErrors, 0);
	samples.sort_unstable();
	println!(
		"{{\"mode\":\"{}\",\"files\":{},\"traces\":{},\"startup_us\":{},\"emission_us\":{},\"drained_us\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},\"purged_files\":{}}}",
		mode,
		oldFiles,
		traces,
		startupUs,
		emissionUs,
		drainedUs,
		samples[(traces - 1) / 2],
		samples[(traces - 1) * 95 / 100],
		samples[(traces - 1) * 99 / 100],
		stats.purgedFiles
	);
	drop(files);
	std::fs::remove_dir_all(directory)?;
	Ok(())
}
