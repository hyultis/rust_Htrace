#![allow(non_snake_case)]

use htrace::HTrace;
use htrace::components::context::Context;
use htrace::components::level::Level;
use htrace::components::trace::OneTrace;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::modules::module_abstract::ModuleAbstract;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

struct Counter
{
	name: String,
	received: Arc<AtomicUsize>,
	slow: bool,
}

impl Counter
{
	fn receive(&self, trace: &OneTrace)
	{
		std::hint::black_box(trace);
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
		_ =>
		{
			HTrace!("benchmark");
		}
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
		return Err("usage: benchmark filtered|simple|formatted|backtrace|slow PRODUCERS TRACES_PER_PRODUCER".into());
	}
	let mode = args[0].clone();
	if !["filtered", "simple", "formatted", "backtrace", "slow"].contains(&mode.as_str())
	{
		return Err("unknown benchmark mode".into());
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
		},
	);
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	HTracer::globalContext_set(context, HtraceBridge::default());

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
	Ok(())
}
