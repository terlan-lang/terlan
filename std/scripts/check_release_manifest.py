#!/usr/bin/env python3
"""Validate the Terlan standard-library release manifest.

Inputs:
- `std/RELEASE_MANIFEST.tsv`.
- Standard-library source files under `std/`.
- Generated summaries under `std/summaries/`.
- Generated docs under `/tmp/terlan-std-docs` by default.
- Exact API test coverage rows in `tests/std/RELEASE_API_TESTS.tsv`,
  pointing at adjacent std `*_test.terl` files.

Outputs:
- Exit status 0 when the release manifest is complete.
- Exit status 1 with stable diagnostics when source, summary, documentation, or
  exact test coverage is missing.

Transformation:
- Reads the module release manifest, validates each module artifact, then checks
  every per-API test row maps to a release module and an annotated Terlan test.
"""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "std" / "RELEASE_MANIFEST.tsv"
DEFAULT_DOCS_DIR = Path("/tmp/terlan-std-docs")
HEADER = ["kind", "id", "source", "summary", "tests", "docs"]


@dataclass(frozen=True)
class ManifestRow:
    """One stdlib release-manifest row.

    Inputs:
    - Parsed TSV cells from `std/RELEASE_MANIFEST.tsv`.

    Outputs:
    - Immutable manifest row used by artifact and coverage checks.

    Transformation:
    - Gives names to the six tab-separated cells without changing their text.
    """

    kind: str
    identifier: str
    source: str
    summary: str
    tests: str
    docs: str


def parse_args() -> argparse.Namespace:
    """Parse command-line options for the manifest checker.

    Inputs:
    - Process command-line arguments.

    Outputs:
    - Namespace containing the documentation output directory.

    Transformation:
    - Provides a stable default docs directory while allowing Make targets and
      local checks to point at a different generated documentation tree.
    """

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--docs-dir",
        default=str(DEFAULT_DOCS_DIR),
        help="directory containing generated `terlc doc std` HTML output",
    )
    return parser.parse_args()


def read_manifest(path: Path) -> tuple[list[ManifestRow], list[str]]:
    """Read and validate the stdlib release manifest rows.

    Inputs:
    - `path`: manifest path.

    Outputs:
    - Parsed manifest rows and diagnostics.

    Transformation:
    - Skips comments and blank lines, validates the header and field count, and
      converts remaining rows into typed `ManifestRow` values.
    """

    diagnostics: list[str] = []
    rows: list[ManifestRow] = []
    header_seen = False
    if not path.is_file():
        return rows, [f"missing std release manifest: {path.relative_to(ROOT)}"]
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if not header_seen:
            if fields != HEADER:
                diagnostics.append(
                    f"{path.relative_to(ROOT)}:{number}: expected header {HEADER}, got {fields}"
                )
            header_seen = True
            continue
        if len(fields) != len(HEADER):
            diagnostics.append(
                f"{path.relative_to(ROOT)}:{number}: expected {len(HEADER)} fields, got {len(fields)}"
            )
            continue
        rows.append(ManifestRow(*fields))
    if not header_seen:
        diagnostics.append(f"{path.relative_to(ROOT)}: missing TSV header")
    return rows, diagnostics


def check_manifest_artifacts(rows: list[ManifestRow], docs_dir: Path) -> list[str]:
    """Validate source, summary, test, and documentation artifacts.

    Inputs:
    - `rows`: parsed release-manifest rows.
    - `docs_dir`: generated docs directory.

    Outputs:
    - Artifact diagnostics.

    Transformation:
    - Checks module rows against real files and validates the single
      `api_manifest` row that points to exact API test coverage.
    """

    diagnostics: list[str] = []
    api_manifest_rows = [row for row in rows if row.kind == "api_manifest"]
    if len(api_manifest_rows) != 1:
        diagnostics.append("std release manifest must contain exactly one api_manifest row")
    for row in rows:
        if row.kind not in {"api_manifest", "module"}:
            diagnostics.append(f"{row.identifier}: unsupported manifest row kind `{row.kind}`")
            continue
        for field, value in (("source", row.source), ("summary", row.summary), ("tests", row.tests)):
            if not (ROOT / value).is_file() and field != "source":
                diagnostics.append(f"{row.identifier}: missing {field} file `{value}`")
            if field == "source" and not (ROOT / value).exists():
                diagnostics.append(f"{row.identifier}: missing source path `{value}`")
        if row.kind == "module":
            doc_path = docs_dir / row.docs
            if not doc_path.is_file():
                diagnostics.append(f"{row.identifier}: missing generated docs page `{doc_path}`")
    return diagnostics


def module_rows(rows: list[ManifestRow]) -> dict[str, ManifestRow]:
    """Return module rows keyed by module identifier.

    Inputs:
    - Parsed manifest rows.

    Outputs:
    - Mapping from module name to manifest row.

    Transformation:
    - Filters non-module rows and preserves the declared module identifiers.
    """

    return {row.identifier: row for row in rows if row.kind == "module"}


def read_api_manifest(path: Path) -> tuple[list[tuple[str, str, str]], list[str]]:
    """Read the exact per-API test coverage manifest.

    Inputs:
    - `path`: API coverage manifest path.

    Outputs:
    - Tuples of API id, test file, and test function plus diagnostics.

    Transformation:
    - Skips comments and blank lines, validates three-column TSV rows, and
      preserves test identity for later source scanning.
    """

    rows: list[tuple[str, str, str]] = []
    diagnostics: list[str] = []
    if not path.is_file():
        return rows, [f"missing API test manifest: {path.relative_to(ROOT)}"]
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 3:
            diagnostics.append(f"{path.relative_to(ROOT)}:{number}: expected 3 fields")
            continue
        rows.append((fields[0], fields[1], fields[2]))
    return rows, diagnostics


def owning_module(api_id: str, modules: dict[str, ManifestRow]) -> str | None:
    """Find the release module that owns an API id.

    Inputs:
    - `api_id`: release API identifier.
    - `modules`: release module rows keyed by module name.

    Outputs:
    - Longest module prefix that owns the API id.
    - `None` when no release module owns the API id.

    Transformation:
    - Performs longest-prefix matching so `std.core.Ordering.Bool.lt` maps to
      `std.core.Ordering` rather than the broader `std.core` package.
    """

    matches = [module for module in modules if api_id == module or api_id.startswith(f"{module}.")]
    return max(matches, key=len) if matches else None


def has_annotated_test(path: Path, function_name: str) -> bool:
    """Return whether a Terlan test file defines an annotated test function.

    Inputs:
    - `path`: Terlan test source path.
    - `function_name`: public zero-argument function expected after `@test`.

    Outputs:
    - `True` when `@test` immediately introduces the named public function.
    - `False` otherwise.

    Transformation:
    - Scans line-oriented Terlan source without parsing the full module.
    """

    pattern = re.compile(rf"^\s*pub\s+{re.escape(function_name)}\(")
    pending = False
    for line in path.read_text(encoding="utf-8").splitlines():
        if re.match(r"^\s*@test\s*$", line):
            pending = True
            continue
        if not line.strip():
            continue
        if pending and pattern.match(line):
            return True
        pending = False
    return False


def check_api_coverage(rows: list[ManifestRow]) -> list[str]:
    """Validate every release API has module and exact test coverage.

    Inputs:
    - Parsed std release-manifest rows.

    Outputs:
    - Coverage diagnostics.

    Transformation:
    - Reads the per-API test manifest referenced by the `api_manifest` row,
      maps each API id to a release module, and validates its exact annotated
      Terlan test function.
    """

    diagnostics: list[str] = []
    api_manifest = next((row for row in rows if row.kind == "api_manifest"), None)
    if api_manifest is None:
        return ["std release manifest is missing api_manifest row"]
    modules = module_rows(rows)
    api_rows, api_diagnostics = read_api_manifest(ROOT / api_manifest.tests)
    diagnostics.extend(api_diagnostics)
    for api_id, test_file, test_function in api_rows:
        owner = owning_module(api_id, modules)
        if owner is None:
            diagnostics.append(f"{api_id}: no release module owns this API id")
        test_path = ROOT / test_file
        if not test_path.is_file():
            diagnostics.append(f"{api_id}: missing test file `{test_file}`")
            continue
        if not has_annotated_test(test_path, test_function):
            diagnostics.append(
                f"{api_id}: missing @test function `{test_function}` in `{test_file}`"
            )
    return diagnostics


def main() -> int:
    """Run the stdlib release manifest validation.

    Inputs:
    - CLI docs-dir option and repository-local manifest files.

    Outputs:
    - Process exit code.

    Transformation:
    - Parses the release manifest, checks artifacts, checks exact API tests, and
      prints a concise release-manifest status line.
    """

    args = parse_args()
    rows, diagnostics = read_manifest(MANIFEST)
    docs_dir = Path(args.docs_dir)
    diagnostics.extend(check_manifest_artifacts(rows, docs_dir))
    diagnostics.extend(check_api_coverage(rows))
    if diagnostics:
        print("[std-release-manifest] failures:", file=sys.stderr)
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}", file=sys.stderr)
        return 1
    print(f"[std-release-manifest] {len(module_rows(rows))} modules and API tests are covered.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
