use crate::Errors;
use crate::components::context::Context;
use crate::components::formater::FormaterCompiled;
use crate::components::trace::OneTrace;
use crate::modules::file_config::FileConfig;
use crate::modules::module_abstract::ModuleAbstract;
use crate::thread_manager::MAIN_THREAD;
use anyhow::Result;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::fs::{File as OutputFile, OpenOptions, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use time::macros::format_description;

mod retention;
use retention::Retention;

const FILE_CACHE_LIMIT: usize = 16;
/// Destination sets cached for the current date key before the cache is reset.
const PATH_CACHE_LIMIT: usize = 256;

/// Per-destination output counters. A successful line was accepted by write_all;
/// it is not a guarantee of fsync or physical durability. Snapshots are not atomic
/// across counters while other threads are writing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileStats
{
	pub linesWritten: u64,
	pub createErrors: u64,
	pub openErrors: u64,
	pub writeErrors: u64,
	pub flushErrors: u64,
	pub cachedHandles: usize,
	/// Started passes, including attempts against missing or unusable roots.
	pub purgeRuns: u64,
	/// Successful removals only; excluded entries are not failures.
	pub purgedFiles: u64,
	/// Directory, metadata, cache-close and removal failures.
	pub purgeErrors: u64,
}

/// Destination paths of the current date key (day, or hour with `byHour`),
/// keyed by source file then producer name ("" for a disabled grouping).
#[derive(Default)]
struct PathCache
{
	dateKey: Option<(time::Date, u8)>,
	date: String,
	entries: HashMap<String, HashMap<String, Arc<[PathBuf]>>>,
	count: usize,
}

struct FileState
{
	files: VecDeque<(PathBuf, OutputFile)>,
	retention: Retention,
}

pub struct File
{
	_name: String,
	_configs: FileConfig,
	_formaterCompiled: FormaterCompiled,
	_path: PathBuf,
	_purgeRootResolved: bool,
	_files: Mutex<FileState>,
	_paths: Mutex<PathCache>,
	_cachedHandles: AtomicUsize,
	_linesWritten: AtomicU64,
	_createErrors: AtomicU64,
	_openErrors: AtomicU64,
	_writeErrors: AtomicU64,
	_flushErrors: AtomicU64,
	_purgeRuns: AtomicU64,
	_purgedFiles: AtomicU64,
	_purgeErrors: AtomicU64,
}

impl File
{
	/// Create a file output. An empty or whitespace-only `config.path` is refused
	/// with [`Errors::EmptyPath`]: nothing is created or written.
	pub fn new(config: FileConfig) -> Result<File, Errors>
	{
		if config.path.trim().is_empty()
		{
			return Err(Errors::EmptyPath);
		}
		let fmtComp = (config.formaterCompiler)(&config.lineFormat);
		// Freeze relative destinations too, so changing cwd cannot redirect retention.
		// If cwd cannot be resolved, purge refuses this non-absolute root later.
		let resolved = std::path::absolute(&config.path);
		let purgeRootResolved = resolved.is_ok();
		// Preserve the original absolute spelling. Removing a final separator can
		// change Windows resolution for directory names ending in a space or dot.
		let path = resolved.unwrap_or_else(|_| PathBuf::from(&config.path));
		Ok(File {
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
			_path: path,
			_purgeRootResolved: purgeRootResolved,
			_files: Mutex::new(FileState {
				files: VecDeque::new(),
				retention: Retention::default(),
			}),
			_paths: Mutex::new(PathCache::default()),
			_cachedHandles: AtomicUsize::new(0),
			_linesWritten: AtomicU64::new(0),
			_createErrors: AtomicU64::new(0),
			_openErrors: AtomicU64::new(0),
			_writeErrors: AtomicU64::new(0),
			_flushErrors: AtomicU64::new(0),
			_purgeRuns: AtomicU64::new(0),
			_purgedFiles: AtomicU64::new(0),
			_purgeErrors: AtomicU64::new(0),
		})
	}

	/// Observe failures without sending diagnostics back into the logger.
	pub fn stats(&self) -> FileStats
	{
		FileStats {
			linesWritten: self._linesWritten.load(Ordering::Relaxed),
			createErrors: self._createErrors.load(Ordering::Relaxed),
			openErrors: self._openErrors.load(Ordering::Relaxed),
			writeErrors: self._writeErrors.load(Ordering::Relaxed),
			flushErrors: self._flushErrors.load(Ordering::Relaxed),
			cachedHandles: self._cachedHandles.load(Ordering::Relaxed),
			purgeRuns: self._purgeRuns.load(Ordering::Relaxed),
			purgedFiles: self._purgedFiles.load(Ordering::Relaxed),
			purgeErrors: self._purgeErrors.load(Ordering::Relaxed),
		}
	}

	fn generateLine(&self, trace: &OneTrace)
	{
		if self._configs.forceInOneFile.is_none()
			&& !self._configs.bySrc
			&& !self._configs.byThreadId
		{
			return;
		}
		let parameters = (self._configs.formaterParamBuilder)(trace, &self._configs.lineReturn);
		let mut line = self._formaterCompiled.render(parameters);
		line.push('\n');
		let paths = self.destinations(trace);
		self.writeToFiles(&paths, line.as_bytes());
	}

	/// Destination paths of a trace. The date component and the paths are
	/// computed once per date key and (source, producer) pair.
	fn destinations(&self, trace: &OneTrace) -> Arc<[PathBuf]>
	{
		let hour = if self._configs.byHour
		{
			trace.date.hour()
		}
		else
		{
			0
		};
		let dateKey = (trace.date.date(), hour);
		let source = if self._configs.bySrc
		{
			trace.filename.as_ref()
		}
		else
		{
			""
		};
		// Provenance is captured at emission, rather than read from the worker TLS.
		let producer = if self._configs.byThreadId
		{
			trace
				.context
				.threadName_get()
				.as_deref()
				.unwrap_or(MAIN_THREAD)
		}
		else
		{
			""
		};

		let mut cache = self._paths.lock();
		if cache.dateKey != Some(dateKey)
		{
			let dateFormat = if self._configs.byHour
			{
				format_description!("[year][month][day]_[hour repr:24]")
			}
			else
			{
				format_description!("[year][month][day]")
			};
			cache.date = trace
				.date
				.format(dateFormat)
				.unwrap_or_else(|_| "00000000".to_string());
			cache.dateKey = Some(dateKey);
			cache.entries.clear();
			cache.count = 0;
		}
		if let Some(paths) = cache
			.entries
			.get(source)
			.and_then(|producers| producers.get(producer))
		{
			return paths.clone();
		}
		if cache.count >= PATH_CACHE_LIMIT
		{
			cache.entries.clear();
			cache.count = 0;
		}
		let paths: Arc<[PathBuf]> = self.destinationsBuild(&cache.date, source, producer).into();
		cache
			.entries
			.entry(source.to_owned())
			.or_default()
			.insert(producer.to_owned(), paths.clone());
		cache.count += 1;
		paths
	}

	/// `source`/`producer` are only used when their grouping is enabled.
	fn destinationsBuild(&self, date: &str, source: &str, producer: &str) -> Vec<PathBuf>
	{
		let mut paths = Vec::with_capacity(3);
		if let Some(filename) = &self._configs.forceInOneFile
		{
			Self::pushUnique(
				&mut paths,
				self.destinationPath(&format!("{filename}_{date}.trc")),
			);
		}
		if self._configs.bySrc
		{
			let basename = source.rsplit(['/', '\\']).next().unwrap_or("");
			let filename = Self::filenameComponent(basename.split('.').next().unwrap_or(""));
			Self::pushUnique(
				&mut paths,
				self.destinationPath(&format!("{filename}_{date}.trc")),
			);
		}
		if self._configs.byThreadId
		{
			let filename = Self::filenameComponent(producer);
			Self::pushUnique(
				&mut paths,
				self.destinationPath(&format!("{filename}_{date}.trc")),
			);
		}
		paths
	}

	// Destinations resolving to the same path receive the line once. Paths are
	// compared exactly: names differing only by case remain distinct, even on
	// case-insensitive filesystems.
	fn pushUnique(paths: &mut Vec<PathBuf>, path: PathBuf)
	{
		if !paths.contains(&path)
		{
			paths.push(path);
		}
	}

	// Preserve forceInOneFile path semantics, including separators and .. .
	fn destinationPath(&self, name: &str) -> PathBuf
	{
		let mut path = self._path.as_os_str().to_owned();
		path.push("/");
		path.push(name);
		PathBuf::from(path)
	}

	// Keep readable names without allowing separators or dot segments.
	// Names that sanitize to the same component intentionally share a file.
	fn filenameComponent(name: &str) -> String
	{
		let mut component = String::new();
		for character in name.chars()
		{
			if character.is_alphanumeric() || matches!(character, '_' | '-' | ' ' | '(' | ')')
			{
				component.push(character);
			}
			else
			{
				component.push('_');
			}
		}
		component
	}

	fn writeToFiles(&self, paths: &[PathBuf], line: &[u8])
	{
		// The same lock covers appends, closing cached handles and deletion.
		let mut state = self._files.lock();
		state.retention.startupAttempted = true;
		let mut written = false;
		for path in paths
		{
			written |= self.writeToFile(&mut state.files, path, line);
		}
		if written
		{
			let _ = self.purgePeriodic(
				&mut state,
				paths,
				std::time::SystemTime::now(),
				std::time::Instant::now(),
			);
		}
	}

	fn writeToFile(
		&self,
		files: &mut VecDeque<(PathBuf, OutputFile)>,
		filepath: &Path,
		line: &[u8],
	) -> bool
	{
		let mut entry = match files.iter().position(|(path, _)| path == filepath)
		{
			Some(position) => files.remove(position),
			None =>
			{
				if files.len() == FILE_CACHE_LIMIT
				{
					if let Some((_, mut evicted)) = files.pop_front()
					{
						self._cachedHandles.store(files.len(), Ordering::Relaxed);
						// Recorded even during eviction; direct files have no userspace buffer.
						let _ = self.flushWriter(&mut evicted);
					}
				}
				let Some(parent) = filepath.parent()
				else
				{
					self._createErrors.fetch_add(1, Ordering::Relaxed);
					return false;
				};
				if create_dir_all(parent).is_err()
				{
					self._createErrors.fetch_add(1, Ordering::Relaxed);
					return false;
				}
				match OpenOptions::new().create(true).append(true).open(filepath)
				{
					Ok(file) => Some((filepath.to_owned(), file)),
					Err(_) =>
					{
						self._openErrors.fetch_add(1, Ordering::Relaxed);
						return false;
					}
				}
			}
		};
		self._cachedHandles.store(
			files.len() + usize::from(entry.is_some()),
			Ordering::Relaxed,
		);
		if let Some((_, file)) = entry.as_mut()
		{
			if self.writeLine(file, line).is_err()
			{
				// Retry opening on the next trace after a write failure.
				self._cachedHandles.store(files.len(), Ordering::Relaxed);
				return false;
			}
		}
		if let Some(entry) = entry
		{
			files.push_back(entry);
		}
		true
	}

	fn writeLine(&self, writer: &mut impl Write, line: &[u8]) -> io::Result<()>
	{
		match writer.write_all(line)
		{
			Ok(()) =>
			{
				self._linesWritten.fetch_add(1, Ordering::Relaxed);
				Ok(())
			}
			Err(error) =>
			{
				self._writeErrors.fetch_add(1, Ordering::Relaxed);
				Err(error)
			}
		}
	}

	fn flushWriter(&self, writer: &mut impl Write) -> io::Result<()>
	{
		writer.flush().inspect_err(|_| {
			self._flushErrors.fetch_add(1, Ordering::Relaxed);
		})
	}
}

impl ModuleAbstract for File
{
	/// Direct callers may run this once before writing; later calls are harmless.
	fn event_onStartup(&self) -> Result<()>
	{
		self.purgeStartup().map_err(Into::into)
	}

	fn name_set(&mut self, moduleName: String) -> Result<()>
	{
		self._name = moduleName;
		return Ok(());
	}

	fn name_get(&self) -> Result<String>
	{
		return Ok(self._name.clone());
	}

	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onError(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.generateLine(trace);
	}

	fn event_onContextExit(&self, _: &Context)
	{
		// flush() records any failure in stats, without recursively tracing it.
		let _ = self.flush();
	}

	fn event_onGlobalExit(&self)
	{
		let _ = self.flush();
	}

	fn flush(&self) -> Result<()>
	{
		let mut state = self._files.lock();
		let mut firstError = None;
		for (_, file) in state.files.iter_mut()
		{
			if let Err(error) = self.flushWriter(file)
			{
				if firstError.is_none()
				{
					firstError = Some(error);
				}
			}
		}
		// A flush advances an existing scan, but never starts an idle cadence.
		if let Err(error) = self.purgeAdvance(&mut state, &[], std::time::SystemTime::now())
			&& firstError.is_none()
		{
			firstError = Some(error);
		}
		match firstError
		{
			Some(error) => Err(error.into()),
			None => Ok(()),
		}
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	struct PartialWriter
	{
		bytes: Vec<u8>,
		remaining: usize,
		failFlush: bool,
	}

	impl Write for PartialWriter
	{
		fn write(&mut self, data: &[u8]) -> io::Result<usize>
		{
			if self.remaining == 0
			{
				return Err(io::Error::other("injected write failure"));
			}
			let count = data.len().min(2).min(self.remaining);
			self.bytes.extend_from_slice(&data[..count]);
			self.remaining -= count;
			Ok(count)
		}

		fn flush(&mut self) -> io::Result<()>
		{
			if self.failFlush
			{
				Err(io::Error::other("injected flush failure"))
			}
			else
			{
				Ok(())
			}
		}
	}

	#[test]
	fn filename_characters_are_readable_and_contained()
	{
		assert_eq!(
			File::filenameComponent("Équipe 東京 (0)"),
			"Équipe 東京 (0)"
		);
		assert_eq!(File::filenameComponent("../escape\\name"), "___escape_name");
		for character in [
			'/', '\\', '.', ':', '*', '?', '"', '<', '>', '|', '%', '\0', '\n', '\r', '\t',
		]
		{
			assert_eq!(File::filenameComponent(&format!("a{character}b")), "a_b");
		}
	}

	fn traceAt(date: time::OffsetDateTime, filename: &str, thread: Option<&str>) -> OneTrace
	{
		let mut context = Context::default();
		if let Some(thread) = thread
		{
			context.threadName_set(thread);
		}
		OneTrace {
			message: String::new(),
			date,
			level: crate::components::level::Level::NORMAL,
			context: Arc::new(context),
			filename: filename.to_string().into(),
			fileline: 1,
			backtraces: Vec::new(),
		}
	}

	#[test]
	fn cached_destinations_follow_date_source_and_producer()
	{
		let output = File::new(FileConfig {
			path: "root/../out".to_string(),
			forceInOneFile: Some("all".to_string()),
			bySrc: true,
			byThreadId: true,
			byHour: true,
			..FileConfig::default()
		})
		.unwrap();
		let root = output._path.as_os_str().to_string_lossy().into_owned();
		let names = |paths: Arc<[PathBuf]>| {
			paths
				.iter()
				.map(|path| {
					let path = path.as_os_str().to_string_lossy().into_owned();
					assert!(path.starts_with(&format!("{root}/")), "{path}");
					path[root.len() + 1..].to_string()
				})
				.collect::<Vec<_>>()
		};
		let morning = time::macros::datetime!(2026-01-02 08:15 UTC);
		let first = output.destinations(&traceAt(morning, "src/app.rs", None));
		assert_eq!(
			names(first.clone()),
			[
				"all_20260102_08.trc",
				"app_20260102_08.trc",
				"MAIN_20260102_08.trc"
			]
		);
		// Same key: the cached set is reused.
		let again = output.destinations(&traceAt(
			time::macros::datetime!(2026-01-02 08:59 UTC),
			"src/app.rs",
			None,
		));
		assert!(Arc::ptr_eq(&first, &again));
		assert_eq!(
			names(output.destinations(&traceAt(morning, "lib/app.rs", Some("all")))),
			["all_20260102_08.trc", "app_20260102_08.trc"]
		);
		assert_eq!(
			names(output.destinations(&traceAt(
				time::macros::datetime!(2026-01-02 09:00 UTC),
				"src/app.rs",
				Some("w/1"),
			))),
			[
				"all_20260102_09.trc",
				"app_20260102_09.trc",
				"w_1_20260102_09.trc"
			]
		);
	}

	#[test]
	fn complete_partial_writes_and_count_errors()
	{
		let output = File::new(FileConfig::default()).unwrap();
		let mut writer = PartialWriter {
			bytes: Vec::new(),
			remaining: 100,
			failFlush: false,
		};
		output.writeLine(&mut writer, b"complete\n").unwrap();
		assert_eq!(writer.bytes, b"complete\n");
		assert_eq!(output.stats().linesWritten, 1);
		let mut failing = PartialWriter {
			bytes: Vec::new(),
			remaining: 3,
			failFlush: true,
		};
		assert!(output.writeLine(&mut failing, b"partial\n").is_err());
		assert_eq!(failing.bytes, b"par");
		assert_eq!(output.stats().linesWritten, 1);
		assert_eq!(output.stats().writeErrors, 1);
		assert!(output.flushWriter(&mut failing).is_err());
		assert_eq!(output.stats().flushErrors, 1);
	}
}
