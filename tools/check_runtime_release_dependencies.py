#!/usr/bin/env python3
"""Check that release runtime dependency crates are committed.

Inputs:
- `Cargo.lock`, or a custom lockfile passed with `--lockfile`.
- Owning crate manifests for the live runtime dependency contract.

Outputs:
- Exit status 0 when every required runtime dependency is present.
- Exit status 1 with one diagnostic per missing dependency when any required
  crate is absent from the lockfile or not declared by its owning crate.
- Exit status 1 when the dependency is only present transitively or in a local
  registry cache, because release runtime crates must be directly owned by the
  crate that uses them and resolved into `Cargo.lock`.

Transformation:
- Parses Cargo lockfile package names and crate manifest dependency names, then
  compares both against the runtime crate contract required before 0.0.5 can
  ship live Postgres, HTTPS, and crate-backed HTTP runtime hardening. Local
  Cargo registry source copies are intentionally ignored; only checked-in
  manifests plus the committed lockfile define the release dependency set.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LOCKFILE = ROOT / "Cargo.lock"
CLI_MANIFEST = ROOT / "crates" / "terlan" / "Cargo.toml"


@dataclass(frozen=True)
class RequiredRuntimeDependency:
    """Required runtime dependency contract row.

    Inputs:
    - `name`: Cargo package name that must appear in `Cargo.lock`.
    - `feature`: release runtime feature that requires the package.
    - `reason`: human-readable implementation responsibility.
    - `manifest`: owning crate manifest that must declare the dependency
      directly.

    Outputs:
    - Immutable dependency requirement used by validation and diagnostics.

    Transformation:
    - Keeps the crate name, feature owner, and user-facing reason together so
      the release gate cannot drift into anonymous grep checks.
    """

    name: str
    feature: str
    reason: str
    manifest: Path

    def missing_lockfile_message(self) -> str:
        """Render the missing-lockfile diagnostic.

        Inputs:
        - Dependency contract row.

        Outputs:
        - Stable diagnostic for a crate absent from `Cargo.lock`.

        Transformation:
        - Formats the exact message consumed by humans and release logs.
        """

        return (
            "runtime-release-dependency-check missing: "
            f"{self.name} for {self.reason}; dependency must be resolved in Cargo.lock."
        )

    def missing_manifest_message(self) -> str:
        """Render the missing-manifest diagnostic.

        Inputs:
        - Dependency contract row.

        Outputs:
        - Stable diagnostic for a crate absent from its owning manifest.

        Transformation:
        - Formats the owning manifest path relative to the repository so the
          release fix is actionable.
        """

        manifest = self.manifest.relative_to(ROOT)
        return (
            "runtime-release-dependency-check missing: "
            f"{self.name} is not declared directly in {manifest} for {self.reason}."
        )


REQUIRED_DEPENDENCIES = (
    RequiredRuntimeDependency(
        name="tokio-postgres",
        feature="postgres",
        reason="live std.db.Postgres client I/O",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="deadpool-postgres",
        feature="postgres",
        reason="live std.db.Postgres pooling",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="refinery",
        feature="postgres",
        reason="terlc db migrate/status/rebuild",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="rustls",
        feature="https",
        reason="live HTTPS serving",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="tokio-rustls",
        feature="https",
        reason="live HTTPS serving",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="instant-acme",
        feature="https",
        reason="public ACME certificate management",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="rcgen",
        feature="https",
        reason="internal/local CA certificate generation",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="cookie",
        feature="http-runtime-hardening",
        reason="std.http.Cookies parsing and Set-Cookie serialization",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="mime_guess",
        feature="http-runtime-hardening",
        reason="static asset content-type detection",
        manifest=CLI_MANIFEST,
    ),
    RequiredRuntimeDependency(
        name="notify",
        feature="http-runtime-hardening",
        reason="live-reload asset watching",
        manifest=CLI_MANIFEST,
    ),
)

FEATURE_COMPLETION_CRITERIA = {
    "postgres": ("2", "9", "10", "11"),
    "https": ("5",),
}

RELEASE_GATE_COMPLETION_CRITERION = "21"


def parse_lockfile_package_names(text: str) -> set[str]:
    """Extract Cargo package names from lockfile text.

    Inputs:
    - `text`: Cargo.lock contents.

    Outputs:
    - Set of package names declared by `[[package]] name = "..."`
      entries.

    Transformation:
    - Uses a lockfile-line regex because the gate only needs package presence,
      not full TOML semantic interpretation.
    """

    return set(re.findall(r'^name = "([^"]+)"$', text, flags=re.MULTILINE))


def missing_dependencies(package_names: set[str]) -> list[RequiredRuntimeDependency]:
    """Return required dependencies that are absent from a package set.

    Inputs:
    - `package_names`: package names parsed from a lockfile.

    Outputs:
    - Dependency contract rows missing from the supplied package set.

    Transformation:
    - Compares the release runtime contract against parsed lockfile package
      names while preserving the contract ordering used in diagnostics.
    """

    return [
        dependency
        for dependency in REQUIRED_DEPENDENCIES
        if dependency.name not in package_names
    ]


def parse_manifest_dependency_names(text: str) -> set[str]:
    """Extract direct dependency names from a Cargo manifest.

    Inputs:
    - `text`: Cargo.toml contents.

    Outputs:
    - Set of direct dependency names declared in the `[dependencies]` section.

    Transformation:
    - Scans only the normal dependency section used by current runtime crates.
      Dev-dependencies are intentionally excluded because runtime stacks must
      be available to release builds. Inline-table dependencies that use
      `package = "crate-name"` contribute both the local dependency key and the
      resolved package name so Cargo aliases do not create false release-gate
      failures. Table-style dependencies such as `[dependencies.foo]` and
      target-scoped runtime dependency sections are also counted as direct
      dependencies.
    """

    names: set[str] = set()
    in_dependencies = False
    in_dependency_table = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            table_name = stripped[1:-1]
            in_dependencies = False
            in_dependency_table = False
            if table_name == "dependencies" or table_name.endswith(".dependencies"):
                in_dependencies = True
                continue
            table_match = re.match(
                r'^(?:target\..*\.dependencies|dependencies)\.("?)([^"\]]+)\1$',
                table_name,
            )
            if table_match:
                names.add(table_match.group(2))
                in_dependency_table = True
            continue
        if not (in_dependencies or in_dependency_table) or not stripped or stripped.startswith("#"):
            continue
        if in_dependencies:
            match = re.match(r'^([A-Za-z0-9_.-]+)\s*=', stripped)
            if match:
                names.add(match.group(1))
        package_match = re.search(r'package\s*=\s*"([^"]+)"', stripped)
        if package_match:
            names.add(package_match.group(1))
    return names


def missing_manifest_dependencies(
    manifest_names: dict[Path, set[str]],
) -> list[RequiredRuntimeDependency]:
    """Return dependencies absent from their owning manifests.

    Inputs:
    - `manifest_names`: map from manifest path to direct dependency names.

    Outputs:
    - Dependency contract rows missing from their owning manifest.

    Transformation:
    - Compares the release runtime contract against direct dependencies for the
      crate that owns each runtime feature.
    """

    return [
        dependency
        for dependency in REQUIRED_DEPENDENCIES
        if dependency.name not in manifest_names.get(dependency.manifest, set())
    ]


def open_feature_gate_message(
    dependencies: list[RequiredRuntimeDependency],
) -> str | None:
    """Render release feature gates opened by missing dependencies.

    Inputs:
    - `dependencies`: runtime dependency contract rows missing from either the
      lockfile or an owning manifest.

    Outputs:
    - `None` when no dependency rows are missing.
    - Stable diagnostic listing open feature gate names otherwise.

    Transformation:
    - Deduplicates feature names in the order declared by
      `REQUIRED_DEPENDENCIES` so release logs show whether the current failure
      keeps Postgres, HTTPS, HTTP runtime hardening, or a combination of those
      gates open.
    """

    if not dependencies:
        return None

    missing_names = {dependency.name for dependency in dependencies}
    groups: list[str] = []
    for dependency in REQUIRED_DEPENDENCIES:
        if dependency.name in missing_names and dependency.feature not in groups:
            groups.append(dependency.feature)

    return (
        "runtime-release-dependency-check open feature gates: "
        f"{', '.join(groups)}."
    )


def open_completion_gate_message(
    dependencies: list[RequiredRuntimeDependency],
) -> str | None:
    """Render the 0.0.5 completion gates opened by missing dependencies.

    Inputs:
    - `dependencies`: runtime dependency contract rows missing from either the
      lockfile or an owning manifest.

    Outputs:
    - `None` when no dependency rows are missing.
    - Stable diagnostic listing the 0.0.5 completion-criteria numbers whose
      gates remain open because runtime dependency contracts are missing.

    Transformation:
    - Maps missing feature groups to the roadmap criteria they keep open and
      always includes the final publish-preflight criterion when any runtime
      group is missing.
    """

    if not dependencies:
        return None

    missing_features = {
        dependency.feature
        for dependency in dependencies
    }
    criteria: set[str] = set()
    for dependency in REQUIRED_DEPENDENCIES:
        if dependency.feature not in missing_features:
            continue
        for criterion in FEATURE_COMPLETION_CRITERIA.get(dependency.feature, ()):
            criteria.add(criterion)
    criteria.add(RELEASE_GATE_COMPLETION_CRITERION)
    ordered_criteria = sorted(criteria, key=int)

    return (
        "runtime-release-dependency-check open completion gates: "
        f"{', '.join(ordered_criteria)}."
    )


def next_action_messages(
    lockfile_missing: list[RequiredRuntimeDependency],
    manifest_missing: list[RequiredRuntimeDependency],
) -> list[str]:
    """Render grouped fix guidance for missing runtime dependencies.

    Inputs:
    - `lockfile_missing`: required dependency rows absent from `Cargo.lock`.
    - `manifest_missing`: required dependency rows absent from their owning
      crate manifests.

    Outputs:
    - Stable human-facing next-action diagnostics; empty when no dependency
      rows are missing.

    Transformation:
    - Groups manifest fixes by owning `Cargo.toml` and lockfile fixes by crate
      name so release logs point at the exact source files and lockfile update
      step instead of ending on a generic Make failure.
    """

    messages: list[str] = []
    if manifest_missing:
        by_manifest: dict[Path, list[str]] = {}
        for dependency in manifest_missing:
            by_manifest.setdefault(dependency.manifest, []).append(dependency.name)
        for manifest, names in sorted(by_manifest.items(), key=lambda item: str(item[0])):
            relative_manifest = manifest.relative_to(ROOT)
            messages.append(
                "runtime-release-dependency-check next action: add "
                f"{', '.join(sorted(names))} directly to {relative_manifest}."
            )
    if lockfile_missing:
        names = sorted({dependency.name for dependency in lockfile_missing})
        messages.append(
            "runtime-release-dependency-check next action: resolve "
            f"{', '.join(names)} into Cargo.lock with the selected crate versions."
        )
    return messages


def read_text(path: Path) -> str:
    """Read a source artifact as UTF-8 text.

    Inputs:
    - `path`: file path selected by the checker.

    Outputs:
    - File contents.

    Transformation:
    - Performs explicit UTF-8 reading so diagnostics stay deterministic across
      platforms.
    """

    return path.read_text(encoding="utf-8")


def check_lockfile(path: Path) -> list[str]:
    """Validate release runtime dependency declarations.

    Inputs:
    - `path`: Cargo lockfile path.

    Outputs:
    - Diagnostic messages for missing lockfile or manifest dependencies; empty
      when valid.

    Transformation:
    - Reads package names from the lockfile and direct dependency names from
      owning manifests, then renders stable diagnostics for each missing
      required runtime dependency.
    """

    package_names = parse_lockfile_package_names(read_text(path))
    manifest_names = {
        manifest: parse_manifest_dependency_names(read_text(manifest))
        for manifest in sorted({dependency.manifest for dependency in REQUIRED_DEPENDENCIES})
    }
    lockfile_missing = missing_dependencies(package_names)
    manifest_missing = missing_manifest_dependencies(manifest_names)
    diagnostics = [
        dependency.missing_lockfile_message() for dependency in lockfile_missing
    ] + [
        dependency.missing_manifest_message()
        for dependency in manifest_missing
    ]
    open_gate_message = open_feature_gate_message(lockfile_missing + manifest_missing)
    if open_gate_message is not None:
        diagnostics.append(open_gate_message)
    criteria_message = open_completion_gate_message(lockfile_missing + manifest_missing)
    if criteria_message is not None:
        diagnostics.append(criteria_message)
    diagnostics.extend(next_action_messages(lockfile_missing, manifest_missing))
    return diagnostics


def run_self_test() -> None:
    """Run focused contract self-tests.

    Inputs:
    - Embedded synthetic lockfile snippets.

    Outputs:
    - Raises `AssertionError` on regression; otherwise returns `None`.

    Transformation:
    - Verifies lockfile parsing, manifest parsing, missing-dependency
      selection, and diagnostic rendering without needing network access or
      mutating the real lockfile. This deliberately proves that local Cargo
      cache presence is irrelevant unless the dependency is declared directly
      and resolved into the checked lockfile.
    """

    complete_lockfile = "\n".join(
        f'[[package]]\nname = "{dependency.name}"\nversion = "0.0.0"'
        for dependency in REQUIRED_DEPENDENCIES
    )
    complete_names = parse_lockfile_package_names(complete_lockfile)
    assert missing_dependencies(complete_names) == []

    complete_manifest = "\n".join(
        ["[dependencies]"] + [f'{dependency.name} = "0.0.0"' for dependency in REQUIRED_DEPENDENCIES]
    )
    manifest_names = parse_manifest_dependency_names(complete_manifest)
    assert missing_manifest_dependencies(
        {CLI_MANIFEST: manifest_names}
    ) == []
    aliased_manifest = """
[dependencies]
postgres_client = { package = "tokio-postgres", version = "0.0.0" }
"""
    aliased_manifest_names = parse_manifest_dependency_names(aliased_manifest)
    assert "postgres_client" in aliased_manifest_names
    assert "tokio-postgres" in aliased_manifest_names
    table_manifest = """
[dependencies.postgres_pool]
package = "deadpool-postgres"
version = "0.0.0"
"""
    table_manifest_names = parse_manifest_dependency_names(table_manifest)
    assert "postgres_pool" in table_manifest_names
    assert "deadpool-postgres" in table_manifest_names
    target_manifest = """
[target.'cfg(unix)'.dependencies]
notify = "0.0.0"

[target.'cfg(unix)'.dependencies.cookie_impl]
package = "cookie"
version = "0.0.0"
"""
    target_manifest_names = parse_manifest_dependency_names(target_manifest)
    assert "notify" in target_manifest_names
    assert "cookie_impl" in target_manifest_names
    assert "cookie" in target_manifest_names

    partial_names = {"tokio-postgres", "rustls"}
    missing = missing_dependencies(partial_names)
    missing_names = [dependency.name for dependency in missing]
    assert "deadpool-postgres" in missing_names
    assert "tokio-postgres" not in missing_names
    assert "rustls" not in missing_names
    assert missing[0].missing_lockfile_message().startswith(
        "runtime-release-dependency-check missing:"
    )
    assert "Cargo.lock" in missing[0].missing_lockfile_message()

    partial_manifest_names = {
        CLI_MANIFEST: {"tokio-postgres", "rustls"},
    }
    manifest_missing = missing_manifest_dependencies(partial_manifest_names)
    manifest_missing_names = [dependency.name for dependency in manifest_missing]
    assert "deadpool-postgres" in manifest_missing_names
    assert "tokio-postgres" not in manifest_missing_names
    assert manifest_missing[0].missing_manifest_message().startswith(
        "runtime-release-dependency-check missing:"
    )
    assert "declared directly" in manifest_missing[0].missing_manifest_message()

    assert (
        open_feature_gate_message(missing + manifest_missing)
        == "runtime-release-dependency-check open feature gates: postgres, https, http-runtime-hardening."
    )
    assert open_feature_gate_message([]) is None
    assert (
        open_completion_gate_message(missing + manifest_missing)
        == "runtime-release-dependency-check open completion gates: 2, 5, 9, 10, 11, 21."
    )
    assert (
        open_completion_gate_message([REQUIRED_DEPENDENCIES[3]])
        == "runtime-release-dependency-check open completion gates: 5, 21."
    )
    assert open_completion_gate_message([]) is None
    next_actions = next_action_messages(
        [REQUIRED_DEPENDENCIES[0], REQUIRED_DEPENDENCIES[3]],
        [REQUIRED_DEPENDENCIES[1], REQUIRED_DEPENDENCIES[2]],
    )
    assert next_actions == [
        "runtime-release-dependency-check next action: add deadpool-postgres, refinery directly to crates/terlan/Cargo.toml.",
        "runtime-release-dependency-check next action: resolve rustls, tokio-postgres into Cargo.lock with the selected crate versions.",
    ]


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse command-line arguments.

    Inputs:
    - `argv`: argument vector without the executable name.

    Outputs:
    - Parsed namespace containing `lockfile` and `self_test`.

    Transformation:
    - Builds the small validation CLI used by Make and focused self-tests.
    """

    parser = argparse.ArgumentParser(
        description="Validate committed live Postgres/TLS runtime dependencies."
    )
    parser.add_argument(
        "--lockfile",
        type=Path,
        default=DEFAULT_LOCKFILE,
        help="Cargo.lock path to validate.",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run the checker self-test instead of validating Cargo.lock.",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Execute the runtime dependency checker.

    Inputs:
    - `argv`: argument vector without the executable name.

    Outputs:
    - Process exit code.

    Transformation:
    - Runs self-tests when requested, otherwise prints missing dependency
      diagnostics and returns a dependency-contract status code. Open runtime
      feature gates are reported explicitly by `check_lockfile`; this final
      summary deliberately avoids broad release-blocker wording.
    """

    args = parse_args(argv)
    if args.self_test:
        run_self_test()
        return 0

    diagnostics = check_lockfile(args.lockfile)
    for diagnostic in diagnostics:
        print(diagnostic)
    if diagnostics:
        print(
            "runtime-release-dependency-check failed: resolve the missing "
            "runtime dependency contract rows listed above."
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
