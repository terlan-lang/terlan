#!/usr/bin/env python3
"""Verify shipped Terlan Rust entrypoints reject unsafe Rust."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CARGO_TOML = ROOT / "crates" / "terlan" / "Cargo.toml"
FORBID_UNSAFE = "#![forbid(unsafe_code)]"
DENY_UNSAFE = "#![deny(unsafe_code)]"
AUDITED_UNSAFE_ENTRYPOINTS = {
    "crates/terlan/src/native_worker/main.rs",
}


def cargo_bin_paths() -> list[Path]:
    text = CARGO_TOML.read_text(encoding="utf-8")
    try:
        import tomllib

        manifest = tomllib.loads(text)
        bins = manifest.get("bin", [])
        return [ROOT / "crates" / "terlan" / bin_entry["path"] for bin_entry in bins]
    except Exception:
        paths: list[Path] = []
        in_bin = False
        for line in text.splitlines():
            stripped = line.strip()
            if stripped == "[[bin]]":
                in_bin = True
                continue
            if stripped.startswith("[") and stripped != "[[bin]]":
                in_bin = False
            if in_bin and stripped.startswith("path"):
                match = re.search(r'"([^"]+)"', stripped)
                if match:
                    paths.append(ROOT / "crates" / "terlan" / match.group(1))
        return paths


def required_roots() -> list[Path]:
    roots = cargo_bin_paths()
    roots.extend(
        [
            ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "mod.rs",
            ROOT / "crates" / "terlan" / "src" / "runtime" / "native_boundary" / "mod.rs",
        ]
    )
    roots.extend(sorted((ROOT / "std" / "summaries").glob("*.native_boundary.rs")))
    return roots


def first_meaningful_line(path: Path) -> str:
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped:
            return stripped
    return ""


def main() -> int:
    failures: list[str] = []
    roots = required_roots()
    for path in roots:
        if not path.exists():
            failures.append(f"missing required safe-Rust root: {path.relative_to(ROOT)}")
            continue
        first_line = first_meaningful_line(path)
        relative = path.relative_to(ROOT).as_posix()
        required_policy = (
            DENY_UNSAFE if relative in AUDITED_UNSAFE_ENTRYPOINTS else FORBID_UNSAFE
        )
        if first_line != required_policy:
            failures.append(
                f"{relative} must start with {required_policy!r}; found {first_line!r}"
            )

    if failures:
        print("safe Rust runtime check failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print(f"safe Rust runtime check passed ({len(roots)} roots)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
