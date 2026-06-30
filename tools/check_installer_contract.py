#!/usr/bin/env python3
"""Validate Terlan installer platform selection and release contract.

Inputs:
- `install.sh`: POSIX installer for Linux and macOS.
- `install.ps1`: PowerShell installer for Windows.
- `tools/package_release_artifact.py`: release artifact naming helper.

Outputs:
- Exit status 0 when installer mapping and user-facing defaults are stable.
- Exit status 1 with diagnostics when an installer contract drifts.

Transformation:
- Executes `install.sh` in dry-run mode for supported Unix platform mappings.
- Reads the PowerShell installer to validate Windows artifact and dry-run
  support without requiring PowerShell on Linux CI.
- Executes the release artifact helper in describe mode to ensure installer and
  packager artifact names stay aligned.
"""

from __future__ import annotations

import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INSTALL_SH = ROOT / "install.sh"
INSTALL_PS1 = ROOT / "install.ps1"
PACKAGE_HELPER = ROOT / "tools" / "package_release_artifact.py"


@dataclass(frozen=True)
class InstallerDiagnostic:
    """Installer contract diagnostic.

    Inputs:
    - `path`: installer file or repository file being checked.
    - `message`: stable diagnostic text.

    Outputs:
    - Immutable diagnostic rendered for CI.

    Transformation:
    - Keeps file ownership attached to the contract failure.
    """

    path: Path
    message: str

    def render(self) -> str:
        """Render this diagnostic as a repository-relative line."""

        try:
            relative = self.path.relative_to(ROOT)
        except ValueError:
            relative = self.path
        return f"{relative}: {self.message}"


def parse_key_values(text: str) -> dict[str, str]:
    """Parse dry-run `key=value` output.

    Inputs:
    - `text`: installer dry-run output.

    Outputs:
    - Mapping of output keys to values.

    Transformation:
    - Ignores non-assignment lines so diagnostics can include shell warnings
      without breaking all parsing.
    """

    values: dict[str, str] = {}
    for line in text.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key] = value
    return values


def run_install_sh_dry_run(os_name: str, arch: str) -> tuple[dict[str, str], str | None]:
    """Run `install.sh` in dry-run mode for one platform.

    Inputs:
    - `os_name`: value exposed through `TERLAN_INSTALL_OS`.
    - `arch`: value exposed through `TERLAN_INSTALL_ARCH`.

    Outputs:
    - Parsed dry-run key/value output.
    - Optional stderr/stdout diagnostic when the script fails.

    Transformation:
    - Uses environment overrides instead of mocking `uname`, keeping the
      installer test deterministic on Linux CI.
    """

    env = os.environ.copy()
    env.update(
        {
            "TERLAN_INSTALL_DRY_RUN": "1",
            "TERLAN_INSTALL_OS": os_name,
            "TERLAN_INSTALL_ARCH": arch,
            "TERLAN_VERSION": "v9.9.9",
            "TERLAN_INSTALL_DIR": "/tmp/terlan-bin",
            "TERLAN_RELEASE_BASE_URL": "https://example.invalid/releases",
        }
    )
    result = subprocess.run(
        ["sh", str(INSTALL_SH)],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return {}, result.stdout + result.stderr
    return parse_key_values(result.stdout), None


def run_package_helper_describe(os_name: str, arch: str) -> tuple[dict[str, str], str | None]:
    """Run the release artifact helper in describe mode for one platform.

    Inputs:
    - `os_name`: value exposed through `TERLAN_RELEASE_OS`.
    - `arch`: value exposed through `TERLAN_RELEASE_ARCH`.

    Outputs:
    - Parsed helper key/value output.
    - Optional diagnostic text when the helper fails.

    Transformation:
    - Compares release packaging names without requiring a compiled platform
      binary.
    """

    env = os.environ.copy()
    env.update(
        {
            "TERLAN_RELEASE_OS": os_name,
            "TERLAN_RELEASE_ARCH": arch,
        }
    )
    result = subprocess.run(
        [sys.executable, "-B", str(PACKAGE_HELPER), "describe"],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return {}, result.stdout + result.stderr
    return parse_key_values(result.stdout), None


def check_install_sh() -> list[InstallerDiagnostic]:
    """Validate POSIX installer platform mapping.

    Inputs:
    - `install.sh`.

    Outputs:
    - Diagnostics for missing file, failed dry runs, or artifact drift.

    Transformation:
    - Verifies Linux/macOS and x86_64/aarch64 artifacts are selected from the
      current platform mapping.
    """

    diagnostics: list[InstallerDiagnostic] = []
    if not INSTALL_SH.is_file():
        return [InstallerDiagnostic(INSTALL_SH, "install.sh is missing")]

    cases = [
        ("Linux", "x86_64", "linux", "x86_64", "terlc-linux-x86_64.tar.gz"),
        ("Linux", "aarch64", "linux", "aarch64", "terlc-linux-aarch64.tar.gz"),
        ("Darwin", "x86_64", "macos", "x86_64", "terlc-macos-x86_64.tar.gz"),
        ("Darwin", "arm64", "macos", "aarch64", "terlc-macos-aarch64.tar.gz"),
    ]
    for os_name, arch, expected_os, expected_arch, expected_artifact in cases:
        values, error = run_install_sh_dry_run(os_name, arch)
        label = f"{os_name}/{arch}"
        if error is not None:
            diagnostics.append(InstallerDiagnostic(INSTALL_SH, f"{label} dry-run failed: {error.strip()}"))
            continue
        expected = {
            "version": "v9.9.9",
            "os": expected_os,
            "arch": expected_arch,
            "artifact": expected_artifact,
            "url": f"https://example.invalid/releases/v9.9.9/{expected_artifact}",
            "install_dir": "/tmp/terlan-bin",
            "lib_dir": "/tmp/lib/terlan",
        }
        for key, expected_value in expected.items():
            actual = values.get(key)
            if actual != expected_value:
                diagnostics.append(
                    InstallerDiagnostic(
                        INSTALL_SH,
                        f"{label} expected {key}={expected_value}, found {actual!r}",
                    )
                )
    return diagnostics


def check_package_helper_mapping() -> list[InstallerDiagnostic]:
    """Validate release packager artifact mapping.

    Inputs:
    - `tools/package_release_artifact.py`.

    Outputs:
    - Diagnostics for missing helper or artifact-name drift.

    Transformation:
    - Mirrors the installer platform cases and adds Windows so all public
      installer artifact names have a packaging contract.
    """

    if not PACKAGE_HELPER.is_file():
        return [InstallerDiagnostic(PACKAGE_HELPER, "release artifact helper is missing")]
    diagnostics: list[InstallerDiagnostic] = []
    cases = [
        ("Linux", "x86_64", "linux", "x86_64", "terlc-linux-x86_64.tar.gz", "terlc"),
        ("Linux", "aarch64", "linux", "aarch64", "terlc-linux-aarch64.tar.gz", "terlc"),
        ("Darwin", "x86_64", "macos", "x86_64", "terlc-macos-x86_64.tar.gz", "terlc"),
        ("Darwin", "arm64", "macos", "aarch64", "terlc-macos-aarch64.tar.gz", "terlc"),
        ("Windows", "AMD64", "windows", "x86_64", "terlc-windows-x86_64.zip", "terlc.exe"),
    ]
    for os_name, arch, expected_os, expected_arch, expected_artifact, expected_binary in cases:
        values, error = run_package_helper_describe(os_name, arch)
        label = f"{os_name}/{arch}"
        if error is not None:
            diagnostics.append(
                InstallerDiagnostic(PACKAGE_HELPER, f"{label} describe failed: {error.strip()}")
            )
            continue
        expected = {
            "os": expected_os,
            "arch": expected_arch,
            "artifact": expected_artifact,
            "binary": expected_binary,
            "experimental_vm": "true",
        }
        for key, expected_value in expected.items():
            actual = values.get(key)
            if actual != expected_value:
                diagnostics.append(
                    InstallerDiagnostic(
                        PACKAGE_HELPER,
                        f"{label} expected {key}={expected_value}, found {actual!r}",
                    )
                )
    return diagnostics


def check_install_ps1() -> list[InstallerDiagnostic]:
    """Validate Windows installer static contract.

    Inputs:
    - `install.ps1`.

    Outputs:
    - Diagnostics for missing required Windows installer behavior.

    Transformation:
    - Uses source checks because Linux CI should not require PowerShell.
    """

    if not INSTALL_PS1.is_file():
        return [InstallerDiagnostic(INSTALL_PS1, "install.ps1 is missing")]
    text = INSTALL_PS1.read_text(encoding="utf-8")
    required = [
        'Version = "v0.0.5"',
        'terlc-windows-$terlanArch.zip',
        "Invoke-WebRequest",
        "Expand-Archive",
        "TERLAN_INSTALL_DRY_RUN",
        "TERLAN_INSTALL_LIB_DIR",
        "experimental\\terlan-vm",
        "--experimental",
        "otp-runtime",
        "terlc.exe",
        "--version",
    ]
    return [
        InstallerDiagnostic(INSTALL_PS1, f"missing required installer text `{needle}`")
        for needle in required
        if needle not in text
    ]


def main() -> int:
    """Run installer contract checks."""

    diagnostics = check_install_sh()
    diagnostics.extend(check_package_helper_mapping())
    diagnostics.extend(check_install_ps1())
    if diagnostics:
        for diagnostic in diagnostics:
            print(diagnostic.render(), file=sys.stderr)
        return 1
    print("Installer contract checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
