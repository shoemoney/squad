#!/usr/bin/env python3
"""Render the formula from the release version and downloaded build archives."""
import hashlib
import os
from pathlib import Path
import re
import tomllib

root = Path(__file__).resolve().parents[2]
version = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
values = {"VERSION": version, "TAG": os.environ.get("GITHUB_REF_NAME", f"v{version}")}
for target, placeholder in (
    ("aarch64-apple-darwin", "SHA256_MACOS_ARM64"),
    ("x86_64-unknown-linux-gnu", "SHA256_LINUX_X64"),
):
    archive = root / "target" / "distrib" / f"squad-cli-{target}.tar.xz"
    # Missing archives fail the global build; never ship guessed checksums.
    with archive.open("rb") as stream:
        values[placeholder] = hashlib.file_digest(stream, "sha256").hexdigest()
formula = (root / "dist/homebrew/squad.rb").read_text()
for placeholder, value in values.items():
    formula = formula.replace(f"@{placeholder}@", value)
if re.search(r"@[A-Z0-9_]+@", formula):
    raise SystemExit("unfilled Homebrew formula placeholder")
output = root / "target" / "homebrew" / "squad.rb"
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(formula)
print(output)
