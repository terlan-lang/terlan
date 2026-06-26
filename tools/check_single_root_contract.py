#!/usr/bin/env python3
"""Check that release entrypoints belong to the published repository root.

Inputs:
- Published repository `Makefile`.
- Published repository GitHub workflow files.
- Optional parent scratch `Makefile` when running inside the local staging
  workspace.

Outputs:
- Exit status 0 when release commands are rooted in the published repository.
- Exit status 1 with stable diagnostics when CI or Makefile wiring points at
  scratch paths.

Transformation:
- Reads release-facing command files as text and validates that build, test,
  check, preflight, and artifact entrypoints are owned by this repository root.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MAKEFILE = ROOT / "Makefile"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
PARENT_MAKEFILE = ROOT.parent / "Makefile"
PARENT_CRATES_ROOT = ROOT.parent / "crates"
PARENT_GRAMMAR_ROOT = ROOT.parent / "docs" / "grammar"
PARENT_STD_ROOT = ROOT.parent / "std"


@dataclass(frozen=True)
class ContractDiagnostic:
    """Single-root contract diagnostic.

    Inputs:
    - `path`: repository-relative or absolute path being checked.
    - `message`: human-readable contract failure.

    Outputs:
    - Immutable diagnostic for report rendering.

    Transformation:
    - Keeps path and reason together so CI output points at the file that needs
      adjustment.
    """

    path: Path
    message: str

    def render(self) -> str:
        """Return this diagnostic as display text.

        Inputs:
        - Diagnostic path and message.

        Outputs:
        - One-line diagnostic.

        Transformation:
        - Uses repository-relative paths when the file is inside the published
          repository.
        """

        try:
            path = self.path.relative_to(ROOT)
        except ValueError:
            path = self.path
        return f"{path}: {self.message}"


def read_required(path: Path) -> tuple[str, ContractDiagnostic | None]:
    """Read a required contract file.

    Inputs:
    - `path`: file expected to exist.

    Outputs:
    - File text and no diagnostic when the file exists.
    - Empty text and a diagnostic when the file is missing.

    Transformation:
    - Converts missing files into normal checker diagnostics instead of Python
      exceptions.
    """

    if not path.is_file():
        return "", ContractDiagnostic(path, "required single-root contract file is missing")
    return path.read_text(encoding="utf-8"), None


def require_text(path: Path, text: str, needles: list[str]) -> list[ContractDiagnostic]:
    """Require substrings in a contract file.

    Inputs:
    - `path`: checked file path.
    - `text`: checked file contents.
    - `needles`: required substrings.

    Outputs:
    - Diagnostics for each missing substring.

    Transformation:
    - Performs exact text checks for release command wiring.
    """

    return [
        ContractDiagnostic(path, f"missing required release-root command `{needle}`")
        for needle in needles
        if needle not in text
    ]


def reject_text(path: Path, text: str, needles: list[str]) -> list[ContractDiagnostic]:
    """Reject substrings in a contract file.

    Inputs:
    - `path`: checked file path.
    - `text`: checked file contents.
    - `needles`: forbidden substrings.

    Outputs:
    - Diagnostics for each forbidden substring found.

    Transformation:
    - Catches workflow commands that escape the published repository root or
      refer back to the scratch staging directory.
    """

    return [
        ContractDiagnostic(path, f"forbidden scratch-root reference `{needle}`")
        for needle in needles
        if needle in text
    ]


def check_makefile() -> list[ContractDiagnostic]:
    """Validate published Makefile release entrypoints.

    Inputs:
    - Published repository `Makefile`.

    Outputs:
    - Diagnostics for missing release-facing targets.

    Transformation:
    - Ensures users and CI can invoke release commands from the published root.
    """

    text, diagnostic = read_required(MAKEFILE)
    diagnostics = [diagnostic] if diagnostic is not None else []
    diagnostics.extend(
        require_text(
            MAKEFILE,
            text,
            [
                "check:",
                "test:",
                "publish-preflight:",
                "release-artifact-linux:",
                "release-boundary-check:",
            ],
        )
    )
    return diagnostics


def check_workflows() -> list[ContractDiagnostic]:
    """Validate CI and release workflow command ownership.

    Inputs:
    - Published repository GitHub workflow files.

    Outputs:
    - Diagnostics for missing release commands or scratch-root references.

    Transformation:
    - Confirms workflows call published-root Make targets directly and do not
      use parent-directory or nested scratch path redirects.
    """

    diagnostics: list[ContractDiagnostic] = []
    workflow_requirements = {
        CI_WORKFLOW: ["run: make check", "run: make test", "run: make test-release"],
        RELEASE_WORKFLOW: [
            "run: make check",
            "run: make test",
            "run: make test-release",
            "run: make release-artifact-linux",
        ],
    }
    forbidden = ["cd terlan", "../", "working-directory: .."]
    for path, required in workflow_requirements.items():
        text, diagnostic = read_required(path)
        if diagnostic is not None:
            diagnostics.append(diagnostic)
            continue
        diagnostics.extend(require_text(path, text, required))
        diagnostics.extend(reject_text(path, text, forbidden))
    return diagnostics


def check_parent_scratch_guard() -> list[ContractDiagnostic]:
    """Validate optional local scratch Makefile guard.

    Inputs:
    - Parent workspace `Makefile`, when present.

    Outputs:
    - Diagnostic if the parent scratch Makefile exists without the release-root
      guard.

    Transformation:
    - Keeps local staging workspaces from accidentally reintroducing scratch
      release entrypoints while remaining harmless in CI, where no parent
      scratch workspace exists.
    """

    if not PARENT_MAKEFILE.is_file():
        return []
    text = PARENT_MAKEFILE.read_text(encoding="utf-8")
    if "scratch-release-root-guard" not in text:
        return [
            ContractDiagnostic(
                PARENT_MAKEFILE,
                "parent scratch Makefile must redirect release commands to terlan/",
            )
        ]
    return []


def check_parent_grammar_root() -> list[ContractDiagnostic]:
    """Validate that local scratch grammar ownership has been removed.

    Inputs:
    - Parent workspace `docs/grammar`, when present.

    Outputs:
    - Diagnostic if a duplicate scratch grammar root exists.

    Transformation:
    - Enforces the 0.0.4 C0.8 rule locally while remaining inert in CI, where
      the published repository is checked out without the scratch parent tree.
    """

    if PARENT_GRAMMAR_ROOT.exists():
        return [
            ContractDiagnostic(
                PARENT_GRAMMAR_ROOT,
                "scratch grammar root must not exist; canonical grammar lives under terlan/docs/grammar",
            )
        ]
    return []


def check_parent_crates_root() -> list[ContractDiagnostic]:
    """Validate that local scratch compiler-crate ownership has been removed.

    Inputs:
    - Parent workspace `crates`, when present.

    Outputs:
    - Diagnostic if a duplicate scratch compiler crate root exists.

    Transformation:
    - Enforces the 0.0.4 C0.2/C0.5 rule locally while remaining inert in CI,
      where the published repository is checked out without the scratch parent
      tree.
    """

    if PARENT_CRATES_ROOT.exists():
        return [
            ContractDiagnostic(
                PARENT_CRATES_ROOT,
                "scratch compiler crate root must not exist; canonical compiler crates live under terlan/crates",
            )
        ]
    return []


def check_parent_std_root() -> list[ContractDiagnostic]:
    """Validate that local scratch std ownership has been removed.

    Inputs:
    - Parent workspace `std`, when present.

    Outputs:
    - Diagnostic if a duplicate scratch std root exists.

    Transformation:
    - Enforces the 0.0.4 C0.6 rule locally while remaining inert in CI, where
      the published repository is checked out without the scratch parent tree.
    """

    if PARENT_STD_ROOT.exists():
        return [
            ContractDiagnostic(
                PARENT_STD_ROOT,
                "scratch std root must not exist; canonical std lives under terlan/std",
            )
        ]
    return []


def main() -> int:
    """Run single-root release contract validation.

    Inputs:
    - Published repository Makefile, workflows, and optional parent scratch
      Makefile.

    Outputs:
    - Exit status 0 when release command ownership is single-rooted.
    - Exit status 1 when a contract violation is found.

    Transformation:
    - Aggregates Makefile, workflow, and optional scratch guard diagnostics into
      one policy check used by `make check`.
    """

    diagnostics = []
    diagnostics.extend(check_makefile())
    diagnostics.extend(check_workflows())
    diagnostics.extend(check_parent_scratch_guard())
    diagnostics.extend(check_parent_crates_root())
    diagnostics.extend(check_parent_grammar_root())
    diagnostics.extend(check_parent_std_root())
    if diagnostics:
        print("[single-root-contract] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic.render()}")
        return 1
    print("[single-root-contract] release entrypoints are rooted in the published repository.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
