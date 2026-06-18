#!/usr/bin/env python3
"""Check that internal planning documents do not enter published docs.

Inputs:
- Files under `docs/`.

Outputs:
- Exit status 0 when published docs contain only release-facing references.
- Exit status 1 with path-specific diagnostics for internal roadmap packets.

Transformation:
- Scans the published documentation tree for filenames and directories that
  are reserved for scratch planning, roadmap, baseline, checkpoint, or research
  material.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = ROOT / "docs"
FORBIDDEN_NAME_PARTS = (
    "roadmap",
    "baseline",
    "checkpoint",
    "scratch",
    "research",
)


@dataclass(frozen=True)
class InternalDocFinding:
    """Published documentation path that looks internal.

    Inputs:
    - `path`: repository-relative path to the internal-looking document.
    - `term`: forbidden term found in the path.

    Outputs:
    - Immutable finding for diagnostic rendering.

    Transformation:
    - Keeps the path and matched term together so the maintainer can either
      delete the file, move it to scratch documentation, or rename it as a
      release-facing contract.
    """

    path: Path
    term: str

    def render(self) -> str:
        """Return this finding as a stable diagnostic.

        Inputs:
        - Finding path and forbidden term.

        Outputs:
        - Human-readable diagnostic.

        Transformation:
        - Formats one finding as `path: internal docs term ...`.
        """

        return f"{self.path}: internal docs term `{self.term}` belongs outside published docs"


def doc_paths() -> list[Path]:
    """Return published documentation files.

    Inputs:
    - Repository `docs/` directory.

    Outputs:
    - Repository-relative documentation file paths.

    Transformation:
    - Recursively walks `docs/` and ignores directories because file paths carry
      the full directory context needed for matching.
    """

    if not DOCS.is_dir():
        return []
    return sorted(path.relative_to(ROOT) for path in DOCS.rglob("*") if path.is_file())


def internal_doc_findings(paths: list[Path]) -> list[InternalDocFinding]:
    """Return internal-looking documentation path findings.

    Inputs:
    - Repository-relative documentation file paths.

    Outputs:
    - Findings for forbidden planning terms in path parts.

    Transformation:
    - Lowercases each path part and matches forbidden planning terms in the
      filename or directory names.
    """

    findings: list[InternalDocFinding] = []
    for path in paths:
        lowered_parts = [part.lower() for part in path.parts]
        for term in FORBIDDEN_NAME_PARTS:
            if any(term in part for part in lowered_parts):
                findings.append(InternalDocFinding(path, term))
                break
    return findings


def main() -> int:
    """Run the published-docs internal leakage check.

    Inputs:
    - Current repository documentation files.

    Outputs:
    - Exit status 0 when no internal planning docs are present.
    - Exit status 1 with stable diagnostics otherwise.

    Transformation:
    - Converts the 0.0.4 single-root release rule into an executable gate for
      the published repository.
    """

    findings = internal_doc_findings(doc_paths())
    if findings:
        print("[internal-docs] failures:")
        for finding in findings:
            print(f"  - {finding.render()}")
        return 1
    print("[internal-docs] published docs contain no roadmap or scratch packets.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
