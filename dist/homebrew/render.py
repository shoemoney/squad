#!/usr/bin/env python3
"""Render the formula from the release version and downloaded build archives."""
import hashlib
import os
from pathlib import Path
import re

root = Path(__file__).resolve().parents[2]
# Release runners ship Python 3.10 (no tomllib), so read the version without it.
manifest = (root / "Cargo.toml").read_text()
section = re.search(r"^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)", manifest, re.M | re.S)
match = section and re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
if not match:
    raise SystemExit("workspace.package.version not found in Cargo.toml")
version = match.group(1)
values = {"VERSION": version, "TAG": os.environ.get("GITHUB_REF_NAME", f"v{version}")}
for target, placeholder in (
    ("aarch64-apple-darwin", "SHA256_MACOS_ARM64"),
    ("x86_64-unknown-linux-gnu", "SHA256_LINUX_X64"),
):
    archive = root / "target" / "distrib" / f"squad-cli-{target}.tar.xz"
    # Missing archives fail the global build; never ship guessed checksums.
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    values[placeholder] = digest.hexdigest()
formula = (root / "dist/homebrew/squad.rb").read_text()
for placeholder, value in values.items():
    formula = formula.replace(f"@{placeholder}@", value)
if re.search(r"@[A-Z0-9_]+@", formula):
    raise SystemExit("unfilled Homebrew formula placeholder")
output = root / "target" / "homebrew" / "squad.rb"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(formula)
print(output)
