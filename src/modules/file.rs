use crate::components::context::Context;
use crate::components::formater::FormaterCompiled;
use crate::components::trace::OneTrace;
use crate::modules::file_config::FileConfig;
use crate::modules::module_abstract::ModuleAbstract;
use crate::thread_manager::MAIN_THREAD;
use anyhow::Result;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fs::{File as OutputFile, OpenOptions, create_dir_all};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use time::macros::format_description;

const FILE_CACHE_LIMIT: usize = 16;

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
}

pub struct File
{
	_name: String,
	_configs: FileConfig,
	_formaterCompiled: FormaterCompiled,
	_files: Mutex<VecDeque<(PathBuf, OutputFile)>>,
	_cachedHandles: AtomicUsize,
	_linesWritten: AtomicU64,
	_createErrors: AtomicU64,
	_openErrors: AtomicU64,
	_writeErrors: AtomicU64,
	_flushErrors: AtomicU64,
}

impl File
{
	pub fn new(config: FileConfig) -> File
	{
		let fmtComp = (config.formaterCompiler)(&config.lineFormat);
		File {
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
			_files: Mutex::new(VecDeque::new()),
			_cachedHandles: AtomicUsize::new(0),
			_linesWritten: AtomicU64::new(0),
			_createErrors: AtomicU64::new(0),
			_openErrors: AtomicU64::new(0),
			_writeErrors: AtomicU64::new(0),
			_flushErrors: AtomicU64::new(0),
		}
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
		let line = format!("{}\n", self._formaterCompiled.render(parameters));
		let dateFormat = if self._configs.byHour
		{
			format_description!("[year][month][day]_[hour repr:24]")
		}
		else
		{
			format_description!("[year][month][day]")
		};
		let date = trace
			.date
			.format(dateFormat)
			.unwrap_or_else(|_| "00000000".to_string());
		if let Some(filename) = &self._configs.forceInOneFile
		{
			self.writeToFile(
				Path::new(&format!("{}/{}_{}.trc", self._configs.path, filename, date)),
				line.as_bytes(),
			);
		}
		if self._configs.bySrc
		{
			let basename = trace.filename.rsplit(['/', '\\']).next().unwrap_or("");
			let filename = Self::filenameComponent(basename.split('.').next().unwrap_or(""));
			self.writeToFile(
				Path::new(&format!("{}/{}_{}.trc", self._configs.path, filename, date)),
				line.as_bytes(),
			);
		}
		if self._configs.byThreadId
		{
			// Provenance is captured at emission, rather than read from the worker TLS.
			let producer = trace
				.context
				.threadName_get()
				.as_deref()
				.unwrap_or(MAIN_THREAD);
			let filename = Self::filenameComponent(producer);
			self.writeToFile(
				Path::new(&format!("{}/{}_{}.trc", self._configs.path, filename, date)),
				line.as_bytes(),
			);
		}
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

	fn writeToFile(&self, filepath: &Path, line: &[u8])
	{
		// Serialize direct concurrent calls too, keeping each complete line together.
		let mut files = self._files.lock();
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
					return;
				};
				if create_dir_all(parent).is_err()
				{
					self._createErrors.fetch_add(1, Ordering::Relaxed);
					return;
				}
				match OpenOptions::new().create(true).append(true).open(filepath)
				{
					Ok(file) => Some((filepath.to_owned(), file)),
					Err(_) =>
					{
						self._openErrors.fetch_add(1, Ordering::Relaxed);
						return;
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
				return;
			}
		}
		if let Some(entry) = entry
		{
			files.push_back(entry);
		}
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
		let mut files = self._files.lock();
		let mut firstError = None;
		for (_, file) in files.iter_mut()
		{
			if let Err(error) = self.flushWriter(file)
			{
				if firstError.is_none()
				{
					firstError = Some(error);
				}
			}
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

	#[test]
	fn complete_partial_writes_and_count_errors()
	{
		let output = File::new(FileConfig::default());
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
