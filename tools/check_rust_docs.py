#!/usr/bin/env python3
"""Check Rustdoc coverage for Rust functions and types.

Inputs:
- Rust source files under `crates/`.
- `tools/quality/rustdoc_missing_baseline.tsv`.

Outputs:
- Exit status 0 when undocumented Rust items match the migration baseline.
- Exit status 1 with item-specific diagnostics when new undocumented items
  appear or baseline rows become stale.

Transformation:
- Scans implementation Rust source text for function and type declarations.
- Classifies each declaration by whether it has adjacent Rustdoc.
- Compares undocumented declarations against the checked-in baseline.
"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
BASELINE = ROOT / "tools" / "quality" / "rustdoc_missing_baseline.tsv"
FUNCTION_PATTERN = re.compile(
    r'^\s*(?:pub(?:\([^)]*\))?\s+)?'
    r'(?:(?:async|const|unsafe|extern(?:\s+"[^"]+")?)\s+)*'
    r"fn\s+([A-Za-z_][A-Za-z0-9_]*)\b"
)
TYPE_PATTERN = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(struct|enum|union|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)\b"
)
RAW_STRING_OPEN_PATTERN = re.compile(r'b?r(#+)?"')


@dataclass(frozen=True)
class RustItem:
    """Rust declaration discovered by the documentation checker.

    Inputs:
    - `path`: repository-relative Rust source path.
    - `kind`: item category such as `fn`, `struct`, or `trait`.
    - `name`: declared Rust identifier.
    - `signature`: normalized declaration line used as a stable baseline key.
    - `line`: one-based source line for diagnostics.
    - `documented`: whether adjacent Rustdoc was found.

    Outputs:
    - Immutable item record consumed by baseline validation.

    Transformation:
    - Keeps declaration identity, source location, and documentation state
      together so quality diagnostics can be precise.
    """

    path: Path
    kind: str
    name: str
    signature: str
    line: int
    documented: bool

    def key(self) -> str:
        """Return the baseline key for this Rust item.

        Inputs:
        - The item path, kind, name, and normalized signature.

        Outputs:
        - A tab-separated key suitable for checked-in baseline files.

        Transformation:
        - Converts path and declaration identity into stable text without
          embedding source line numbers.
        """

        return f"{self.path}\t{self.kind}\t{self.name}\t{self.signature}"


def normalized_signature(line: str) -> str:
    """Return a whitespace-normalized declaration line.

    Inputs:
    - Raw source line containing a Rust item declaration.

    Outputs:
    - Single-line signature text for baseline comparison.

    Transformation:
    - Trims leading/trailing whitespace, collapses internal whitespace, and
      removes trailing body/opening markers that are not part of the signature.
    """

    signature = " ".join(line.strip().split())
    return signature.rstrip(" {")


def line_has_rustdoc(lines: list[str], item_index: int) -> bool:
    """Return whether an item has adjacent Rustdoc.

    Inputs:
    - `lines`: source file split into lines.
    - `item_index`: zero-based index of the item declaration line.

    Outputs:
    - `True` when `///`, `/** ... */`, or `//!` documentation is adjacent.
    - `False` otherwise.

    Transformation:
    - Walks upward past attributes attached to the item and checks the closest
      documentation comment block.
    """

    index = item_index - 1
    while index >= 0 and lines[index].strip().startswith("#["):
        index -= 1
    if index < 0:
        return False

    previous = lines[index].strip()
    if previous.startswith("///") or previous.startswith("//!"):
        return True
    if previous.endswith("*/"):
        while index >= 0:
            text = lines[index].strip()
            if text.startswith("/**") or text.startswith("/*!"):
                return True
            if text.startswith("/*"):
                return False
            index -= 1
    return previous.startswith("/**") or previous.startswith("/*!")


def escaped_string_state(line: str, active: bool) -> tuple[bool, bool]:
    """Return whether a line should be skipped as an escaped fixture string.

    Inputs:
    - `line`: current source line.
    - `active`: whether the previous source line opened an escaped string.

    Outputs:
    - Updated escaped-string state.
    - `True` when the current line is part of the string and should be skipped.

    Transformation:
    - Tracks Rust test fixtures written as `"\\` followed by source-like lines
      ending in `\\n\\`, which otherwise look like real Rust declarations.
    """

    stripped = line.strip()
    if active:
        return not stripped.endswith('",'), True
    if stripped == '"\\':
        return True, True
    return False, False


def raw_string_state(line: str, terminator: str | None) -> tuple[str | None, bool]:
    """Return whether a line should be skipped as a Rust raw string literal.

    Inputs:
    - `line`: current source line.
    - `terminator`: active raw-string terminator such as `"#`, or `None`.

    Outputs:
    - Updated raw-string terminator state.
    - `True` when the current line is part of a raw string and should be
      skipped.

    Transformation:
    - Tracks raw strings such as `r#"..."#` and `r###"..."###` so embedded
      Terlan/Rust-like fixture text is not counted as real Rust declarations.
    """

    if terminator is not None:
        return (None if terminator in line else terminator), True

    raw_start = RAW_STRING_OPEN_PATTERN.search(line)
    if raw_start is None:
        return None, False

    hashes = raw_start.group(1) or ""
    raw_terminator = f'"{hashes}'
    remainder = line[raw_start.end() :]
    return (None if raw_terminator in remainder else raw_terminator), True


def discover_items() -> list[RustItem]:
    """Discover Rust functions and types under implementation files in `crates/`.

    Inputs:
    - Repository Rust files under `crates/`.

    Outputs:
    - Sorted Rust item records.

    Transformation:
    - Skips adjacent `*_test.rs` modules because the Rustdoc rule protects
      compiler implementation files, not test bodies.
    - Reads each implementation Rust file, matches declaration lines with
      conservative regexes, and records whether each declaration has adjacent
      Rustdoc.
    """

    items: list[RustItem] = []
    for path in sorted(CRATES.rglob("*.rs")):
        if path.name.endswith("_test.rs"):
            continue
        relative = path.relative_to(ROOT)
        lines = path.read_text(encoding="utf-8").splitlines()
        in_escaped_string = False
        raw_string_terminator: str | None = None
        for index, line in enumerate(lines):
            raw_string_terminator, skip_raw_string = raw_string_state(
                line, raw_string_terminator
            )
            if skip_raw_string:
                continue

            in_escaped_string, skip_line = escaped_string_state(line, in_escaped_string)
            if skip_line:
                continue

            function_match = FUNCTION_PATTERN.match(line)
            if function_match is not None:
                items.append(
                    RustItem(
                        path=relative,
                        kind="fn",
                        name=function_match.group(1),
                        signature=normalized_signature(line),
                        line=index + 1,
                        documented=line_has_rustdoc(lines, index),
                    )
                )
                continue

            type_match = TYPE_PATTERN.match(line)
            if type_match is not None:
                items.append(
                    RustItem(
                        path=relative,
                        kind=type_match.group(1),
                        name=type_match.group(2),
                        signature=normalized_signature(line),
                        line=index + 1,
                        documented=line_has_rustdoc(lines, index),
                    )
                )
    return items


def read_baseline() -> tuple[set[str], list[str]]:
    """Read the undocumented Rustdoc migration baseline.

    Inputs:
    - `tools/quality/rustdoc_missing_baseline.tsv`.

    Outputs:
    - Set of item keys allowed to remain undocumented.
    - Diagnostics for malformed rows.

    Transformation:
    - Parses tab-separated path/kind/name/signature rows into comparable keys.
    """

    baseline: set[str] = set()
    diagnostics: list[str] = []
    if not BASELINE.exists():
        diagnostics.append(f"{BASELINE}: missing baseline; run with --write-baseline")
        return baseline, diagnostics

    for number, line in enumerate(BASELINE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        if len(line.split("\t")) != 4:
            diagnostics.append(f"{BASELINE}:{number}: expected path<TAB>kind<TAB>name<TAB>signature")
            continue
        baseline.add(line)
    return baseline, diagnostics


def undocumented_items(items: list[RustItem]) -> dict[str, RustItem]:
    """Return undocumented Rust items keyed by baseline identity.

    Inputs:
    - Discovered Rust items.

    Outputs:
    - Mapping from baseline key to undocumented item.

    Transformation:
    - Filters documented declarations away and keeps the remaining item records
      for diagnostics and baseline writing.
    """

    return {item.key(): item for item in items if not item.documented}


def check_baseline(current: dict[str, RustItem], baseline: set[str]) -> list[str]:
    """Validate undocumented items against the baseline.

    Inputs:
    - Current undocumented Rust items.
    - Checked-in undocumented-item baseline keys.

    Outputs:
    - Diagnostics for new undocumented items and stale baseline entries.

    Transformation:
    - Treats existing undocumented declarations as migration debt while
      blocking new undocumented functions or types from entering the tree.
    """

    diagnostics: list[str] = []
    for key in sorted(baseline):
        if key not in current:
            diagnostics.append(f"{key}: stale Rustdoc baseline row")
    for key, item in sorted(current.items()):
        if key not in baseline:
            diagnostics.append(
                f"{item.path}:{item.line}: undocumented {item.kind} `{item.name}`; "
                "add Rustdoc or update reviewed baseline"
            )
    return diagnostics


def write_baseline(items: dict[str, RustItem]) -> None:
    """Write the current undocumented Rustdoc baseline.

    Inputs:
    - Current undocumented Rust items.

    Outputs:
    - Rewrites `tools/quality/rustdoc_missing_baseline.tsv`.

    Transformation:
    - Serializes current undocumented declaration keys in sorted order with a
      short file header.
    """

    BASELINE.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Existing undocumented Rust items allowed during 0.0.4 consolidation.",
        "# New Rust functions and types must add Rustdoc instead of extending this file.",
    ]
    lines.extend(sorted(items))
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments for the Rustdoc checker.

    Inputs:
    - Process command-line arguments.

    Outputs:
    - Parsed argparse namespace.

    Transformation:
    - Provides a maintainer-only baseline rewrite flag while keeping normal
      execution read-only.
    """

    parser = argparse.ArgumentParser(description="check Rustdoc coverage baseline")
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="rewrite the undocumented Rustdoc baseline from the current tree",
    )
    return parser.parse_args()


def main() -> int:
    """Run Rustdoc coverage validation.

    Inputs:
    - Rust source files and optional `--write-baseline` flag.

    Outputs:
    - Exit status 0 when Rustdoc debt has not grown.
    - Exit status 1 with diagnostics when documentation coverage regresses.

    Transformation:
    - Discovers Rust items, compares undocumented declarations against the
      baseline, and optionally rewrites the migration baseline.
    """

    args = parse_args()
    current = undocumented_items(discover_items())
    if args.write_baseline:
        write_baseline(current)
        print(f"[rustdoc] wrote baseline with {len(current)} undocumented items.")
        return 0

    baseline, diagnostics = read_baseline()
    diagnostics.extend(check_baseline(current, baseline))
    if diagnostics:
        print("[rustdoc] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}")
        return 1

    print(f"[rustdoc] baseline enforced: {len(current)} undocumented items.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
