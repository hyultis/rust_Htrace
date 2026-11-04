"""Regression checks for the manual publishing guard; no registry access."""

from pathlib import Path
import tempfile
import unittest

from check_release import validate


class ReleaseGuardTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "Cargo.toml").write_text('[package]\nname = "Htrace"\nversion = "3.0.0"\n')
        (self.root / "CHANGELOG.md").write_text('## [3.0.0] — Unreleased\n')

    def test_dry_run_accepts_unreleased(self):
        self.assertEqual(validate(self.root), "3.0.0")

    def test_publish_requires_exact_version(self):
        for version in ("", "2.0.0", "3.0.0; echo injected"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                validate(self.root, True, version)

    def test_publish_rejects_undated_or_invalid_date(self):
        for heading in ("Unreleased", "2026-02-30", "2026-11-04 extra"):
            (self.root / "CHANGELOG.md").write_text(f'## [3.0.0] — {heading}\n')
            with self.subTest(heading=heading), self.assertRaises(ValueError):
                validate(self.root, True, "3.0.0")

    def test_publish_accepts_matching_dated_release(self):
        (self.root / "CHANGELOG.md").write_text('## [3.0.0] — 2026-11-04\n')
        self.assertEqual(validate(self.root, True, "3.0.0"), "3.0.0")


if __name__ == "__main__":
    unittest.main()
