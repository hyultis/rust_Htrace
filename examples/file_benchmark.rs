#![allow(non_snake_case)]

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>>
{
	let args = std::env::args().skip(1).collect::<Vec<_>>();
	if args.len() != 3
	{
		return Err("usage: file_benchmark open|cached|buffered LINES DESTINATIONS".into());
	}
	let mode = &args[0];
	if !["open", "cached", "buffered"].contains(&mode.as_str())
	{
		return Err("unknown mode".into());
	}
	let lines: usize = args[1].parse()?;
	let destinations: usize = args[2].parse()?;
	if !(1..=20_000).contains(&lines) || !(1..=64).contains(&destinations)
	{
		return Err("require 1..=20000 lines and 1..=64 destinations".into());
	}
	let directory = std::env::temp_dir().join(format!(
		"htrace-file-benchmark-{}-{}",
		std::process::id(),
		SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
	));
	std::fs::create_dir(&directory)?;
	let paths = (0..destinations)
		.map(|index| directory.join(format!("{index}.trc")))
		.collect::<Vec<_>>();
	let payload = b"file benchmark: fixed message payload, no formatting, tracing, backtrace, or queue; direct I/O comparison only\n";
	let mut cache: VecDeque<(usize, Box<dyn Write>)> = VecDeque::new();
	let mut peakHandles = 0;
	let start = Instant::now();
	for index in 0..lines
	{
		let destination = index % destinations;
		let path = &paths[destination];
		if mode == "open"
		{
			std::fs::create_dir_all(&directory)?;
			let mut file = OpenOptions::new().create(true).append(true).open(path)?;
			file.write_all(payload)?;
			peakHandles = 1;
			continue;
		}
		let mut entry = match cache.iter().position(|(key, _)| *key == destination)
		{
			Some(position) => cache.remove(position).ok_or("cache entry missing")?,
			None =>
			{
				if cache.len() == 16
				{
					let (_, mut evicted) = cache.pop_front().ok_or("cache eviction missing")?;
					evicted.flush()?;
				}
				std::fs::create_dir_all(&directory)?;
				let file: File = OpenOptions::new().create(true).append(true).open(path)?;
				let writer: Box<dyn Write> = if mode == "buffered"
				{
					Box::new(BufWriter::new(file))
				}
				else
				{
					Box::new(file)
				};
				(destination, writer)
			}
		};
		entry.1.write_all(payload)?;
		cache.push_back(entry);
		peakHandles = peakHandles.max(cache.len());
	}
	let writesNs = start.elapsed().as_nanos();
	for (_, writer) in &mut cache
	{
		writer.flush()?;
	}
	cache.clear();
	let totalNs = start.elapsed().as_nanos();
	let mut actualLines = 0;
	let mut actualBytes = 0;
	for path in &paths
	{
		if path.exists()
		{
			let bytes = std::fs::read(path)?;
			if !bytes.chunks(payload.len()).all(|chunk| chunk == payload)
			{
				return Err("file contents differ from complete expected lines".into());
			}
			actualBytes += bytes.len();
			actualLines += bytes.len() / payload.len();
		}
	}
	let valid = actualLines == lines && actualBytes == lines * payload.len();
	println!(
		"{{\"schema\":1,\"mode\":\"{mode}\",\"lines\":{lines},\"destinations\":{destinations},\"payload_bytes\":{},\"received_lines\":{actualLines},\"received_bytes\":{actualBytes},\"valid\":{valid},\"writes_ns\":{writesNs},\"total_ns\":{totalNs},\"lines_s\":{:.3},\"peak_handles\":{peakHandles}}}",
		payload.len(),
		lines as f64 * 1e9 / totalNs.max(1) as f64
	);
	std::fs::remove_dir_all(directory)?;
	if !valid
	{
		return Err("incomplete file benchmark output".into());
	}
	Ok(())
}
