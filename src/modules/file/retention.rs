use super::{File, FileState};
use std::ffi::OsStr;
use std::fs::{self, Metadata, ReadDir};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

#[cfg(test)]
mod audit_tests;

// These bound work, not syscall latency. Idle scans resume on writes or flushes.
const ENTRY_BUDGET: usize = 64;
const REMOVE_BUDGET: usize = 8;
const PURGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub(super) struct Retention
{
	pub(super) startupAttempted: bool,
	lastRun: Option<Instant>,
	scan: Option<ReadDir>,
	anchor: Option<(PathBuf, Metadata)>,
}

impl File
{
	fn hasDestinations(&self) -> bool
	{
		self._configs.forceInOneFile.is_some() || self._configs.bySrc || self._configs.byThreadId
	}

	pub(super) fn purgeStartup(&self) -> io::Result<()>
	{
		let mut state = self._files.lock();
		if state.retention.startupAttempted
		{
			return Ok(());
		}
		// Consume the attempt even on failure; never delete newly written logs later.
		state.retention.startupAttempted = true;
		if !self._configs.purgeOnStartup || !self.hasDestinations()
		{
			return Ok(());
		}
		self.purgeBegin(&mut state)?;
		let mut firstError = None;
		while state.retention.scan.is_some()
		{
			if let Err(error) = self.purgeBatch(&mut state, &[], None)
				&& firstError.is_none()
			{
				firstError = Some(error);
			}
		}
		firstError.map_or(Ok(()), Err)
	}

	pub(super) fn purgePeriodic(
		&self,
		state: &mut FileState,
		protected: &[PathBuf],
		now: SystemTime,
		monotonic: Instant,
	) -> io::Result<()>
	{
		if !self.hasDestinations()
			|| self
				._configs
				.purgeAfterDays
				.filter(|days| *days > 0)
				.is_none()
		{
			return Ok(());
		}
		if state.retention.scan.is_none()
		{
			if state
				.retention
				.lastRun
				.is_some_and(|last| monotonic.saturating_duration_since(last) < PURGE_INTERVAL)
			{
				return Ok(());
			}
			state.retention.lastRun = Some(monotonic);
			self.purgeBegin(state)?;
		}
		self.purgeAdvance(state, protected, now)
	}

	pub(super) fn purgeAdvance(
		&self,
		state: &mut FileState,
		protected: &[PathBuf],
		now: SystemTime,
	) -> io::Result<()>
	{
		let Some(days) = self._configs.purgeAfterDays.filter(|days| *days > 0)
		else
		{
			return Ok(());
		};
		self.purgeBatch(
			state,
			protected,
			Some((now, Duration::from_secs(u64::from(days) * 86_400))),
		)
	}

	fn purgeBegin(&self, state: &mut FileState) -> io::Result<()>
	{
		self._purgeRuns.fetch_add(1, Ordering::Relaxed);
		let result = (|| {
			if !self._purgeRootResolved || !self._path.is_absolute()
			{
				return Err(io::Error::other(
					"trace purge root could not be made absolute",
				));
			}
			let (canonical, metadata) = match self.purgeRoot()
			{
				Ok(metadata) => metadata,
				Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
				Err(error) => return Err(error),
			};
			if state.retention.anchor.is_none()
			{
				state.retention.anchor = Some((canonical.clone(), metadata));
			}
			self.purgeCheckRoot(&state.retention)?;
			state.retention.scan = Some(fs::read_dir(canonical)?);
			Ok(())
		})();
		result.inspect_err(|_| {
			self._purgeErrors.fetch_add(1, Ordering::Relaxed);
		})
	}

	fn purgeRoot(&self) -> io::Result<(PathBuf, Metadata)>
	{
		// A trailing slash/dot can follow a root symlink on Unix. Check the final
		// component without it, but refuse spellings that resolve differently.
		let stripped: PathBuf = self._path.components().collect();
		let metadata = fs::symlink_metadata(&stripped)?;
		if !metadata.is_dir() || isLink(&metadata)
		{
			return Err(io::Error::other(
				"trace purge root must be a real directory",
			));
		}
		let canonical = fs::canonicalize(&self._path)?;
		if canonical.file_name().is_none() || fs::canonicalize(&stripped)? != canonical
		{
			return Err(io::Error::other(
				"trace purge root is a pathname root or ambiguous path",
			));
		}
		Ok((canonical, metadata))
	}

	// std path checks assume the application keeps this directory and its parents
	// stable throughout purge. They cannot make remove_file atomic with these checks.
	fn purgeCheckRoot(&self, retention: &Retention) -> io::Result<()>
	{
		let (canonical, original) = retention
			.anchor
			.as_ref()
			.ok_or_else(|| io::Error::other("trace purge root is not anchored"))?;
		let (currentPath, current) = self.purgeRoot()?;
		if !sameIdentity(original, &current) || currentPath != *canonical
		{
			return Err(io::Error::other("trace purge root changed"));
		}
		Ok(())
	}

	fn purgeBatch(
		&self,
		state: &mut FileState,
		protected: &[PathBuf],
		age: Option<(SystemTime, Duration)>,
	) -> io::Result<()>
	{
		if state.retention.scan.is_none()
		{
			return Ok(());
		}
		let protected: Vec<_> = protected
			.iter()
			.filter_map(|path| fs::canonicalize(path).ok())
			.collect();
		let mut firstError = None;
		let mut removed = 0;
		for _ in 0..ENTRY_BUDGET
		{
			if removed == REMOVE_BUDGET
			{
				break;
			}
			let entry = state.retention.scan.as_mut().and_then(Iterator::next);
			let result = match entry
			{
				None =>
				{
					state.retention.scan = None;
					break;
				}
				Some(Err(error)) => Err(error),
				Some(Ok(entry)) =>
				{
					if let Err(error) = self.purgeCheckRoot(&state.retention)
					{
						state.retention.scan = None;
						Err(error)
					}
					else
					{
						self.purgeEntry(state, &entry.file_name(), &protected, age)
							.map(|deleted| {
								removed += usize::from(deleted);
							})
					}
				}
			};
			if let Err(error) = result
			{
				self._purgeErrors.fetch_add(1, Ordering::Relaxed);
				if firstError.is_none()
				{
					firstError = Some(error);
				}
			}
			if state.retention.scan.is_none()
			{
				break;
			}
		}
		firstError.map_or(Ok(()), Err)
	}

	fn purgeEntry(
		&self,
		state: &mut FileState,
		name: &OsStr,
		protected: &[PathBuf],
		age: Option<(SystemTime, Duration)>,
	) -> io::Result<bool>
	{
		let component = Path::new(name);
		if component.extension() != Some(OsStr::new("trc"))
			|| component.file_name() != Some(name)
			|| !matches!(
				(component.components().next(), component.components().nth(1)),
				(Some(Component::Normal(_)), None)
			)
		{
			return Ok(false);
		}
		let root = &state
			.retention
			.anchor
			.as_ref()
			.ok_or_else(|| io::Error::other("trace purge root is not anchored"))?
			.0;
		// Use the same canonical namespace for enumeration, checks and deletion.
		// On Windows this also keeps extended-length semantics for DOS-like names.
		let path = root.join(component);
		if protected.contains(&path)
		{
			return Ok(false);
		}
		let original = fs::symlink_metadata(&path)?;
		if !original.is_file() || isLink(&original) || !expired(&original, age)?
		{
			return Ok(false);
		}
		if path.parent() != Some(root.as_path()) || fs::canonicalize(&path)? != path
		{
			return Err(io::Error::other(
				"trace purge candidate resolves outside its expected name",
			));
		}

		// Close all cached spellings of this destination while holding the writer lock.
		let mut index = 0;
		while index < state.files.len()
		{
			let cachedPath = &state.files[index].0;
			if *cachedPath == path
				|| fs::canonicalize(cachedPath).is_ok_and(|cached| cached == path)
			{
				if let Some((_, mut file)) = state.files.remove(index)
				{
					self._cachedHandles
						.store(state.files.len(), Ordering::Relaxed);
					self.flushWriter(&mut file)?;
				}
			}
			else
			{
				index += 1;
			}
		}
		let current = fs::symlink_metadata(&path)?;
		if !current.is_file()
			|| isLink(&current)
			|| !sameIdentity(&original, &current)
			|| original.len() != current.len()
			|| original.modified()? != current.modified()?
			|| !expired(&current, age)?
		{
			return Ok(false);
		}
		self.purgeCheckRoot(&state.retention)?;
		fs::remove_file(path)?;
		self._purgedFiles.fetch_add(1, Ordering::Relaxed);
		Ok(true)
	}
}

fn expired(metadata: &Metadata, age: Option<(SystemTime, Duration)>) -> io::Result<bool>
{
	let Some((now, maximumAge)) = age
	else
	{
		return Ok(true);
	};
	Ok(now
		.duration_since(metadata.modified()?)
		.is_ok_and(|elapsed| elapsed > maximumAge))
}

fn isLink(metadata: &Metadata) -> bool
{
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt;
		metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
	}
	#[cfg(not(windows))]
	{
		metadata.file_type().is_symlink()
	}
}

fn sameIdentity(original: &Metadata, current: &Metadata) -> bool
{
	#[cfg(unix)]
	{
		use std::os::unix::fs::MetadataExt;
		original.dev() == current.dev() && original.ino() == current.ino()
	}
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt;
		original.creation_time() == current.creation_time()
	}
	#[cfg(not(any(unix, windows)))]
	{
		original.created().ok() == current.created().ok()
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::modules::file_config::FileConfig;
	use crate::modules::module_abstract::ModuleAbstract;
	use std::fs::{File as TestFile, FileTimes};
	use std::sync::atomic::AtomicU64;

	static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

	struct TestDirectory(PathBuf);

	impl TestDirectory
	{
		fn new() -> Self
		{
			let path = std::env::temp_dir().join(format!(
				"htrace-retention-{}-{}",
				std::process::id(),
				NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
			));
			fs::create_dir(&path).unwrap();
			Self(path)
		}

		fn output(&self, days: Option<u32>, startup: bool) -> File
		{
			File::new(FileConfig {
				path: self.0.to_str().unwrap().to_owned(),
				purgeAfterDays: days,
				purgeOnStartup: startup,
				..FileConfig::default()
			})
		}

		fn put(&self, name: impl AsRef<Path>, modified: SystemTime) -> PathBuf
		{
			let path = self.0.join(name);
			fs::write(&path, b"existing\n").unwrap();
			setModified(&path, modified);
			path
		}
	}

	impl Drop for TestDirectory
	{
		fn drop(&mut self)
		{
			fs::remove_dir_all(&self.0).unwrap();
		}
	}

	fn setModified(path: &Path, modified: SystemTime)
	{
		TestFile::options()
			.write(true)
			.open(path)
			.unwrap()
			.set_times(FileTimes::new().set_modified(modified))
			.unwrap();
	}

	fn clock() -> SystemTime
	{
		// Exactly representable on filesystems with coarser timestamp precision.
		SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
	}

	fn run(output: &File, now: SystemTime, monotonic: Instant)
	{
		let mut state = output._files.lock();
		output
			.purgePeriodic(&mut state, &[], now, monotonic)
			.unwrap();
		while state.retention.scan.is_some()
		{
			output.purgeAdvance(&mut state, &[], now).unwrap();
		}
	}

	#[test]
	fn modification_age_is_strict_and_future_dates_are_preserved()
	{
		let directory = TestDirectory::new();
		let now = clock();
		let old = directory.put("old.trc", now - Duration::from_secs(86_401));
		let threshold = directory.put("exact.trc", now - Duration::from_secs(86_400));
		let recent = directory.put("recent.trc", now - Duration::from_secs(86_399));
		let future = directory.put("future.trc", now + Duration::from_secs(1));
		let output = directory.output(Some(1), false);
		run(&output, now, Instant::now());
		assert!(!old.exists());
		assert!(threshold.exists() && recent.exists() && future.exists());
		assert_eq!(output.stats().purgedFiles, 1);
		assert_eq!(output.stats().purgeErrors, 0);
		// u32::MAX days is representable without overflow and preserves these files.
		let output = directory.output(Some(u32::MAX), false);
		run(&output, now, Instant::now());
		assert_eq!(output.stats().purgedFiles, 0);
	}

	#[test]
	fn only_exact_extension_of_direct_regular_children_is_purged()
	{
		let directory = TestDirectory::new();
		let now = clock();
		for name in [
			"legacy.trc",
			"old%name.trc",
			"東京_invalid-date.trc",
			"with_under_score.trc",
		]
		{
			directory.put(name, now);
		}
		let upper = directory.put("other.TRC", now);
		let backup = directory.put("other.trc.bak", now);
		let noExtension = directory.put("trc", now);
		fs::create_dir(directory.0.join("nested.trc")).unwrap();
		let nested = directory.put("nested.trc/keep.trc", now);
		let external = TestDirectory::new();
		let sentinel = external.put("sentinel.trc", now);
		let output = directory.output(None, true);
		output.event_onStartup().unwrap();
		assert_eq!(output.stats().purgedFiles, 4);
		assert!(upper.exists() && backup.exists() && noExtension.exists() && nested.exists());
		assert_eq!(fs::read(sentinel).unwrap(), b"existing\n");
	}

	#[test]
	fn startup_finishes_all_batches_once_and_ignores_age()
	{
		let directory = TestDirectory::new();
		for index in 0..(REMOVE_BUDGET * 3 + 1)
		{
			directory.put(
				format!("{index}.trc"),
				clock() + Duration::from_secs(86_400),
			);
		}
		let output = directory.output(Some(1), true);
		output.event_onStartup().unwrap();
		assert_eq!(output.stats().purgedFiles, (REMOVE_BUDGET * 3 + 1) as u64);
		let fresh = directory.put("new.trc", clock());
		output.event_onStartup().unwrap();
		output.flush().unwrap();
		assert!(fresh.exists());
		assert_eq!(output.stats().purgeRuns, 1);
	}

	#[test]
	fn direct_write_consumes_startup_and_keeps_current_destinations()
	{
		let directory = TestDirectory::new();
		let old = directory.put("old.trc", clock() - Duration::from_secs(172_800));
		let output = directory.output(None, true);
		let current = directory.0.join("current.trc");
		output.writeToFiles(std::slice::from_ref(&current), b"new\n");
		output.event_onStartup().unwrap();
		assert_eq!(fs::read(current).unwrap(), b"new\n");
		assert!(old.exists());
		assert_eq!(output.stats().purgeRuns, 0);
	}

	#[test]
	fn disabled_options_and_inactive_destinations_never_scan_or_create_directories()
	{
		let directory = TestDirectory::new();
		for (days, startup, active) in [
			(None, false, true),
			(Some(0), false, true),
			(Some(1), true, false),
		]
		{
			let missing = directory.0.join(format!("missing-{days:?}-{active}"));
			let output = File::new(FileConfig {
				path: missing.to_str().unwrap().to_owned(),
				purgeAfterDays: days,
				purgeOnStartup: startup,
				byThreadId: active,
				..FileConfig::default()
			});
			output.event_onStartup().unwrap();
			output.flush().unwrap();
			run(&output, clock(), Instant::now());
			assert!(!missing.exists());
			assert_eq!(output.stats().purgeRuns, 0);
		}
		let keep = directory.put("keep.trc", clock() - Duration::from_secs(172_800));
		let output = File::new(FileConfig {
			path: directory.0.to_str().unwrap().to_owned(),
			byThreadId: false,
			purgeOnStartup: true,
			purgeAfterDays: Some(1),
			..FileConfig::default()
		});
		output.event_onStartup().unwrap();
		run(&output, clock(), Instant::now());
		assert!(keep.exists());
		assert_eq!(output.stats().purgeRuns, 0);
	}

	#[test]
	fn batches_are_bounded_resume_on_flush_and_obey_monotonic_cadence()
	{
		let directory = TestDirectory::new();
		let now = SystemTime::now();
		for index in 0..(REMOVE_BUDGET * 3)
		{
			directory.put(format!("{index}.trc"), now - Duration::from_secs(172_800));
		}
		let output = directory.output(Some(1), false);
		let monotonic = Instant::now();
		{
			let mut state = output._files.lock();
			output
				.purgePeriodic(&mut state, &[], now, monotonic)
				.unwrap();
			assert!(state.retention.scan.is_some());
		}
		assert_eq!(output.stats().purgedFiles, REMOVE_BUDGET as u64);
		output.flush().unwrap();
		assert_eq!(output.stats().purgedFiles, (REMOVE_BUDGET * 2) as u64);
		run(&output, now, monotonic);
		assert_eq!(output.stats().purgedFiles, (REMOVE_BUDGET * 3) as u64);
		let next = directory.put("next.trc", now - Duration::from_secs(172_800));
		run(
			&output,
			now + Duration::from_secs(172_800),
			monotonic + PURGE_INTERVAL - Duration::from_nanos(1),
		);
		assert!(next.exists());
		assert_eq!(output.stats().purgeRuns, 1);
		run(&output, now, monotonic + PURGE_INTERVAL);
		assert!(!next.exists());
		assert_eq!(output.stats().purgeRuns, 2);
	}

	#[test]
	fn entry_budget_also_bounds_scanning_when_nothing_can_be_removed()
	{
		let directory = TestDirectory::new();
		for index in 0..(ENTRY_BUDGET + 1)
		{
			directory.put(format!("{index}.txt"), clock());
		}
		let output = directory.output(Some(1), false);
		let mut state = output._files.lock();
		output
			.purgePeriodic(&mut state, &[], clock(), Instant::now())
			.unwrap();
		assert!(state.retention.scan.is_some());
		assert_eq!(output.stats().purgedFiles, 0);
		output.purgeAdvance(&mut state, &[], clock()).unwrap();
		assert!(state.retention.scan.is_none());
	}

	#[test]
	fn expired_cached_handles_are_closed_and_late_append_recreates_destination()
	{
		let directory = TestDirectory::new();
		let output = directory.output(Some(1), false);
		let path = directory.0.join("old.trc");
		output.writeToFiles(std::slice::from_ref(&path), b"first\n");
		setModified(&path, clock() - Duration::from_secs(172_800));
		// Reset only the private test clock; the first actual write already ran a pass.
		let later = output._files.lock().retention.lastRun.unwrap() + PURGE_INTERVAL;
		run(&output, clock(), later);
		assert!(!path.exists());
		assert_eq!(output.stats().cachedHandles, 0);
		output.writeToFiles(std::slice::from_ref(&path), b"late\n");
		output.flush().unwrap();
		assert_eq!(fs::read(&path).unwrap(), b"late\n");
		assert_eq!(output.stats().cachedHandles, 1);
		assert_eq!(output.stats().linesWritten, 2);
	}

	#[test]
	fn recent_append_and_protected_paths_survive_expired_cache_scan()
	{
		let directory = TestDirectory::new();
		let now = SystemTime::now();
		let path = directory.put("old-name.trc", now - Duration::from_secs(172_800));
		let output = directory.output(Some(1), false);
		output.writeToFiles(std::slice::from_ref(&path), b"append\n");
		assert_eq!(fs::read(&path).unwrap(), b"existing\nappend\n");
		assert_eq!(output.stats().purgedFiles, 0);
		setModified(&path, now - Duration::from_secs(172_800));
		let mut state = output._files.lock();
		output.purgeBegin(&mut state).unwrap();
		output
			.purgeAdvance(&mut state, std::slice::from_ref(&path), now)
			.unwrap();
		assert!(path.exists());
		assert_eq!(output.stats().cachedHandles, 1);
	}

	#[test]
	fn startup_failure_is_diagnostic_and_is_never_retried()
	{
		let directory = TestDirectory::new();
		let root = directory.put("root", clock());
		let output = File::new(FileConfig {
			path: root.to_str().unwrap().to_owned(),
			purgeOnStartup: true,
			..FileConfig::default()
		});
		assert!(output.event_onStartup().is_err());
		assert_eq!(output.stats().purgeErrors, 1);
		fs::remove_file(&root).unwrap();
		fs::create_dir(&root).unwrap();
		let keep = root.join("keep.trc");
		fs::write(&keep, b"keep").unwrap();
		output.event_onStartup().unwrap();
		output.writeToFiles(&[root.join("new.trc")], b"new");
		assert_eq!(fs::read(keep).unwrap(), b"keep");
		assert_eq!(output.stats().purgeRuns, 1);
	}

	#[test]
	fn periodic_failure_retries_only_after_interval_and_can_recover()
	{
		let directory = TestDirectory::new();
		let root = directory.put("root", clock());
		let output = File::new(FileConfig {
			path: root.to_str().unwrap().to_owned(),
			purgeAfterDays: Some(1),
			..FileConfig::default()
		});
		let monotonic = Instant::now();
		let mut state = output._files.lock();
		assert!(
			output
				.purgePeriodic(&mut state, &[], clock(), monotonic)
				.is_err()
		);
		fs::remove_file(&root).unwrap();
		fs::create_dir(&root).unwrap();
		let old = root.join("old.trc");
		fs::write(&old, b"old").unwrap();
		setModified(&old, clock() - Duration::from_secs(172_800));
		output
			.purgePeriodic(&mut state, &[], clock(), monotonic)
			.unwrap();
		assert!(old.exists());
		output
			.purgePeriodic(&mut state, &[], clock(), monotonic + PURGE_INTERVAL)
			.unwrap();
		assert!(!old.exists());
		assert_eq!(output.stats().purgeRuns, 2);
		assert_eq!(output.stats().purgeErrors, 1);
	}

	#[test]
	fn external_cached_destination_with_same_basename_is_untouched()
	{
		let directory = TestDirectory::new();
		let external = TestDirectory::new();
		let path = external.put("same.trc", clock() - Duration::from_secs(172_800));
		let internal = directory.put("same.trc", clock() - Duration::from_secs(172_800));
		let output = directory.output(Some(1), false);
		output.writeToFiles(std::slice::from_ref(&path), b"external\n");
		let later = output._files.lock().retention.lastRun.unwrap() + PURGE_INTERVAL;
		run(&output, clock(), later);
		assert!(!internal.exists());
		assert_eq!(fs::read(path).unwrap(), b"existing\nexternal\n");
		assert_eq!(output.stats().cachedHandles, 1);
	}

	#[cfg(unix)]
	#[test]
	fn non_utf8_names_are_eligible_and_symlink_children_and_roots_are_rejected()
	{
		use std::os::unix::ffi::OsStrExt;
		use std::os::unix::fs::symlink;
		let directory = TestDirectory::new();
		let external = TestDirectory::new();
		let sentinel = external.put("sentinel.trc", clock());
		let nonUtf8 = directory.put(OsStr::from_bytes(b"\xff.trc"), clock());
		symlink(&sentinel, directory.0.join("link.trc")).unwrap();
		symlink(&external.0, directory.0.join("linked-directory.trc")).unwrap();
		let output = directory.output(None, true);
		output.event_onStartup().unwrap();
		assert!(!nonUtf8.exists());
		assert!(
			fs::symlink_metadata(directory.0.join("link.trc"))
				.unwrap()
				.file_type()
				.is_symlink()
		);
		assert_eq!(fs::read(&sentinel).unwrap(), b"existing\n");
		let linkedRoot = directory.0.join("root");
		symlink(&external.0, &linkedRoot).unwrap();
		for suffix in ["", "/", "/."]
		{
			let output = File::new(FileConfig {
				path: format!("{}{suffix}", linkedRoot.display()),
				purgeOnStartup: true,
				..FileConfig::default()
			});
			assert!(output.event_onStartup().is_err());
			assert_eq!(output.stats().purgeErrors, 1);
			assert_eq!(fs::read(&sentinel).unwrap(), b"existing\n");
		}
	}

	#[test]
	fn concurrent_direct_appends_and_purge_share_one_cache_lock()
	{
		let directory = TestDirectory::new();
		let output = directory.output(Some(1), false);
		let old = directory.put("old.trc", SystemTime::now() - Duration::from_secs(172_800));
		let destination = directory.0.join("current.trc");
		std::thread::scope(|scope| {
			for _ in 0..4
			{
				let output = &output;
				let destination = &destination;
				scope.spawn(move || {
					for _ in 0..32
					{
						output.writeToFiles(std::slice::from_ref(destination), b"whole line\n");
						output.flush().unwrap();
					}
				});
			}
		});
		assert!(!old.exists());
		assert_eq!(fs::read(&destination).unwrap(), b"whole line\n".repeat(128));
		assert_eq!(output.stats().linesWritten, 128);
		assert_eq!(output.stats().purgedFiles, 1);
		assert_eq!(output.stats().purgeErrors, 0);
	}

	#[test]
	fn changed_root_is_detected_before_deleting_any_candidate()
	{
		let directory = TestDirectory::new();
		let root = directory.0.join("root");
		fs::create_dir(&root).unwrap();
		let output = File::new(FileConfig {
			path: root.to_str().unwrap().to_owned(),
			purgeAfterDays: Some(1),
			..FileConfig::default()
		});
		let mut state = output._files.lock();
		output.purgeBegin(&mut state).unwrap();
		// Deterministic replacement between anchoring and a candidate check.
		// Release the enumeration handle first for platforms that lock directories.
		state.retention.scan = None;
		fs::rename(&root, directory.0.join("previous")).unwrap();
		fs::create_dir(&root).unwrap();
		let sentinel = root.join("keep.trc");
		fs::write(&sentinel, b"keep").unwrap();
		setModified(&sentinel, clock() - Duration::from_secs(172_800));
		assert!(output.purgeBegin(&mut state).is_err());
		assert_eq!(fs::read(sentinel).unwrap(), b"keep");
		assert_eq!(output.stats().purgedFiles, 0);
		assert_eq!(output.stats().purgeErrors, 1);
	}
}
