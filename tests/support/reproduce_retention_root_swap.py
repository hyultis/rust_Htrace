#!/usr/bin/env python3
"""Demonstrate the pathname race in an instrumented temporary source copy (Unix).

A passing injected test means the counterexample was reproduced, NOT that purge
is race-safe. No instrumentation is added to the actual library. All filesystem
fixtures, build outputs and copied sources are isolated in a temporary directory.
"""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

REPRODUCTION = r"""	#[test]
	fn deterministic_root_swap_after_final_check_unlinks_external_sentinel()
	{
		let directory = TestDirectory::new();
		let now = clock();
		directory.put("victim.trc", now);
		let external = TestDirectory::new();
		let sentinel = external.put("victim.trc", now);
		let root = directory.0.clone();
		let moved = directory.0.with_file_name(format!("{}-moved", directory.0.file_name().unwrap().to_string_lossy()));
		let outside = external.0.clone();
		let moved_victim = moved.join("victim.trc");
		PURGE_RACE_HOOK.with(|hook| {
			*hook.borrow_mut() = Some(Box::new(move || {
				fs::rename(&root, &moved).unwrap();
				#[cfg(unix)] std::os::unix::fs::symlink(&outside, &root).unwrap();
			}));
		});
		let output = directory.output(None, true);
		output.event_onStartup().unwrap();
		assert!(!sentinel.exists(), "external fixture sentinel was not unlinked");
		assert!(moved_victim.exists(), "original trace-root victim should remain");
		assert_eq!(output.stats().purgedFiles, 1);
		assert_eq!(output.stats().purgeErrors, 0);
	}
"""


def main():
    if os.name != "posix":
        raise SystemExit("This deterministic reproduction requires Unix symlinks.")
    repository = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory(prefix="htrace-root-swap-audit-") as scratch:
        copied = Path(scratch)
        shutil.copytree(repository / "src", copied / "src")
        for name in ("Cargo.toml", "Cargo.lock", "README.md"):
            shutil.copy2(repository / name, copied / name)
        source = copied / "src/modules/file/retention.rs"
        contents = source.read_text()
        boundary = "\t\tself.purgeCheckRoot(&state.retention)?;\n\t\tfs::remove_file(path)?;"
        hook = """
#[cfg(test)]
thread_local! {
    static PURGE_RACE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}
"""
        injected = """		self.purgeCheckRoot(&state.retention)?;
        #[cfg(test)]
        PURGE_RACE_HOOK.with(|hook| {
            if let Some(hook) = hook.borrow_mut().take() { hook(); }
        });
        fs::remove_file(path)?;"""
        test_marker = "\t#[test]\n\tfn modification_age_is_strict_and_future_dates_are_preserved()"
        if contents.count(boundary) != 1 or contents.count(test_marker) != 1:
            raise SystemExit("Production layout changed; review injection points before rerunning.")
        contents = hook + contents.replace(boundary, injected)
        contents = contents.replace(test_marker, REPRODUCTION + "\n\n" + test_marker)
        source.write_text(contents)
        fixtures = copied / "fixtures"
        fixtures.mkdir()
        environment = os.environ.copy()
        environment["TMPDIR"] = str(fixtures)
        environment["CARGO_TARGET_DIR"] = str(copied / "target")
        subprocess.run([
            "cargo", "test", "--offline", "--locked", "--lib",
            "--manifest-path", str(copied / "Cargo.toml"),
            "deterministic_root_swap_after_final_check_unlinks_external_sentinel",
            "--", "--nocapture",
        ], env=environment, check=True)
        print("Counterexample reproduced: a root swap after the final check unlinks an external fixture sentinel.")


if __name__ == "__main__":
    main()
