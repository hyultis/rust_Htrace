use super::*;
use crate::components::{context::Context, level::Level, trace::OneTrace};
use crate::modules::{file_config::FileConfig, module_abstract::ModuleAbstract};
use std::ffi::OsString;
use std::sync::atomic::AtomicU64;

const SENTINEL: &[u8] = b"outside sentinel\n";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture
{
	fn new() -> Self
	{
		let path = std::env::temp_dir().join(format!(
			"htrace-retention-audit-{}-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(SystemTime::UNIX_EPOCH)
				.unwrap()
				.as_nanos(),
			NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&path).unwrap();
		fs::create_dir(path.join("root")).unwrap();
		fs::create_dir(path.join("root-sibling")).unwrap();
		let fixture = Self(path);
		fixture.put("sentinel.trc");
		fixture.put("root-sibling/sentinel.trc");
		fixture
	}

	fn root(&self) -> PathBuf
	{
		self.0.join("root")
	}

	fn put(&self, name: impl AsRef<Path>) -> PathBuf
	{
		let path = self.0.join(name);
		fs::write(&path, SENTINEL).unwrap();
		path
	}

	fn output(&self) -> File
	{
		File::new(FileConfig {
			path: self.root().to_str().unwrap().to_owned(),
			purgeOnStartup: true,
			..FileConfig::default()
		})
	}

	fn assertOutside(&self)
	{
		for name in ["sentinel.trc", "root-sibling/sentinel.trc"]
		{
			assert_eq!(fs::read(self.0.join(name)).unwrap(), SENTINEL, "{name}");
		}
	}
}

impl Drop for Fixture
{
	fn drop(&mut self)
	{
		fs::remove_dir_all(&self.0).unwrap();
	}
}

fn trace() -> OneTrace
{
	OneTrace {
		message: "audit append".to_owned(),
		date: time::macros::datetime!(2026-01-02 12:00 UTC),
		level: Level::NORMAL,
		context: Context::default(),
		filename: "audit.rs".to_owned(),
		fileline: 1,
		backtraces: Vec::new(),
	}
}

#[test]
fn direct_entry_calls_reject_absolute_parent_and_multiple_components()
{
	let fixture = Fixture::new();
	let eligible = fixture.put("root/eligible.trc");
	fs::create_dir(fixture.root().join("nested")).unwrap();
	let nested = fixture.put("root/nested/eligible.trc");
	let output = fixture.output();
	let mut state = output._files.lock();
	output.purgeBegin(&mut state).unwrap();
	let mut names = vec![
		fixture.0.join("root-sibling/sentinel.trc").into_os_string(),
		fixture.root().join("eligible.trc").into_os_string(),
		OsString::from("../sentinel.trc"),
		OsString::from("../root-sibling/sentinel.trc"),
		OsString::from("nested/eligible.trc"),
		OsString::from("nested/../eligible.trc"),
		OsString::from("./eligible.trc"),
		OsString::from("eligible.trc/"),
		OsString::from("eligible.trc/."),
	];
	#[cfg(windows)]
	names.extend([
		OsString::from(r"..\root-sibling\sentinel.trc"),
		OsString::from(r"nested\eligible.trc"),
		OsString::from(r"nested\..\eligible.trc"),
		OsString::from("eligible.trc\\"),
		OsString::from("eligible.trc\\."),
	]);
	for name in names.drain(..)
	{
		assert!(
			!output.purgeEntry(&mut state, &name, &[], None).unwrap(),
			"{name:?}"
		);
		assert_eq!(fs::read(&eligible).unwrap(), SENTINEL);
		assert_eq!(fs::read(&nested).unwrap(), SENTINEL);
		fixture.assertOutside();
	}
	assert!(
		output
			.purgeEntry(&mut state, OsStr::new("eligible.trc"), &[], None)
			.unwrap()
	);
	assert!(!eligible.exists());
	assert_eq!(output.stats().purgedFiles, 1);
}

#[test]
fn filesystem_root_is_refused_without_enumeration_or_deletion()
{
	let fixture = Fixture::new();
	let canonical = fs::canonicalize(fixture.root()).unwrap();
	let root = canonical.ancestors().last().unwrap();
	let output = File::new(FileConfig {
		path: root.to_str().unwrap().to_owned(),
		..FileConfig::default()
	});
	// Only the read-only root validator is called: never run a purge on a real root.
	assert!(output.purgeRoot().is_err());
	assert_eq!(output.stats().purgeRuns, 0);
	assert_eq!(output.stats().purgedFiles, 0);
	fixture.assertOutside();
}

#[test]
fn similarly_prefixed_sibling_and_nested_logs_are_outside_the_scan()
{
	let fixture = Fixture::new();
	let eligible = fixture.put("root/sentinel.trc");
	fs::create_dir(fixture.root().join("nested.trc")).unwrap();
	let nested = fixture.put("root/nested.trc/sentinel.trc");
	let output = fixture.output();
	output.event_onStartup().unwrap();
	assert!(!eligible.exists());
	assert_eq!(fs::read(nested).unwrap(), SENTINEL);
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 1);
	assert_eq!(output.stats().purgeErrors, 0);
}

#[test]
fn native_extensions_are_exact_and_percent_encoded_separators_are_literal()
{
	let fixture = Fixture::new();
	let eligible = [
		"東京.trc",
		"é.trc",
		"%2e%2e%2froot-sibling%2fsentinel.trc",
		"..%5croot-sibling%5csentinel.trc",
		"nested%2Fsentinel.trc",
	];
	let retained = [
		"upper.TRC",
		"backup.trc.bak",
		"wide.ｔｒｃ",
		"zero.trc\u{200b}",
		".trc",
	];
	for name in eligible.iter().chain(retained.iter())
	{
		fixture.put(Path::new("root").join(name));
	}
	let output = fixture.output();
	output.event_onStartup().unwrap();
	for name in eligible
	{
		assert!(!fixture.root().join(name).exists(), "{name}");
	}
	for name in retained
	{
		assert_eq!(
			fs::read(fixture.root().join(name)).unwrap(),
			SENTINEL,
			"{name}"
		);
	}
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, eligible.len() as u64);
}

#[cfg(unix)]
#[test]
fn non_utf8_native_names_preserve_exact_extension_rules()
{
	use std::os::unix::ffi::OsStrExt;
	let fixture = Fixture::new();
	let eligible = fixture.put(Path::new("root").join(OsStr::from_bytes(b"\xff.trc")));
	let retained = fixture.put(Path::new("root").join(OsStr::from_bytes(b"native.trc\xff")));
	let output = fixture.output();
	output.event_onStartup().unwrap();
	assert!(!eligible.exists());
	assert_eq!(fs::read(retained).unwrap(), SENTINEL);
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 1);
}

#[cfg(windows)]
#[test]
fn windows_native_names_preserve_unpaired_surrogates_and_exact_extensions()
{
	use std::os::windows::ffi::OsStringExt;
	let fixture = Fixture::new();
	let eligible = OsString::from_wide(&[0xd800, 0x2e, 0x74, 0x72, 0x63]);
	let retained = OsString::from_wide(&[0x78, 0x2e, 0x74, 0x72, 0x63, 0xd800]);
	let eligible = fixture.put(Path::new("root").join(eligible));
	let retained = fixture.put(Path::new("root").join(retained));
	let output = fixture.output();
	output.event_onStartup().unwrap();
	assert!(!eligible.exists());
	assert_eq!(fs::read(retained).unwrap(), SENTINEL);
	fixture.assertOutside();
}

#[cfg(windows)]
#[test]
fn windows_ambiguous_trailing_space_root_preserves_both_directories()
{
	use std::path::Prefix;
	let fixture = Fixture::new();
	// The configuration must use Win32 spelling; creation uses verbatim spelling
	// so the two distinct fixture directories can coexist without trimming.
	if !matches!(
		fixture.0.components().next(),
		Some(Component::Prefix(prefix))
			if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _))
	)
	{
		eprintln!("skipping ambiguous Windows root: fixture has no non-verbatim absolute spelling");
		return;
	}
	let extended = fs::canonicalize(&fixture.0).unwrap();
	let ordinary = extended.join("logs");
	let spaced = extended.join("logs ");
	fs::create_dir(&ordinary).unwrap();
	if let Err(error) = fs::create_dir(&spaced)
	{
		eprintln!(
			"skipping ambiguous Windows root: trailing-space directory creation is unsupported: {error}"
		);
		fixture.assertOutside();
		return;
	}
	assert_ne!(
		fs::canonicalize(&ordinary).unwrap(),
		fs::canonicalize(&spaced).unwrap()
	);
	let ordinaryLog = ordinary.join("sentinel.trc");
	let spacedLog = spaced.join("sentinel.trc");
	fs::write(&ordinaryLog, b"ordinary sentinel\n").unwrap();
	fs::write(&spacedLog, b"trailing-space sentinel\n").unwrap();
	let mut configured = fixture.0.join("logs ").into_os_string();
	configured.push("\\");
	let output = File::new(FileConfig {
		path: configured.into_string().unwrap(),
		purgeOnStartup: true,
		..FileConfig::default()
	});
	assert!(output.event_onStartup().is_err());
	assert_eq!(fs::read(ordinaryLog).unwrap(), b"ordinary sentinel\n");
	assert_eq!(fs::read(spacedLog).unwrap(), b"trailing-space sentinel\n");
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 0);
	assert_eq!(output.stats().purgeErrors, 1);
	fs::remove_dir_all(spaced).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_literal_reserved_basename_is_deleted_exactly_or_retained()
{
	let fixture = Fixture::new();
	let extendedRoot = fs::canonicalize(fixture.root()).unwrap();
	// Use only a real verbatim child path. No DOS device aliases are installed
	// and no non-verbatim spelling of this reserved basename is opened.
	let literal = extendedRoot.join("COM9.trc");
	if let Err(error) = fs::write(&literal, SENTINEL)
	{
		eprintln!(
			"skipping literal reserved Windows basename: verbatim creation is unsupported: {error}"
		);
		fixture.assertOutside();
		return;
	}
	assert!(
		fs::read_dir(&extendedRoot)
			.unwrap()
			.any(|entry| { entry.unwrap().file_name() == OsStr::new("COM9.trc") })
	);
	assert!(fs::symlink_metadata(&literal).unwrap().is_file());
	assert_eq!(fs::read(&literal).unwrap(), SENTINEL);
	let ordinary = fixture.put("root/ordinary.trc");
	let output = fixture.output();
	let result = output.event_onStartup();
	assert!(!ordinary.exists());
	match fs::symlink_metadata(&literal)
	{
		Err(error) if error.kind() == io::ErrorKind::NotFound =>
		{
			result.unwrap();
			assert_eq!(output.stats().purgedFiles, 2);
		}
		Ok(metadata) =>
		{
			assert!(metadata.is_file());
			assert_eq!(fs::read(&literal).unwrap(), SENTINEL);
			assert_eq!(output.stats().purgedFiles, 1);
			eprintln!("literal reserved Windows child was safely retained: {result:?}");
			fs::remove_file(&literal).unwrap();
		}
		Err(error) => panic!("could not inspect literal reserved child: {error}"),
	}
	fixture.assertOutside();
}

#[cfg(unix)]
#[test]
fn symlink_children_to_external_files_and_directories_are_retained()
{
	use std::os::unix::fs::symlink;
	let fixture = Fixture::new();
	let fileLink = fixture.root().join("file.trc");
	let directoryLink = fixture.root().join("directory.trc");
	symlink(fixture.0.join("root-sibling/sentinel.trc"), &fileLink).unwrap();
	symlink(fixture.0.join("root-sibling"), &directoryLink).unwrap();
	fixture.put("root/eligible.trc");
	let output = fixture.output();
	output.event_onStartup().unwrap();
	for path in [fileLink, directoryLink]
	{
		assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
	}
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 1);
	assert_eq!(output.stats().purgeErrors, 0);
}

#[test]
fn unlinking_an_eligible_hard_link_preserves_its_external_alias_and_content()
{
	let fixture = Fixture::new();
	let external = fixture.0.join("root-sibling/sentinel.trc");
	let internal = fixture.root().join("hard-link.trc");
	fs::hard_link(&external, &internal).unwrap();
	let output = fixture.output();
	output.event_onStartup().unwrap();
	assert!(!internal.exists());
	assert_eq!(fs::read(external).unwrap(), SENTINEL);
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 1);
}

#[cfg(unix)]
#[test]
fn root_symlinks_with_trailing_separator_and_dot_are_rejected()
{
	use std::os::unix::fs::symlink;
	let fixture = Fixture::new();
	let link = fixture.0.join("linked-root");
	symlink(fixture.0.join("root-sibling"), &link).unwrap();
	for suffix in ["", "/", "/.", "//", "/./"]
	{
		let output = File::new(FileConfig {
			path: format!("{}{suffix}", link.display()),
			purgeOnStartup: true,
			..FileConfig::default()
		});
		assert!(output.event_onStartup().is_err(), "{suffix:?}");
		assert_eq!(output.stats().purgedFiles, 0);
		assert_eq!(output.stats().purgeErrors, 1);
		fixture.assertOutside();
	}
}

#[test]
fn force_in_one_file_parent_destination_is_written_but_never_purged()
{
	let fixture = Fixture::new();
	let historical = fixture.put("root-sibling/external_20000101.trc");
	let destination = fixture.put("root-sibling/external_20260102.trc");
	let inside = fixture.put("root/old.trc");
	let output = File::new(FileConfig {
		path: fixture.root().to_str().unwrap().to_owned(),
		forceInOneFile: Some("../root-sibling/external".to_owned()),
		byThreadId: false,
		lineFormat: "{msg}".to_owned(),
		purgeAfterDays: Some(1),
		purgeOnStartup: true,
		..FileConfig::default()
	});
	output.event_onStartup().unwrap();
	assert!(!inside.exists());
	assert_eq!(fs::read(&historical).unwrap(), SENTINEL);
	assert_eq!(fs::read(&destination).unwrap(), SENTINEL);
	output.event_onNormal(&trace());
	output.flush().unwrap();
	assert_eq!(
		fs::read(&destination).unwrap(),
		[SENTINEL, b"audit append\n"].concat()
	);
	// A later age-independent pass must still stay inside the configured root.
	let mut state = output._files.lock();
	output.purgeBegin(&mut state).unwrap();
	while state.retention.scan.is_some()
	{
		output.purgeBatch(&mut state, &[], None).unwrap();
	}
	assert_eq!(fs::read(historical).unwrap(), SENTINEL);
	assert_eq!(
		fs::read(destination).unwrap(),
		[SENTINEL, b"audit append\n"].concat()
	);
	fixture.assertOutside();
	assert_eq!(output.stats().linesWritten, 1);
	assert_eq!(output.stats().purgedFiles, 1);
	assert_eq!(output.stats().purgeErrors, 0);
}

#[test]
fn stale_enumerated_name_cannot_delete_a_replacement_root_candidate()
{
	let fixture = Fixture::new();
	fixture.put("root/candidate.trc");
	let output = fixture.output();
	let mut state = output._files.lock();
	output.purgeBegin(&mut state).unwrap();
	let name = state
		.retention
		.scan
		.as_mut()
		.unwrap()
		.next()
		.unwrap()
		.unwrap()
		.file_name();
	// Close the enumeration before renaming on platforms that lock directories.
	state.retention.scan = None;
	fs::rename(fixture.root(), fixture.0.join("previous-root")).unwrap();
	fs::create_dir(fixture.root()).unwrap();
	let replacement = fixture.put(Path::new("root").join(&name));
	// Directly exercise the final root check with a stale enumerated component.
	assert!(output.purgeEntry(&mut state, &name, &[], None).is_err());
	assert_eq!(fs::read(replacement).unwrap(), SENTINEL);
	assert_eq!(
		fs::read(fixture.0.join("previous-root").join(name)).unwrap(),
		SENTINEL
	);
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 0);
}

#[cfg(unix)]
#[test]
fn stale_enumeration_rejects_redirected_ancestor_even_with_same_root_inode()
{
	use std::os::unix::fs::symlink;
	let fixture = Fixture::new();
	let ancestor = fixture.0.join("ancestor");
	fs::create_dir(&ancestor).unwrap();
	fs::create_dir(ancestor.join("root")).unwrap();
	fs::write(ancestor.join("root/candidate.trc"), SENTINEL).unwrap();
	let output = File::new(FileConfig {
		path: ancestor.join("root").to_str().unwrap().to_owned(),
		..FileConfig::default()
	});
	let mut state = output._files.lock();
	output.purgeBegin(&mut state).unwrap();
	let name = state
		.retention
		.scan
		.as_mut()
		.unwrap()
		.next()
		.unwrap()
		.unwrap()
		.file_name();
	state.retention.scan = None;
	let moved = fixture.0.join("moved-ancestor");
	fs::rename(&ancestor, &moved).unwrap();
	symlink(&moved, &ancestor).unwrap();
	let (_, original) = state.retention.anchor.as_ref().unwrap();
	assert!(sameIdentity(
		original,
		&fs::symlink_metadata(ancestor.join("root")).unwrap()
	));
	assert!(output.purgeEntry(&mut state, &name, &[], None).is_err());
	assert_eq!(fs::read(moved.join("root").join(name)).unwrap(), SENTINEL);
	fixture.assertOutside();
	assert_eq!(output.stats().purgedFiles, 0);
}

#[test]
fn relative_configuration_is_frozen_in_a_child_process()
{
	const CHILD_FIXTURE: &str = "HTRACE_RETENTION_AUDIT_CWD_FIXTURE";
	if let Some(base) = std::env::var_os(CHILD_FIXTURE)
	{
		let base = PathBuf::from(base);
		let original = base.join("root/logs");
		let changed = base.join("root-sibling/logs");
		assert_eq!(std::env::current_dir().unwrap(), base.join("root"));
		let output = File::new(FileConfig {
			path: "logs".to_owned(),
			forceInOneFile: Some("current".to_owned()),
			byThreadId: false,
			lineFormat: "{msg}".to_owned(),
			purgeOnStartup: true,
			..FileConfig::default()
		});
		std::env::set_current_dir(base.join("root-sibling")).unwrap();
		output.event_onStartup().unwrap();
		output.event_onNormal(&trace());
		output.flush().unwrap();
		assert!(!original.join("old.trc").exists());
		assert_eq!(
			fs::read(original.join("current_20260102.trc")).unwrap(),
			b"audit append\n"
		);
		assert_eq!(fs::read(changed.join("old.trc")).unwrap(), SENTINEL);
		assert!(!changed.join("current_20260102.trc").exists());
		assert_eq!(output.stats().purgedFiles, 1);
		std::env::set_current_dir(&base).unwrap();
		return;
	}
	let fixture = Fixture::new();
	for parent in ["root", "root-sibling"]
	{
		fs::create_dir(fixture.0.join(parent).join("logs")).unwrap();
		fixture.put(Path::new(parent).join("logs/old.trc"));
	}
	let result = std::process::Command::new(std::env::current_exe().unwrap())
		.args([
			"--exact",
			"modules::file::retention::audit_tests::relative_configuration_is_frozen_in_a_child_process",
			"--nocapture",
		])
		.env(CHILD_FIXTURE, &fixture.0)
		.current_dir(fixture.root())
		.output()
		.unwrap();
	assert!(
		result.status.success(),
		"child failed: {}\n{}",
		String::from_utf8_lossy(&result.stdout),
		String::from_utf8_lossy(&result.stderr)
	);
	assert!(!fixture.root().join("logs/old.trc").exists());
	assert_eq!(
		fs::read(fixture.0.join("root-sibling/logs/old.trc")).unwrap(),
		SENTINEL
	);
	fixture.assertOutside();
}
