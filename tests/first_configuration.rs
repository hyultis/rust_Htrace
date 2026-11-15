#![allow(non_snake_case)]

#[path = "support/capture.rs"]
mod capture;

use capture::Capture;
use htrace::components::context::Context;
#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
use htrace::crates::bridge::HtraceBridge;
use htrace::htracer::HTracer;
use htrace::{Errors, HTrace};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

const PRODUCERS: usize = 4;
const AFTER_START: usize = 200;

fn install(context: Context) -> Result<(), Errors>
{
	#[cfg(not(any(feature = "tracing_consumer", feature = "log_consumer")))]
	return HTracer::globalContext_set(context);
	#[cfg(any(feature = "tracing_consumer", feature = "log_consumer"))]
	return HTracer::globalContext_set(context, HtraceBridge::default());
}

// Single test: the first configuration is process-wide.
// Producers race the very first configuration. Whatever the interleaving, a
// trace accepted by the worker must reach the configured modules: the global
// context is published before admission opens. The race window is narrow, so
// a regression is detected only probabilistically, but the assertion never
// fails spuriously with the current ordering.
#[test]
fn traces_accepted_during_first_configuration_reach_its_modules()
{
	let capture = Capture::new();
	let mut context: Context = "first".into();
	context.module_add("capture", capture.clone());

	let stop = Arc::new(AtomicBool::new(false));
	let afterStart = Arc::new(AtomicUsize::new(0));
	let ready = Arc::new(Barrier::new(PRODUCERS + 1));
	let producers: Vec<_> = (0..PRODUCERS)
		.map(|_| {
			let stop = stop.clone();
			let afterStart = afterStart.clone();
			let ready = ready.clone();
			thread::spawn(move || {
				ready.wait();
				while !stop.load(Ordering::Relaxed)
				{
					HTrace!("racing trace");
					if HTracer::stats().accepted > 0
					{
						afterStart.fetch_add(1, Ordering::Relaxed);
					}
				}
			})
		})
		.collect();

	ready.wait();
	install(context).unwrap();
	while afterStart.load(Ordering::Relaxed) < AFTER_START
	{
		thread::yield_now();
	}
	stop.store(true, Ordering::Relaxed);
	for producer in producers
	{
		producer.join().unwrap();
	}
	HTracer::flush().unwrap();

	let stats = HTracer::stats();
	assert!(stats.accepted > 0);
	assert_eq!(capture.count() as u64, stats.accepted, "{stats:?}");
	HTracer::shutdown().unwrap();
}
