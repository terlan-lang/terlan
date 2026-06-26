#!/usr/bin/env python3
"""Validate and optionally normalize Terlan std documentation block spacing.

Inputs:
- Standard-library `.terl` and `.terli` files under `std/`.
- Optional `--fix` flag.

Outputs:
- Exit status 0 when every documentation block body marker is formatted as
  ` * text`, ` *`, or ` */`.
- Exit status 1 with file/line diagnostics when malformed marker spacing is
  found in check mode.

Transformation:
- In fix mode, rewrites documentation block body lines such as ` *Text` and
  ` *@param` to ` * Text` and ` * @param` without changing non-doc comments.
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
STD = ROOT / "std"
SOURCE_SUFFIXES = {".terl", ".terli"}


@dataclass(frozen=True)
class DocFormatFinding:
    """One malformed documentation marker line.

    Inputs:
    - Source path, line number, and original line text.

    Outputs:
    - Immutable finding used for diagnostics.

    Transformation:
    - Carries scanner output without modifying source text.
    """

    path: Path
    line_number: int
    line: str

    def diagnostic(self) -> str:
        """Render one stable source diagnostic.

        Inputs:
        - Finding fields.

        Outputs:
        - Human-readable `path:line: message` diagnostic.

        Transformation:
        - Converts the absolute path to a repository-relative path and embeds
          the offending source line for quick repair.
        """

        relative = self.path.relative_to(ROOT)
        return (
            f"{relative}:{self.line_number}: doc block marker must be followed "
            f"by a space: {self.line}"
        )


def parse_args() -> argparse.Namespace:
    """Parse checker options.

    Inputs:
    - Process command-line arguments.

    Outputs:
    - Namespace containing the `fix` mode flag.

    Transformation:
    - Keeps the default behavior as validation-only so Make targets cannot
      rewrite source by accident.
    """

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--fix",
        action="store_true",
        help="rewrite malformed doc block marker spacing in place",
    )
    return parser.parse_args()


def iter_std_sources() -> list[Path]:
    """Return Terlan std source files that should obey doc formatting.

    Inputs:
    - Repository `std/` directory.

    Outputs:
    - Sorted list of `.terl` and `.terli` source paths.

    Transformation:
    - Recursively walks std source directories and excludes generated summary
      artifacts because they are not Terlan source modules.
    """

    return sorted(
        path
        for path in STD.rglob("*")
        if path.is_file()
        and path.suffix in SOURCE_SUFFIXES
        and "summaries" not in path.relative_to(STD).parts
    )


def normalize_line(line: str, in_doc_block: bool) -> tuple[str, bool, bool]:
    """Normalize one source line when it belongs to a doc block.

    Inputs:
    - `line`: source line without trailing newline.
    - `in_doc_block`: whether previous lines opened a `/**` block.

    Outputs:
    - Normalized line text.
    - Updated doc-block state for the next line.
    - Whether the original line had malformed marker spacing.

    Transformation:
    - Tracks `/** ... */` blocks and inserts one space after body marker `*`
      when text follows immediately. Closing `*/` and blank `*` lines are left
      unchanged.
    """

    active = in_doc_block or line.lstrip().startswith("/**")
    malformed = False
    normalized = line
    if active:
        indent_len = len(line) - len(line.lstrip())
        rest = line[indent_len:]
        if rest.startswith("*") and not rest.startswith("*/"):
            after_marker = rest[1:]
            if after_marker and not after_marker[0].isspace():
                normalized = f"{line[:indent_len]}* {after_marker}"
                malformed = True
    if active and "*/" in line:
        active = False
    return normalized, active, malformed


def normalize_source(text: str) -> tuple[str, list[tuple[int, str]]]:
    """Normalize one source file's documentation marker spacing.

    Inputs:
    - Complete source text.

    Outputs:
    - Potentially rewritten source text.
    - Line-number/text pairs for malformed original lines.

    Transformation:
    - Applies `normalize_line` to every line while preserving the source file's
      trailing newline convention.
    """

    has_trailing_newline = text.endswith("\n")
    lines = text.splitlines()
    normalized_lines: list[str] = []
    malformed: list[tuple[int, str]] = []
    in_doc_block = False
    for number, line in enumerate(lines, start=1):
        normalized, in_doc_block, changed = normalize_line(line, in_doc_block)
        normalized_lines.append(normalized)
        if changed:
            malformed.append((number, line))
    normalized_text = "\n".join(normalized_lines)
    if has_trailing_newline:
        normalized_text += "\n"
    return normalized_text, malformed


def check_or_fix_source(path: Path, fix: bool) -> list[DocFormatFinding]:
    """Validate or rewrite one std source file.

    Inputs:
    - `path`: source file to inspect.
    - `fix`: whether to write normalized source back to disk.

    Outputs:
    - Findings for malformed original lines.

    Transformation:
    - Reads UTF-8 source, normalizes documentation markers, and writes the
      normalized text only when fix mode is enabled and content changed.
    """

    text = path.read_text(encoding="utf-8")
    normalized, malformed = normalize_source(text)
    if fix and normalized != text:
        path.write_text(normalized, encoding="utf-8")
    return [
        DocFormatFinding(path=path, line_number=line_number, line=line)
        for line_number, line in malformed
    ]


def main() -> int:
    """Run the std documentation-format checker.

    Inputs:
    - CLI options and current repository std source tree.

    Outputs:
    - Process exit status.

    Transformation:
    - Aggregates findings across std sources, optionally rewrites them, and
      prints a concise success or failure summary.
    """

    args = parse_args()
    findings: list[DocFormatFinding] = []
    for path in iter_std_sources():
        findings.extend(check_or_fix_source(path, args.fix))

    if findings and not args.fix:
        print("[stdlib-doc-format] failures:", file=sys.stderr)
        for finding in findings:
            print(finding.diagnostic(), file=sys.stderr)
        return 1

    if args.fix:
        print(f"[stdlib-doc-format] normalized {len(findings)} doc marker lines.")
    else:
        print("[stdlib-doc-format] std documentation block spacing is canonical.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
