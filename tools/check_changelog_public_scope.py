#!/usr/bin/env python3
"""Check that the changelog stays release-facing.

Inputs:
- `CHANGELOG.md`.

Outputs:
- Exit status 0 when changelog bullet entries avoid internal planning,
  scratch-workspace, and validation-bookkeeping language.
- Exit status 1 with line-specific diagnostics when internal-only terms appear
  in release-facing entries.

Transformation:
- Scans changelog bullet entries.
- Rejects phrases that describe internal scratch cleanup, preflight mechanics,
  boundary checks, drift checks, caches, proof experiments, or roadmap state
  instead of user-visible compiler/library changes.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHANGELOG = ROOT / "CHANGELOG.md"
FORBIDDEN_TERMS = (
    "scratch",
    "golden",
    "preflight",
    "release-boundary",
    "boundary check",
    "drift check",
    "drift-check",
    "__pycache__",
    "cache artifact",
    "roadmap",
    "checkpoint",
    "proof experiment",
    "validation artifact",
    "internal validation",
    "migration baseline",
)


@dataclass(frozen=True)
class ChangelogFinding:
    """Internal-only changelog wording finding.

    Inputs:
    - `line`: one-based changelog line number.
    - `term`: forbidden term that was found.
    - `text`: offending changelog line text.

    Outputs:
    - Immutable finding used for diagnostics.

    Transformation:
    - Keeps the forbidden term and source line together so maintainers can
      rewrite the entry as user-facing release language.
    """

    line: int
    term: str
    text: str

    def render(self) -> str:
        """Return a stable diagnostic string.

        Inputs:
        - Finding line, forbidden term, and source text.

        Outputs:
        - Human-readable diagnostic.

        Transformation:
        - Formats one finding as `CHANGELOG.md:line: term ...`.
        """

        return f"CHANGELOG.md:{self.line}: internal changelog term `{self.term}` in `{self.text}`"


def is_entry_line(line: str) -> bool:
    """Return whether a changelog line is part of a bullet entry.

    Inputs:
    - Raw changelog line.

    Outputs:
    - `True` for bullet starts and wrapped bullet continuation lines.
    - `False` for headings, blanks, and prose outside release entries.

    Transformation:
    - Treats indented continuation lines as entry text because multi-line
      bullets often wrap after the first line.
    """

    return line.startswith("- ") or line.startswith("  ")


def changelog_findings() -> list[ChangelogFinding]:
    """Return internal-only wording findings from changelog entries.

    Inputs:
    - Changelog text.

    Outputs:
    - Finding records for forbidden terms.

    Transformation:
    - Lowercases entry text for matching while preserving the original text in
      diagnostics.
    """

    findings: list[ChangelogFinding] = []
    for line_no, line in enumerate(CHANGELOG.read_text(encoding="utf-8").splitlines(), 1):
        if not is_entry_line(line):
            continue
        lowered = line.lower()
        for term in FORBIDDEN_TERMS:
            if term in lowered:
                findings.append(ChangelogFinding(line_no, term, line.strip()))
    return findings


def main() -> int:
    """Run the changelog public-scope check.

    Inputs:
    - Current `CHANGELOG.md`.

    Outputs:
    - Exit status 0 when the changelog stays user-facing.
    - Exit status 1 with diagnostics for internal-only changelog terms.

    Transformation:
    - Converts roadmap guidance about changelog scope into an executable release
      policy check.
    """

    findings = changelog_findings()
    if findings:
        print("changelog-public-scope-check failed:")
        for finding in findings:
            print(f"  - {finding.render()}")
        return 1
    print("[changelog-public-scope] changelog entries are release-facing.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
