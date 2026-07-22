#!/usr/bin/env python3
"""Validate the external Battleship app against Terlan VM packaging.

Inputs:
- `TERLAN_BATTLESHIP_ROOT`, or a conventional local Battleship checkout.
- The current Terlan compiler from `TERLC`, or `cargo run -p terlan --bin terlc`.

Outputs:
- Exit status 0 when the copied Battleship app declares a VM artifact, builds
  with the current compiler, and its generated launcher executes.
- Exit status 1 with stable diagnostics when the external app or compiler
  package path regresses.

Transformation:
- Copies the external checkout into `/tmp` so validation never writes build
  artifacts into the application repository, then runs the normal VM package
  build and launcher path against that temporary copy.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
IGNORED_DIRS = {
    ".git",
    "_build",
    "node_modules",
    "playwright-report",
    "test-results",
}
IGNORED_FILES = {
    "tsconfig.tsbuildinfo",
}


def battleship_root_candidates() -> list[Path]:
    """Return ordered candidate paths for the external Battleship checkout."""

    candidates: list[Path] = []
    if env_root := os.environ.get("TERLAN_BATTLESHIP_ROOT"):
        candidates.append(Path(env_root))
    candidates.extend(
        [
            Path("/home/anatoly/Applications/battleship"),
            Path("/Applications/battleship"),
            ROOT.parent.parent / "battleship",
        ]
    )
    return candidates


def find_battleship_root() -> Path:
    """Resolve the external Battleship checkout path or exit with a diagnostic."""

    for candidate in battleship_root_candidates():
        manifest = candidate / "terlan.toml"
        if manifest.is_file():
            return candidate
    print(
        "battleship external VM contract skipped: no Battleship checkout found; "
        "set TERLAN_BATTLESHIP_ROOT to require a specific checkout.",
        file=sys.stderr,
    )
    sys.exit(0)


def ignore_generated(_dir: str, names: list[str]) -> set[str]:
    """Return generated checkout entries excluded from the temporary copy."""

    return {name for name in names if name in IGNORED_DIRS or name in IGNORED_FILES}


def read_manifest_text(project: Path) -> str:
    """Read the external app manifest for legacy-token checks."""

    manifest_path = project / "terlan.toml"
    try:
        return manifest_path.read_text(encoding="utf-8")
    except OSError as err:
        print(f"battleship contract failed: cannot read {manifest_path}: {err}", file=sys.stderr)
        sys.exit(1)


def validate_manifest(project: Path) -> None:
    """Reject legacy manifest text before the Rust compiler parses TOML."""

    manifest_text = read_manifest_text(project)
    if "beam-thin" in manifest_text:
        print("battleship contract failed: manifest still references beam-thin", file=sys.stderr)
        sys.exit(1)


def terlc_command() -> list[str]:
    """Return the compiler command used by the external app gate."""

    if terlc := os.environ.get("TERLC"):
        return [terlc]
    return ["cargo", "run", "-p", "terlan", "--bin", "terlc", "--"]


def run_checked(command: list[str], cwd: Path) -> None:
    """Run one command and render captured output when it fails."""

    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    if result.returncode == 0:
        return
    print(f"battleship contract failed: command exited {result.returncode}: {' '.join(command)}", file=sys.stderr)
    if result.stdout:
        print(result.stdout, file=sys.stderr, end="" if result.stdout.endswith("\n") else "\n")
    if result.stderr:
        print(result.stderr, file=sys.stderr, end="" if result.stderr.endswith("\n") else "\n")
    sys.exit(result.returncode)


def validate_build_outputs(build_dir: Path) -> Path:
    """Validate the VM package bundle and return the generated launcher path."""

    launcher = build_dir / "bin" / "battleship"
    if sys.platform.startswith("win"):
        launcher = build_dir / "bin" / "battleship.cmd"
    required = [
        build_dir / "bin" / ("terlan-vm.exe" if sys.platform.startswith("win") else "terlan-vm"),
        build_dir / "bin" / ("terlan-native-worker.exe" if sys.platform.startswith("win") else "terlan-native-worker"),
        build_dir / "terlan-package-build.json",
        build_dir / "vm" / "battleship_Main.tvm",
        launcher,
    ]
    missing = [path for path in required if not path.is_file()]
    if missing:
        rendered = ", ".join(str(path) for path in missing)
        print(f"battleship contract failed: missing build output(s): {rendered}", file=sys.stderr)
        sys.exit(1)
    if (build_dir / "ebin").exists():
        print("battleship contract failed: package emitted legacy ebin output", file=sys.stderr)
        sys.exit(1)
    return launcher


def main() -> int:
    """Run the external Battleship VM contract gate."""

    external_root = find_battleship_root()
    with tempfile.TemporaryDirectory(prefix="terlan-battleship-vm-contract.") as tmp:
        project = Path(tmp) / "battleship"
        shutil.copytree(external_root, project, ignore=ignore_generated)
        validate_manifest(project)
        build_dir = project / "_terlan_vm_contract_build"
        run_checked(
            terlc_command() + ["build", str(project), "--out-dir", str(build_dir)],
            ROOT,
        )
        launcher = validate_build_outputs(build_dir)
        run_checked([str(launcher)], ROOT)
    print("battleship external VM contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
