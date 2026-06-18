#!/usr/bin/env python3
"""Check that Oxc stays behind JavaScript-owned compiler boundaries.

Inputs:
- Rust source files under non-JavaScript compiler crates and validation modules.
- Cargo manifests under `crates/`.

Outputs:
- Exit status 0 when Oxc symbols and dependencies stay inside approved
  JavaScript backend or binding-generator ownership boundaries.
- Exit status 1 with file-specific diagnostics when Oxc leaks into syntax,
  HIR, typecheck, CoreIR, Erlang, validation, or unrelated crates.

Transformation:
- Scans forbidden source roots for Oxc-related identifiers.
- Scans crate manifests for Oxc dependencies outside approved crates.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
OX_CLASS_RE = re.compile(r"\b(?:Oxc|oxc_|oxc::)")
OX_DEP_RE = re.compile(r"^\s*oxc[-_A-Za-z0-9]*\s*=", re.MULTILINE)
FORBIDDEN_SOURCE_ROOTS = (
    Path("crates/terlan_syntax"),
    Path("crates/terlan_hir"),
    Path("crates/terlan_typeck"),
    Path("crates/terlan_erlang"),
    Path("crates/terlan_html"),
    Path("crates/terlan_lsp"),
    Path("crates/terlan_safenative"),
    Path("crates/terlan_cli/src/validation"),
)
APPROVED_OXC_DEP_CRATES = {
    Path("crates/terlan_cli/Cargo.toml"),
}


@dataclass(frozen=True)
class Finding:
    """Oxc boundary violation discovered by the checker.

    Inputs:
    - `path`: repository-relative file path.
    - `line`: optional one-based source line number.
    - `message`: violation message.

    Outputs:
    - Immutable diagnostic record.

    Transformation:
    - Keeps file identity and explanation together for readable checker output.
    """

    path: Path
    line: int | None
    message: str

    def render(self) -> str:
        """Return a human-readable diagnostic line.

        Inputs:
        - Finding path, optional line, and message.

        Outputs:
        - Stable diagnostic text.

        Transformation:
        - Formats source findings as `path:line: message` and manifest findings
          as `path: message`.
        """

        if self.line is None:
            return f"{self.path}: {self.message}"
        return f"{self.path}:{self.line}: {self.message}"


def forbidden_source_files() -> list[Path]:
    """Return Rust source files in Oxc-forbidden compiler areas.

    Inputs:
    - Configured forbidden source roots.

    Outputs:
    - Sorted repository-relative Rust source paths.

    Transformation:
    - Recursively scans only roots that exist in the current checkout.
    """

    files: list[Path] = []
    for root in FORBIDDEN_SOURCE_ROOTS:
        absolute = ROOT / root
        if not absolute.exists():
            continue
        for path in sorted(absolute.rglob("*.rs")):
            files.append(path.relative_to(ROOT))
    return files


def source_findings() -> list[Finding]:
    """Return Oxc symbol findings in forbidden Rust source files.

    Inputs:
    - Forbidden Rust source files.

    Outputs:
    - Finding records for each Oxc symbol occurrence.

    Transformation:
    - Searches line by line so diagnostics can point to the exact leak.
    """

    findings: list[Finding] = []
    for relative in forbidden_source_files():
        text = (ROOT / relative).read_text(encoding="utf-8")
        for line_no, line in enumerate(text.splitlines(), 1):
            if OX_CLASS_RE.search(line):
                findings.append(
                    Finding(
                        path=relative,
                        line=line_no,
                        message="Oxc symbol is outside JS backend or binding-generator ownership",
                    )
                )
    return findings


def manifest_findings() -> list[Finding]:
    """Return Oxc dependency findings outside approved crate manifests.

    Inputs:
    - Cargo manifests under `crates/`.

    Outputs:
    - Finding records for disallowed Oxc dependencies.

    Transformation:
    - Allows Oxc dependencies only in explicitly approved crates.
    """

    findings: list[Finding] = []
    for manifest in sorted(CRATES.glob("*/Cargo.toml")):
        relative = manifest.relative_to(ROOT)
        if relative in APPROVED_OXC_DEP_CRATES:
            continue
        text = manifest.read_text(encoding="utf-8")
        if OX_DEP_RE.search(text):
            findings.append(
                Finding(
                    path=relative,
                    line=None,
                    message="Oxc dependency must stay in approved JS backend/bindgen crates",
                )
            )
    return findings


def check_boundary() -> list[Finding]:
    """Return all current Oxc boundary findings.

    Inputs:
    - Forbidden Rust source roots and crate manifests.

    Outputs:
    - Combined finding records.

    Transformation:
    - Concatenates source-symbol findings with dependency findings.
    """

    return source_findings() + manifest_findings()


def main() -> int:
    """Run the Oxc boundary checker.

    Inputs:
    - Current repository source tree.

    Outputs:
    - Exit status 0 when no Oxc boundary findings exist.
    - Exit status 1 with diagnostics when Oxc leaks outside approved ownership.

    Transformation:
    - Prints stable policy-check output for Make and CI.
    """

    findings = check_boundary()
    if findings:
        print("oxc-boundary-check failed:")
        for finding in findings:
            print(f"  - {finding.render()}")
        return 1
    print("[oxc-boundary] Oxc is confined to JS backend and binding-generator ownership.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
