#![allow(non_snake_case)]

use htrace::components::context::Context;
use htrace::components::formater::{FormaterCompile, FormaterCompiled, FormaterParamBuilder};
use htrace::components::level::Level;
use htrace::components::trace::OneTrace;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
#[cfg(feature = "default_module")]
use htrace::modules::file::File;
#[cfg(feature = "default_module")]
use htrace::modules::file_config::FileConfig;
use htrace::modules::module_abstract::ModuleAbstract;
use htrace::{HTrace, Spaned};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

const MODES: [&str; 11] = [
	"filtered",
	"simple",
	"formatted",
	"backtrace",
	"slow",
	"spans3",
	"span_per_trace",
	"render",
	"file",
	"log",
	"tracing",
];
const RENDER_FORMAT: &str = "{time} {lvl} ({thread:>, }{file}:l{line}) : {msg}";
const RENDER_LINE_RETURN: &str = " | ";

struct Counter
{
	name: String,
	received: Arc<AtomicUsize>,
	slow: bool,
	/// Approximates per-module worker rendering with the default formatter.
	render: Option<(FormaterCompiled, String)>,
}

impl Counter
{
	fn receive(&self, trace: &OneTrace)
	{
		std::hint::black_box(trace);
		if let Some((compiled, lineReturn)) = &self.render
		{
			std::hint::black_box(compiled.render(FormaterParamBuilder(trace, lineReturn)));
		}
		if self.slow
		{
			thread::sleep(Duration::from_micros(100));
		}
		self.received.fetch_add(1, Ordering::Release);
	}
}

impl ModuleAbstract for Counter
{
	fn name_set(&mut self, name: String) -> anyhow::Result<()>
	{
		self.name = name;
		Ok(())
	}
	fn name_get(&self) -> anyhow::Result<String>
	{
		Ok(self.name.clone())
	}
	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onError(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.receive(trace);
	}
	fn event_onContextExit(&self, _: &Context) {}
	fn event_onGlobalExit(&self) {}
}

fn emit(mode: &str, producer: usize, index: usize)
{
	match mode
	{
		"filtered" =>
		{
			HTrace!((Level::DEBUG) "benchmark");
		}
		"formatted" =>
		{
			HTrace!("benchmark {} {}", producer, index);
		}
		"backtrace" =>
		{
			HTrace!((Level::ERROR) "benchmark");
		}
		"span_per_trace" =>
		{
			let mut context = Context::default();
			context.name_set("bench");
			Spaned!(context);
			HTrace!("benchmark");
		}
		#[cfg(feature = "log_consumer")]
		"log" =>
		{
			log::info!("benchmark");
		}
		#[cfg(feature = "tracing_consumer")]
		"tracing" =>
		{
			tracing::info!("benchmark");
		}
		_ =>
		{
			HTrace!("benchmark");
		}
	}
}

/// Run a producer body inside the per-producer scope required by the mode.
fn producerScope(mode: &str, body: impl FnOnce() -> Vec<u128>) -> Vec<u128>
{
	match mode
	{
		"spans3" =>
		{
			let mut outer = Context::default();
			outer.name_set("bench outer");
			Spaned!(outer);
			let mut middle = Context::default();
			middle.name_set("bench middle");
			Spaned!(middle);
			let mut inner = Context::default();
			inner.name_set("bench inner");
			Spaned!(inner);
			body()
		}
		#[cfg(feature = "tracing_consumer")]
		"tracing" =>
		{
			let _entered = tracing::info_span!("bench").entered();
			body()
		}
		_ => body(),
	}
}

/// Fresh trace directory for the file mode, removed when dropped.
#[cfg(feature = "default_module")]
struct TempDirectory(std::path::PathBuf);

#[cfg(feature = "default_module")]
impl TempDirectory
{
	fn create() -> Result<Self, Box<dyn std::error::Error>>
	{
		let path = std::env::temp_dir().join(format!(
			"htrace-benchmark-file-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)?
				.as_nanos()
		));
		std::fs::create_dir(&path)?;
		Ok(Self(path))
	}
}

#[cfg(feature = "default_module")]
impl Drop for TempDirectory
{
	fn drop(&mut self)
	{
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn linuxStatus() -> (usize, usize)
{
	let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
	let value = |key: &str| {
		status
			.lines()
			.find_map(|line| {
				line.strip_prefix(key)?
					.split_whitespace()
					.next()?
					.parse::<usize>()
					.ok()
			})
			.unwrap_or(0)
	};
	(value("Threads:"), value("VmHWM:"))
}

fn percentile(samples: &[u128], percentile: usize) -> u128
{
	samples[((samples.len() - 1) * percentile) / 100]
}

fn main() -> Result<(), Box<dyn std::error::Error>>
{
	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.len() != 3
	{
		return Err(format!(
			"usage: benchmark {} PRODUCERS TRACES_PER_PRODUCER",
			MODES.join("|")
		)
		.into());
	}
	let mode = args[0].clone();
	if !MODES.contains(&mode.as_str())
	{
		return Err("unknown benchmark mode".into());
	}
	#[cfg(not(feature = "default_module"))]
	if mode == "file"
	{
		return Err("mode file requires feature default_module".into());
	}
	#[cfg(not(feature = "log_consumer"))]
	if mode == "log"
	{
		return Err("mode log requires feature log_consumer".into());
	}
	#[cfg(not(feature = "tracing_consumer"))]
	if mode == "tracing"
	{
		return Err("mode tracing requires feature tracing_consumer".into());
	}
	let producers: usize = args[1].parse()?;
	let traces: usize = args[2].parse()?;
	if !(1..=16).contains(&producers) || traces == 0
	{
		return Err("require 1..=16 producers and positive traces".into());
	}
	let calls = producers.checked_mul(traces).ok_or("volume overflow")?;
	let maxCalls = match mode.as_str()
	{
		"backtrace" => 320,
		"slow" => 2_000,
		_ => 20_000,
	};
	if calls > maxCalls
	{
		return Err(format!("volume exceeds bounded maximum {maxCalls}").into());
	}
	let received = Arc::new(AtomicUsize::new(0));
	let mut context = Context::default();
	context.level_setMin(Some(
		if mode == "filtered"
		{
			Level::ERROR
		}
		else
		{
			Level::DEBUG
		},
	));
	context.module_add(
		"counter",
		Counter {
			name: "counter".to_string(),
			received: received.clone(),
			slow: mode == "slow",
			render: (mode == "render").then(|| {
				(
					FormaterCompile(&RENDER_FORMAT.to_string()),
					RENDER_LINE_RETURN.to_string(),
				)
			}),
		},
	);
	// Real file output (byThreadId default layout) alongside the counter.
	#[cfg(feature = "default_module")]
	let mut fileOutput: Option<(Arc<File>, TempDirectory)> = None;
	#[cfg(feature = "default_module")]
	if mode == "file"
	{
		let directory = TempDirectory::create()?;
		let file = Arc::new(File::new(FileConfig {
			path: directory.0.to_string_lossy().into_owned(),
			..FileConfig::default()
		})?);
		context.module_add_arc("file", file.clone());
		fileOutput = Some((file, directory));
	}
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context).expect("Htrace configuration failed");
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default())
		.expect("Htrace configuration failed");

	// Warm one call without terminating the singleton. Account for it separately.
	emit(&mode, 0, 0);
	let warmupExpected = usize::from(mode != "filtered");
	let warmupStart = Instant::now();
	while received.load(Ordering::Acquire) < warmupExpected
	{
		if warmupStart.elapsed() > Duration::from_secs(5)
		{
			return Err("warmup did not drain within 5s".into());
		}
		thread::yield_now();
	}
	let mut timerSamples = (0..1_000)
		.map(|_| {
			let start = Instant::now();
			start.elapsed().as_nanos()
		})
		.collect::<Vec<_>>();
	timerSamples.sort_unstable();
	let done = Arc::new(AtomicBool::new(false));
	let sampleDone = done.clone();
	let sampleReceived = received.clone();
	let completedCalls = Arc::new(AtomicUsize::new(0));
	let sampleCompleted = completedCalls.clone();
	let sampler = thread::spawn(move || {
		let mut peakThreads = 0;
		let sampleStart = Instant::now();
		let mut progress = Vec::new();
		while !sampleDone.load(Ordering::Acquire)
		{
			let (threads, rssKiB) = linuxStatus();
			peakThreads = peakThreads.max(threads);
			let emitted = sampleCompleted.load(Ordering::Acquire);
			let drained = sampleReceived
				.load(Ordering::Acquire)
				.saturating_sub(warmupExpected);
			progress.push(format!(
				"[{}, {}, {}, {}]",
				sampleStart.elapsed().as_micros(),
				emitted,
				drained,
				rssKiB
			));
			thread::sleep(Duration::from_millis(1));
		}
		(peakThreads, progress)
	});
	let barrier = Arc::new(Barrier::new(producers + 1));
	let mut workers = Vec::new();
	for producer in 0..producers
	{
		let mode = mode.clone();
		let barrier = barrier.clone();
		let completedCalls = completedCalls.clone();
		workers.push(thread::spawn(move || {
			producerScope(&mode, || {
				let mut samples = Vec::with_capacity(traces);
				barrier.wait();
				for index in 0..traces
				{
					let start = Instant::now();
					emit(&mode, producer, index);
					samples.push(start.elapsed().as_nanos());
					completedCalls.fetch_add(1, Ordering::Release);
				}
				samples
			})
		}));
	}
	let start = Instant::now();
	barrier.wait();
	let mut samples = Vec::with_capacity(calls);
	for worker in workers
	{
		samples.extend(worker.join().map_err(|_| "producer panicked")?);
	}
	let emissionNs = start.elapsed().as_nanos();
	let receivedAtEmission = received
		.load(Ordering::Acquire)
		.saturating_sub(warmupExpected);
	let drainStart = Instant::now();
	HTracer::shutdown().map_err(|error| error.to_string())?;
	let drainNs = drainStart.elapsed().as_nanos();
	let totalNs = start.elapsed().as_nanos();
	let actual = received
		.load(Ordering::Acquire)
		.saturating_sub(warmupExpected);
	done.store(true, Ordering::Release);
	let (peakThreads, progress) = sampler.join().map_err(|_| "sampler panicked")?;
	let peakRssKiB = linuxStatus().1;
	samples.sort_unstable();
	let expected = if mode == "filtered" { 0 } else { calls };
	println!(
		concat!(
			"{{\"schema\":1,\"mode\":\"{}\",\"producers\":{},\"traces_per_producer\":{},",
			"\"calls\":{},\"expected\":{},\"received\":{},\"received_at_emission\":{},",
			"\"missing\":{},\"excess\":{},\"valid\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{},",
			"\"instant_pair_p50_ns\":{},\"emission_ns\":{},\"drain_ns\":{},\"total_ns\":{},",
			"\"emission_calls_s\":{:.3},\"drained_traces_s\":{:.3},\"sampled_peak_threads\":{},\"process_peak_rss_kib\":{},\"progress\":[{}]}}"
		),
		mode,
		producers,
		traces,
		calls,
		expected,
		actual,
		receivedAtEmission,
		expected.saturating_sub(actual),
		actual.saturating_sub(expected),
		actual == expected,
		percentile(&samples, 50),
		percentile(&samples, 95),
		percentile(&samples, 99),
		percentile(&timerSamples, 50),
		emissionNs,
		drainNs,
		totalNs,
		calls as f64 * 1e9 / emissionNs.max(1) as f64,
		actual as f64 * 1e9 / totalNs.max(1) as f64,
		peakThreads,
		peakRssKiB,
		if mode == "slow"
		{
			progress.join(",")
		}
		else
		{
			String::new()
		}
	);
	if actual != expected
	{
		return Err("received trace count differs from expected; benchmark invalid".into());
	}
	#[cfg(feature = "default_module")]
	if let Some((file, _directory)) = &fileOutput
	{
		let stats = file.stats();
		let errors = stats.createErrors + stats.openErrors + stats.writeErrors + stats.flushErrors;
		let expectedLines = (expected + warmupExpected) as u64;
		if stats.linesWritten != expectedLines || errors != 0
		{
			return Err(format!(
				"file module wrote {} lines (expected {expectedLines}) with {errors} I/O errors; benchmark invalid",
				stats.linesWritten
			)
			.into());
		}
	}
	Ok(())
}
