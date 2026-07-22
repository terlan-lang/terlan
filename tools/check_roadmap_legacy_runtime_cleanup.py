#!/usr/bin/env python3
"""Validate legacy runtime references in active roadmap files.

Inputs:
- Active roadmap markdown files from `docs/roadmap`.
- `docs/runtime/LEGACY_RUNTIME_ALLOWED_REFERENCES.tsv`.

Outputs:
- Exit status 0 when every legacy runtime reference is explicitly classified
  as removal, parity-port, stale-proof-cleanup, legacy-metadata-rejection, or
  historical-baseline.
- Exit status 1 with stable diagnostics for unclassified references, stale
  allowlist rows, duplicate matches, invalid categories, or malformed rows.

Transformation:
- Keeps future 0.0.7 roadmap work from reintroducing OTP/BEAM/Erlang as an
  active product direction while still allowing removal and parity-porting
  language to remain visible.
"""

from __future__ import annotations

from dataclasses import dataclass
from fnmatch import fnmatchcase
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
ALLOWLIST = ROOT / "docs" / "runtime" / "LEGACY_RUNTIME_ALLOWED_REFERENCES.tsv"
ROADMAP_ROOT_CANDIDATES = (
    ROOT / "docs" / "roadmap",
    ROOT.parent / "docs" / "roadmap",
)
ACTIVE_ROADMAP_NAMES = {
    "ROADMAP_0_0_7.md",
    "ROADMAP_0_0_7_UNBLOCKERS.md",
}
ALLOWED_CATEGORIES = {
    "removal",
    "parity-port",
    "stale-proof-cleanup",
    "legacy-metadata-rejection",
    "historical-baseline",
}
LEGACY_RUNTIME_PATTERN = re.compile(
    r"\b(OTP|BEAM|Erlang|EUnit|ERTS|epmd|CoreV0|beam-thin|beam|erlang|eunit|erts)\b"
)


@dataclass(frozen=True)
class AllowRow:
    """One accepted legacy-runtime reference pattern."""

    file_glob: str
    line_regex: re.Pattern[str]
    category: str
    notes: str
    line_number: int


@dataclass(frozen=True)
class LegacyReference:
    """One legacy runtime reference found in an active roadmap file."""

    path: Path
    line_number: int
    text: str


def roadmap_root() -> Path | None:
    """Return the active roadmap root when present."""

    return select_roadmap_root(ROADMAP_ROOT_CANDIDATES)


def select_roadmap_root(candidates: tuple[Path, ...]) -> Path | None:
    """Return the first candidate containing an active roadmap file."""

    for candidate in candidates:
        if candidate.is_dir() and any(
            (candidate / name).is_file() for name in ACTIVE_ROADMAP_NAMES
        ):
            return candidate
    return None


def read_allowlist(path: Path) -> tuple[list[AllowRow], list[str]]:
    """Read the allowlist TSV and return rows plus diagnostics."""

    rows: list[AllowRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(ROOT)}: missing allowlist file"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw_line.strip() or raw_line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 4:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: expected 4 TSV fields, found {len(parts)}"
            )
            continue
        file_glob, regex_source, category, notes = parts
        try:
            line_regex = re.compile(regex_source)
        except re.error as err:
            findings.append(f"{path.relative_to(ROOT)}:{line_number}: invalid regex: {err}")
            continue
        if category not in ALLOWED_CATEGORIES:
            findings.append(f"{path.relative_to(ROOT)}:{line_number}: unknown category `{category}`")
        if not notes:
            findings.append(f"{path.relative_to(ROOT)}:{line_number}: notes are required")
        rows.append(
            AllowRow(
                file_glob=file_glob,
                line_regex=line_regex,
                category=category,
                notes=notes,
                line_number=line_number,
            )
        )
    return rows, findings


def active_roadmap_files(root: Path) -> list[Path]:
    """Return active roadmap files checked by the gate."""

    return sorted(path for path in root.iterdir() if path.is_file() and path.name in ACTIVE_ROADMAP_NAMES)


def legacy_references(files: list[Path]) -> list[LegacyReference]:
    """Return all legacy runtime references in active roadmap files."""

    refs: list[LegacyReference] = []
    for path in files:
        for line_number, text in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if LEGACY_RUNTIME_PATTERN.search(text):
                refs.append(LegacyReference(path=path, line_number=line_number, text=text))
    return refs


def matching_rows(reference: LegacyReference, rows: list[AllowRow]) -> list[AllowRow]:
    """Return allowlist rows that classify a legacy reference."""

    return [
        row
        for row in rows
        if fnmatchcase(reference.path.name, row.file_glob) and row.line_regex.search(reference.text)
    ]


def audit(rows: list[AllowRow], references: list[LegacyReference]) -> list[str]:
    """Return all legacy-runtime cleanup findings."""

    findings: list[str] = []
    for reference in references:
        matches = matching_rows(reference, rows)
        relative = reference.path.name
        if not matches:
            findings.append(
                f"{relative}:{reference.line_number}: unclassified legacy runtime reference: {reference.text.strip()}"
            )
        elif len(matches) > 1:
            rendered = ", ".join(f"{row.category}@{row.line_number}" for row in matches)
            findings.append(f"{relative}:{reference.line_number}: legacy runtime reference matches multiple rows: {rendered}")

    for row in rows:
        matched = [
            ref
            for ref in references
            if fnmatchcase(ref.path.name, row.file_glob) and row.line_regex.search(ref.text)
        ]
        if not matched:
            findings.append(
                f"{ALLOWLIST.relative_to(ROOT)}:{row.line_number}: allowlist row matches no active roadmap reference"
            )
    return findings


def print_summary(references: list[LegacyReference], rows: list[AllowRow]) -> None:
    """Print stable cleanup counts for successful runs."""

    by_category: dict[str, int] = {}
    for reference in references:
        row = matching_rows(reference, rows)[0]
        by_category[row.category] = by_category.get(row.category, 0) + 1
    print(f"roadmap legacy runtime cleanup: {len(references)} references classified")
    for category, count in sorted(by_category.items()):
        print(f"  {category}: {count}")


def main() -> int:
    """Run the roadmap legacy runtime cleanup gate."""

    root = roadmap_root()
    if root is None:
        print("roadmap legacy runtime cleanup skipped: no docs/roadmap root found", file=sys.stderr)
        return 0
    rows, findings = read_allowlist(ALLOWLIST)
    files = active_roadmap_files(root)
    references = legacy_references(files)
    findings.extend(audit(rows, references))
    if findings:
        print("roadmap legacy runtime cleanup failed:", file=sys.stderr)
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        return 1
    print_summary(references, rows)
    return 0


if __name__ == "__main__":
    sys.exit(main())
