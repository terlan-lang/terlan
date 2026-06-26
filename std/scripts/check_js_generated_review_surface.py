#!/usr/bin/env python3
"""Validate the committed generated std.js review surface.

Inputs:
- `std/js/manifests/std_js_bindings.json`.
- `std/js/manifests/std_js_skipped.json`.
- Generated source, interface, summary, and test files referenced by the
  binding manifest.

Outputs:
- Exit status 0 when generated artifacts are present and carry provenance
  headers, and unsupported TypeScript declarations have stable skip records.
- Exit status 1 with stable diagnostics when a manifest reference, generated
  file, header field, or skip-record field is missing.

Transformation:
- Reads the generated binding manifest.
- Verifies referenced files exist.
- Checks generated headers for machine-readable provenance keys on binding
  artifacts and leaves normalized summary contents to the std summary drift
  checker.
- Checks skipped TypeScript declaration rows so missing std.js coverage stays
  explicit and reviewable.
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
REQUIRED_ES2015_COLLECTION_MODULES = (
    "std.js.Map",
    "std.js.MapConstructor",
    "std.js.ReadonlyMap",
    "std.js.Set",
    "std.js.SetConstructor",
    "std.js.ReadonlySet",
    "std.js.WeakMap",
    "std.js.WeakMapConstructor",
    "std.js.WeakSet",
    "std.js.WeakSetConstructor",
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

    summary = repository_path(output.get("summary"))
    if summary is None:
        diagnostics.append(
            ReviewDiagnostic(BINDINGS_MANIFEST.relative_to(ROOT), "output field `summary` must be a safe path")
        )
    elif not (ROOT / summary).is_file():
        diagnostics.append(ReviewDiagnostic(summary, "missing normalized summary artifact"))
    else:
        deps = Path(f"{summary.as_posix()}.deps")
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
    - Checks schema metadata, skipped-manifest linkage, skipped declaration
      records, and every output entry.
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
        if skipped is not None:
            diagnostics.extend(skipped_manifest_diagnostics(manifest, skipped_relative, skipped))

    outputs = manifest.get("outputs")
    if not isinstance(outputs, list) or not outputs:
        diagnostics.append(ReviewDiagnostic(manifest_path, "outputs must be a non-empty array"))
    elif isinstance(outputs, list):
        diagnostics.extend(required_module_diagnostics(outputs))
        for output in outputs:
            diagnostics.extend(output_diagnostics(output))
    return diagnostics


def skipped_manifest_diagnostics(
    manifest: dict[str, Any], skipped_relative: Path, skipped_manifest: dict[str, Any]
) -> list[ReviewDiagnostic]:
    """Validate unsupported TypeScript declaration skip records.

    Inputs:
    - `manifest`: parsed generated binding manifest.
    - `skipped_relative`: repository-relative skipped-manifest path.
    - `skipped_manifest`: parsed standalone skipped-declarations manifest.

    Outputs:
    - Diagnostics for malformed skip rows or mismatched manifest copies.

    Transformation:
    - Treats every skipped TypeScript declaration as a required review record
      with stable source, reason, and detail text.
    - Compares the binding-manifest embedded skip list with the standalone
      skip manifest so release artifacts cannot drift silently.
    """

    diagnostics: list[ReviewDiagnostic] = []
    embedded_skipped = manifest.get("skipped")
    skipped = skipped_manifest.get("skipped")

    if not isinstance(embedded_skipped, list):
        diagnostics.append(ReviewDiagnostic(BINDINGS_MANIFEST.relative_to(ROOT), "`skipped` must be an array"))
    if not isinstance(skipped, list):
        diagnostics.append(ReviewDiagnostic(skipped_relative, "`skipped` must be an array"))
        return diagnostics
    if isinstance(embedded_skipped, list) and embedded_skipped != skipped:
        diagnostics.append(
            ReviewDiagnostic(
                skipped_relative,
                "standalone skipped declarations must match binding manifest `skipped` entries",
            )
        )

    for index, item in enumerate(skipped):
        row_path = Path(f"{skipped_relative.as_posix()}#/skipped/{index}")
        if not isinstance(item, dict):
            diagnostics.append(ReviewDiagnostic(row_path, "skip row must be an object"))
            continue
        source = item.get("source")
        reason = item.get("reason")
        detail = item.get("detail")
        if not isinstance(source, str) or not source.strip():
            diagnostics.append(ReviewDiagnostic(row_path, "skip row must include non-empty `source`"))
        if not isinstance(reason, str) or not reason.strip():
            diagnostics.append(ReviewDiagnostic(row_path, "skip row must include non-empty `reason`"))
        elif not reason.startswith("ts_bindgen."):
            diagnostics.append(ReviewDiagnostic(row_path, "`reason` must use the `ts_bindgen.` namespace"))
        if not isinstance(detail, str) or not detail.strip():
            diagnostics.append(ReviewDiagnostic(row_path, "skip row must include non-empty `detail`"))
    return diagnostics


def required_module_diagnostics(outputs: list[Any]) -> list[ReviewDiagnostic]:
    """Validate required generated std.js module coverage.

    Inputs:
    - `outputs`: raw binding-manifest output entries.

    Outputs:
    - Diagnostics for required modules missing from `std_js_bindings.json`.

    Transformation:
    - Builds the generated module-name set and checks the ES2015 collection
      surface that must stay present now that `lib.es2015.collection.d.ts` is a
      pinned standard-library input.
    """

    manifest_path = BINDINGS_MANIFEST.relative_to(ROOT)
    modules = {
        output.get("module")
        for output in outputs
        if isinstance(output, dict) and isinstance(output.get("module"), str)
    }
    return [
        ReviewDiagnostic(manifest_path, f"missing required ES2015 collection module `{module}`")
        for module in REQUIRED_ES2015_COLLECTION_MODULES
        if module not in modules
    ]


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
