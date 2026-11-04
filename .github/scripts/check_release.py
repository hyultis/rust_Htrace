#!/usr/bin/env python3
"""Check release metadata without publishing or changing files."""

import datetime
import os
from pathlib import Path
import re
import tomllib


def validate(root, publish=False, expected_version=""):
    package = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    version = package["version"]
    if package["name"] != "Htrace":
        raise ValueError("unexpected package name")
    if expected_version and expected_version != version:
        raise ValueError("requested version does not match Cargo.toml")
    if publish and not expected_version:
        raise ValueError("publishing requires the exact version as an explicit input")
    if publish:
        pattern = rf"^## \[{re.escape(version)}\] — (\d{{4}}-\d{{2}}-\d{{2}})$"
        match = re.search(pattern, (root / "CHANGELOG.md").read_text(), re.MULTILINE)
        if not match:
            raise ValueError("date the version's changelog entry before publishing")
        datetime.date.fromisoformat(match[1])
    return version


if __name__ == "__main__":
    try:
        version = validate(
            Path(__file__).resolve().parents[2],
            publish=os.environ.get("PUBLISH_RELEASE") == "true",
            expected_version=os.environ.get("EXPECTED_VERSION", ""),
        )
    except (KeyError, ValueError, OSError) as error:
        raise SystemExit(f"release check failed: {error}") from error
    print(f"Release metadata checked: Htrace {version}")
