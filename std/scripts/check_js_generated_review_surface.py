#!/usr/bin/env python3
"""Validate the committed generated std.js review surface.

Inputs:
- `std/js/manifests/std_js_bindings.json`.
- `std/js/manifests/std_js_skipped.json`.
- Generated source, interface, summary, and test files referenced by the
  binding manifest.

Outputs:
- Exit status 0 when generated artifacts are present and carry provenance
  headers.
- Exit status 1 with stable diagnostics when a manifest reference, generated
  file, or header field is missing.

Transformation:
- Reads the generated binding manifest.
- Verifies referenced files exist.
- Checks generated headers for machine-readable provenance keys.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
BINDINGS_MANIFEST = ROOT / "std" / "js" / "manifests" / "std_js_bindings.json"
REQUIRED_GENERATED_HEADER_KEYS = (
    "@generated true",
    "@do-not-edit true",
    "@generator terlc",
    "@generator-version",
    "@generator-profile",
    "@artifact-kind",
    "@input-manifest",
    "@source-package",
    "@source-input",
    "@source-interface",
)


@dataclass(frozen=True)
class ReviewDiagnostic:
    """Generated review-surface diagnostic.

    Inputs:
    - `path`: repository-relative path related to the diagnostic.
    - `message`: human-readable issue.

    Outputs:
    - Immutable diagnostic for checker output.

    Transformation:
    - Keeps artifact identity and issue text together for stable CI messages.
    """

    path: Path
    message: str

    def render(self) -> str:
        """Return a stable diagnostic string.

        Inputs:
        - Diagnostic path and message.

        Outputs:
        - One-line diagnostic suitable for Make and CI logs.

        Transformation:
        - Converts paths to POSIX-style repository-relative text.
        """

        return f"{self.path.as_posix()}: {self.message}"


def load_json(path: Path) -> tuple[dict[str, Any] | None, list[ReviewDiagnostic]]:
    """Load a JSON object from a repository file.

    Inputs:
    - Absolute JSON file path.

    Outputs:
    - Parsed JSON object when valid.
    - Diagnostics for missing, malformed, or non-object JSON.

    Transformation:
    - Converts file and JSON failures into review-surface diagnostics.
    """

    relative = path.relative_to(ROOT)
    if not path.is_file():
        return None, [ReviewDiagnostic(relative, "missing JSON manifest")]
    try:
        parsed = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        return None, [ReviewDiagnostic(relative, f"invalid JSON: {error.msg}")]
    if not isinstance(parsed, dict):
        return None, [ReviewDiagnostic(relative, "manifest must be a JSON object")]
    return parsed, []


def repository_path(value: object) -> Path | None:
    """Return a repository-relative path from a JSON value.

    Inputs:
    - Manifest value expected to be a path string.

    Outputs:
    - `Path` when the value is a safe relative path.
    - `None` otherwise.

    Transformation:
    - Rejects absolute paths and parent-directory escapes.
    """

    if not isinstance(value, str):
        return None
    path = Path(value)
    if path.is_absolute() or ".." in path.parts:
        return None
    return path


def header_text(path: Path) -> str:
    """Return the leading generated-header text for one artifact.

    Inputs:
    - Absolute artifact path.

    Outputs:
    - First 16 lines joined as text.

    Transformation:
    - Reads only the compact header area needed by the review-surface check.
    """

    return "\n".join(path.read_text(encoding="utf-8").splitlines()[:16])


def check_generated_header(relative: Path, expected_kind: str | None) -> list[ReviewDiagnostic]:
    """Validate one generated artifact header.

    Inputs:
    - `relative`: repository-relative artifact path.
    - `expected_kind`: required `@artifact-kind` value, or `None` when only
      provenance presence is required.

    Outputs:
    - Diagnostics for missing files or missing generated-header keys.

    Transformation:
    - Supports block-comment Terlan headers and `//!` summary headers by
      searching the leading header text for required keys.
    """

    diagnostics: list[ReviewDiagnostic] = []
    absolute = ROOT / relative
    if not absolute.is_file():
        return [ReviewDiagnostic(relative, "missing generated artifact")]

    header = header_text(absolute)
    for key in REQUIRED_GENERATED_HEADER_KEYS:
        if key not in header:
            diagnostics.append(ReviewDiagnostic(relative, f"missing generated header key `{key}`"))
    if expected_kind is not None and f"@artifact-kind {expected_kind}" not in header:
        diagnostics.append(
            ReviewDiagnostic(relative, f"missing generated header kind `@artifact-kind {expected_kind}`")
        )
    return diagnostics


def output_diagnostics(output: object) -> list[ReviewDiagnostic]:
    """Validate one generated binding-manifest output entry.

    Inputs:
    - JSON output entry from `std_js_bindings.json`.

    Outputs:
    - Diagnostics for malformed entries, missing files, and invalid headers.

    Transformation:
    - Checks source, interface, summary, and test paths through the same
      generated-header contract.
    """

    if not isinstance(output, dict):
        return [ReviewDiagnostic(BINDINGS_MANIFEST.relative_to(ROOT), "output entry must be an object")]

    diagnostics: list[ReviewDiagnostic] = []
    path_fields = {
        "source": "source",
        "interface": "interface",
        "summary": None,
        "test": "test",
    }
    for field, expected_kind in path_fields.items():
        relative = repository_path(output.get(field))
        if relative is None:
            diagnostics.append(
                ReviewDiagnostic(BINDINGS_MANIFEST.relative_to(ROOT), f"output field `{field}` must be a safe path")
            )
            continue
        diagnostics.extend(check_generated_header(relative, expected_kind))

        if field == "summary":
            deps = Path(f"{relative.as_posix()}.deps")
            if not (ROOT / deps).is_file():
                diagnostics.append(ReviewDiagnostic(deps, "missing generated summary dependency file"))
    return diagnostics


def manifest_diagnostics(manifest: dict[str, Any]) -> list[ReviewDiagnostic]:
    """Validate generated binding and skipped-declaration manifests.

    Inputs:
    - Parsed binding manifest.

    Outputs:
    - Diagnostics for missing manifest fields and generated output review
      surface issues.

    Transformation:
    - Checks schema metadata, skipped-manifest linkage, and every output entry.
    """

    diagnostics: list[ReviewDiagnostic] = []
    manifest_path = BINDINGS_MANIFEST.relative_to(ROOT)
    if manifest.get("schema") != "terlan.std.js.bindings.v1":
        diagnostics.append(ReviewDiagnostic(manifest_path, "invalid or missing binding manifest schema"))
    if manifest.get("generator") != "terlc":
        diagnostics.append(ReviewDiagnostic(manifest_path, "missing generator `terlc`"))
    if not manifest.get("generator_version"):
        diagnostics.append(ReviewDiagnostic(manifest_path, "missing generator version"))
    if not manifest.get("input_manifest"):
        diagnostics.append(ReviewDiagnostic(manifest_path, "missing input manifest path"))

    skipped_relative = repository_path(manifest.get("skipped_manifest"))
    if skipped_relative is None:
        diagnostics.append(ReviewDiagnostic(manifest_path, "missing safe skipped manifest path"))
    else:
        skipped, skipped_errors = load_json(ROOT / skipped_relative)
        diagnostics.extend(skipped_errors)
        if skipped is not None and skipped.get("schema") != "terlan.std.js.skipped-declarations.v1":
            diagnostics.append(ReviewDiagnostic(skipped_relative, "invalid skipped-declarations schema"))

    outputs = manifest.get("outputs")
    if not isinstance(outputs, list) or not outputs:
        diagnostics.append(ReviewDiagnostic(manifest_path, "outputs must be a non-empty array"))
    elif isinstance(outputs, list):
        for output in outputs:
            diagnostics.extend(output_diagnostics(output))
    return diagnostics


def main() -> int:
    """Run the generated std.js review-surface check.

    Inputs:
    - Committed generated std.js manifests and artifacts.

    Outputs:
    - Exit status 0 when the review surface is complete.
    - Exit status 1 with diagnostics when generated provenance or manifest
      references are incomplete.

    Transformation:
    - Converts the T0.6 generated review-surface rule into an executable
      release check.
    """

    manifest, diagnostics = load_json(BINDINGS_MANIFEST)
    if manifest is not None:
        diagnostics.extend(manifest_diagnostics(manifest))
    if diagnostics:
        print("[std-js-review-surface] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic.render()}")
        return 1
    print("[std-js-review-surface] generated std.js review surface is complete.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
