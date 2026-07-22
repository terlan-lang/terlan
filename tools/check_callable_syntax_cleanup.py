#!/usr/bin/env python3
"""Reject removed Terlan function-value dot-call syntax.

Inputs:
- Canonical Terlan std sources, tests, grammar docs, editor snippets, and
  compiler/runtime source files.

Outputs:
- Exit status 0 when no canonical source still uses `f.(args)` or
  `(expr).(args)` callable syntax.
- Exit status 1 with stable file/line diagnostics for any remaining usage.

Transformation:
- Keeps higher-order Terlan code on ordinary call syntax such as
  `callback(value)` and prevents the removed dot-call form from reappearing in
  std modules, docs, tests, parser fixtures, or editor surfaces.
"""

from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
SCAN_ROOTS = (
    ROOT / "std",
    ROOT / "tests",
    ROOT / "docs" / "grammar",
    ROOT / "crates" / "terlan" / "src",
    ROOT / "editors",
)
SCAN_SUFFIXES = {
    ".terl",
    ".terli",
    ".typi",
    ".rs",
    ".md",
    ".js",
    ".json",
    ".scm",
}
IGNORED_DIRS = {
    ".git",
    "target",
    "node_modules",
    "__pycache__",
}
REMOVED_DOT_CALL = re.compile(r"(\b[A-Za-z_][A-Za-z0-9_]*|\))\.\s*\(")
ALLOWED_NEGATIVE_FIXTURES = {
    (
        Path("crates/terlan/src/compiler/syntax/parser_expr_test.rs"),
        "callback.(1)",
    ),
    (
        Path("crates/terlan/src/compiler/syntax/parser_expr_test.rs"),
        "(callback).(1)",
    ),
}


def is_allowed_negative_fixture(path: Path, line: str) -> bool:
    """Return whether a removed syntax occurrence is an intentional fixture."""

    relative = path.relative_to(ROOT)
    return any(
        relative == fixture_path and fixture_text in line
        for fixture_path, fixture_text in ALLOWED_NEGATIVE_FIXTURES
    )


def source_files() -> list[Path]:
    """Return sorted files that may contain canonical callable syntax."""

    files: list[Path] = []
    for root in SCAN_ROOTS:
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            if any(part in IGNORED_DIRS for part in path.relative_to(ROOT).parts):
                continue
            if path.suffix in SCAN_SUFFIXES:
                files.append(path)
    return sorted(files)


def findings(paths: list[Path]) -> list[str]:
    """Return diagnostics for removed dot-call syntax."""

    diagnostics: list[str] = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            if REMOVED_DOT_CALL.search(line):
                if is_allowed_negative_fixture(path, line):
                    continue
                diagnostics.append(
                    f"{path.relative_to(ROOT)}:{line_number}: removed callable dot-call syntax: {line.strip()}"
                )
    return diagnostics


def main() -> int:
    """Run the callable syntax cleanup gate."""

    diagnostics = findings(source_files())
    if diagnostics:
        print("callable syntax cleanup failed:", file=sys.stderr)
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}", file=sys.stderr)
        return 1
    print("callable syntax cleanup: no removed dot-call syntax found")
    return 0


if __name__ == "__main__":
    sys.exit(main())
