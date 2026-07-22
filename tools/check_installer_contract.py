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
import hashlib
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass
from pathlib import Path

from release_promotion_pipeline import PromotionError, validate_repository_contract


ROOT = Path(__file__).resolve().parents[1]
INSTALL_SH = ROOT / "install.sh"
INSTALL_PS1 = ROOT / "install.ps1"
PACKAGE_HELPER = ROOT / "tools" / "package_release_artifact.py"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
WORKFLOWS_DOC = ROOT / ".github" / "WORKFLOWS.md"
MAKEFILE = ROOT / "Makefile"
PUBLISH_FROM_DIST = ROOT / "scripts" / "publish_release_from_dist.sh"
REQUIRED_RELEASE_ARTIFACTS = {
    "terlc-linux-x86_64.tar.gz",
    "terlc-linux-aarch64.tar.gz",
    "terlc-macos-x86_64.tar.gz",
    "terlc-macos-aarch64.tar.gz",
    "terlc-windows-x86_64.zip",
    "terlc-windows-aarch64.zip",
}


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


def write_fake_unix_artifact(
    release_dir: Path, artifact: str, compiler_exit: int = 0
) -> None:
    """Write a fake Unix release artifact for installer smoke tests.

    Inputs:
    - `release_dir`: directory representing one release tag.
    - `artifact`: artifact filename expected by `install.sh`.

    Outputs:
    - A `.tar.gz` artifact containing executable compiler, VM, native worker,
      and language-server stubs.

    Transformation:
    - Builds a local file-backed release so the installer uses its normal
      download, extraction, move, and version-check path without network.
    """

    release_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="terlan-installer-artifact.") as tmp:
        stage = Path(tmp)
        compiler = stage / "terlc"
        vm = stage / "terlan-vm"
        native_worker = stage / "terlan-native-worker"
        lsp = stage / "terlan-lsp"
        compiler.write_text(
            "#!/usr/bin/env sh\nprintf 'terlc fake 9.9.9\\n'\n"
            f"exit {compiler_exit}\n",
            encoding="utf-8",
        )
        vm.write_text("#!/usr/bin/env sh\nprintf 'terlan-vm fake 9.9.9\\n'\n", encoding="utf-8")
        native_worker.write_text(
            "#!/usr/bin/env sh\nprintf 'terlan-native-worker fake 9.9.9\\n'\n",
            encoding="utf-8",
        )
        lsp.write_text("#!/usr/bin/env sh\nprintf 'terlan-lsp --stdio\\n'\n", encoding="utf-8")
        compiler.chmod(0o755)
        vm.chmod(0o755)
        native_worker.chmod(0o755)
        lsp.chmod(0o755)
        required_files = {
            "share/terlan/std/core/Unit.terl": "module std.core.Unit.\n",
            "share/terlan/editors/vscode/package.json": "{}\n",
            "share/terlan/tree-sitter-terlan/grammar.js": "module.exports = {};\n",
            "share/terlan/runtime/release-self-test.tvm": "fake native image\n",
            "terlan-release.json": "{}\n",
            "SHA256SUMS": "\n",
            "terlan-install-manifest.json": "{}\n",
        }
        for relative, content in required_files.items():
            path = stage / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        with tarfile.open(release_dir / artifact, "w:gz") as archive:
            for path in sorted(item for item in stage.rglob("*") if item.is_file()):
                archive.add(path, arcname=path.relative_to(stage).as_posix())
        artifact_path = release_dir / artifact
        digest = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
        artifact_path.with_name(f"{artifact}.sha256").write_text(
            f"{digest}  {artifact}\n", encoding="utf-8"
        )


def run_install_sh_download_smoke(os_name: str, arch: str, artifact: str) -> str | None:
    """Run a local download/install smoke for `install.sh`.

    Inputs:
    - `os_name`: installer OS override.
    - `arch`: installer architecture override.
    - `artifact`: expected artifact filename.

    Outputs:
    - Optional diagnostic text when the installer fails.

    Transformation:
    - Publishes a fake local release under a `file://` base URL, runs the
      installer without dry-run mode, and verifies installed binaries exist.
    """

    with tempfile.TemporaryDirectory(prefix="terlan-installer-smoke.") as tmp:
        root = Path(tmp)
        version = "v9.9.9"
        release_dir = root / "releases" / version
        install_dir = root / "bin"
        write_fake_unix_artifact(release_dir, artifact)
        env = os.environ.copy()
        env.update(
            {
                "TERLAN_INSTALL_OS": os_name,
                "TERLAN_INSTALL_ARCH": arch,
                "TERLAN_VERSION": version,
                "TERLAN_INSTALL_DIR": str(install_dir),
                "TERLAN_RELEASE_BASE_URL": f"file://{root / 'releases'}",
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
            return result.stdout + result.stderr
        for binary in ("terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"):
            installed = install_dir / binary
            if not installed.is_file():
                return f"installer did not install {binary} from {artifact}"
            if not os.access(installed, os.X_OK):
                return f"installer did not preserve executable bit for {binary}"
        return None


def run_install_sh_upgrade_rollback_smoke() -> str | None:
    """Prove a failed replacement restores the prior installed layout."""

    with tempfile.TemporaryDirectory(prefix="terlan-installer-rollback.") as tmp:
        root = Path(tmp)
        version = "v9.9.9"
        release_dir = root / "releases" / version
        install_dir = root / "install" / "bin"
        artifact = "terlc-linux-x86_64.tar.gz"
        env = os.environ.copy()
        env.update(
            {
                "TERLAN_INSTALL_OS": "Linux",
                "TERLAN_INSTALL_ARCH": "x86_64",
                "TERLAN_VERSION": version,
                "TERLAN_INSTALL_DIR": str(install_dir),
                "TERLAN_RELEASE_BASE_URL": f"file://{root / 'releases'}",
            }
        )
        write_fake_unix_artifact(release_dir, artifact)
        first = subprocess.run(
            ["sh", str(INSTALL_SH)], cwd=ROOT, env=env, capture_output=True, text=True, check=False
        )
        if first.returncode != 0:
            return f"initial rollback fixture install failed: {first.stdout}{first.stderr}"
        compiler = install_dir / "terlc"
        baseline = compiler.read_bytes()
        write_fake_unix_artifact(release_dir, artifact, compiler_exit=9)
        replacement = subprocess.run(
            ["sh", str(INSTALL_SH)], cwd=ROOT, env=env, capture_output=True, text=True, check=False
        )
        if replacement.returncode == 0:
            return "failing upgrade fixture unexpectedly succeeded"
        if compiler.read_bytes() != baseline:
            return "failed upgrade did not restore the previous compiler"
        if not (install_dir / "terlan-vm").is_file():
            return "failed upgrade did not restore the previous VM"
        if not (install_dir / "terlan-native-worker").is_file():
            return "failed upgrade did not restore the previous native worker"
        if not (install_dir / "terlan-lsp").is_file():
            return "failed upgrade did not restore the previous language server"
        if not (root / "install/share/terlan/std").is_dir():
            return "failed upgrade did not restore the previous stdlib"
        return None


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
        smoke_error = run_install_sh_download_smoke(os_name, arch, expected_artifact)
        if smoke_error is not None:
            diagnostics.append(
                InstallerDiagnostic(
                    INSTALL_SH,
                    f"{label} local artifact install smoke failed: {smoke_error.strip()}",
                )
            )
    rollback_error = run_install_sh_upgrade_rollback_smoke()
    if rollback_error is not None:
        diagnostics.append(InstallerDiagnostic(INSTALL_SH, rollback_error))
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
    described_artifacts: set[str] = set()
    cases = [
        ("Linux", "x86_64", "linux", "x86_64", "terlc-linux-x86_64.tar.gz", "terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"),
        ("Linux", "aarch64", "linux", "aarch64", "terlc-linux-aarch64.tar.gz", "terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"),
        ("Darwin", "x86_64", "macos", "x86_64", "terlc-macos-x86_64.tar.gz", "terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"),
        ("Darwin", "arm64", "macos", "aarch64", "terlc-macos-aarch64.tar.gz", "terlc", "terlan-vm", "terlan-native-worker", "terlan-lsp"),
        ("Windows", "AMD64", "windows", "x86_64", "terlc-windows-x86_64.zip", "terlc.exe", "terlan-vm.exe", "terlan-native-worker.exe", "terlan-lsp.exe"),
        ("Windows", "ARM64", "windows", "aarch64", "terlc-windows-aarch64.zip", "terlc.exe", "terlan-vm.exe", "terlan-native-worker.exe", "terlan-lsp.exe"),
    ]
    for (
        os_name,
        arch,
        expected_os,
        expected_arch,
        expected_artifact,
        expected_binary,
        expected_vm_binary,
        expected_native_worker_binary,
        expected_lsp_binary,
    ) in cases:
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
            "vm_binary": expected_vm_binary,
            "native_worker_binary": expected_native_worker_binary,
            "lsp_binary": expected_lsp_binary,
        }
        described_artifacts.add(values.get("artifact", ""))
        for key, expected_value in expected.items():
            actual = values.get(key)
            if actual != expected_value:
                diagnostics.append(
                    InstallerDiagnostic(
                        PACKAGE_HELPER,
                        f"{label} expected {key}={expected_value}, found {actual!r}",
                    )
                )
    if described_artifacts != REQUIRED_RELEASE_ARTIFACTS:
        diagnostics.append(
            InstallerDiagnostic(
                PACKAGE_HELPER,
                "release helper artifact set drifted; "
                f"expected {sorted(REQUIRED_RELEASE_ARTIFACTS)}, found {sorted(described_artifacts)}",
            )
        )
    return diagnostics


def check_release_publication_contract() -> list[InstallerDiagnostic]:
    """Validate the local release-publication contract.

    Inputs:
    - `Makefile`.
    - `scripts/publish_release_from_dist.sh`.
    - `.github/WORKFLOWS.md`.
    - `.github/workflows/release.yml`.

    Outputs:
    - Diagnostics when the local release flow stops owning artifact
      construction and publication.

    Transformation:
    - Keeps installer naming coupled to the packaging helper while ensuring
      GitHub Actions remains validation-only after the 0.0.7 runtime pivot.
    """

    diagnostics: list[InstallerDiagnostic] = []
    if not MAKEFILE.is_file():
        diagnostics.append(InstallerDiagnostic(MAKEFILE, "Makefile is missing"))
    else:
        makefile = MAKEFILE.read_text(encoding="utf-8")
        required_makefile = [
            "publish-preflight:",
            "$(MAKE) release-artifact-current",
            "publish-release-from-dist:",
            "bash scripts/publish_release_from_dist.sh",
        ]
        diagnostics.extend(
            InstallerDiagnostic(MAKEFILE, f"missing local publication contract text `{needle}`")
            for needle in required_makefile
            if needle not in makefile
        )

    if not PUBLISH_FROM_DIST.is_file():
        diagnostics.append(InstallerDiagnostic(PUBLISH_FROM_DIST, "publish-from-dist script is missing"))
    else:
        publisher = PUBLISH_FROM_DIST.read_text(encoding="utf-8")
        required_publisher = [
            "release_promotion_pipeline.py verify",
            "release_promotion_pipeline.py list",
            "gh release upload",
            "--clobber",
        ]
        diagnostics.extend(
            InstallerDiagnostic(PUBLISH_FROM_DIST, f"missing local publication upload text `{needle}`")
            for needle in required_publisher
            if needle not in publisher
        )
        try:
            validate_repository_contract(ROOT)
        except (OSError, PromotionError) as error:
            diagnostics.append(InstallerDiagnostic(PUBLISH_FROM_DIST, str(error)))

    if not WORKFLOWS_DOC.is_file():
        diagnostics.append(InstallerDiagnostic(WORKFLOWS_DOC, "workflow documentation is missing"))
    else:
        workflows_doc = WORKFLOWS_DOC.read_text(encoding="utf-8")
        required_doc = [
            "release artifacts are built and published from the",
            "local release command",
            "make publish VERSION=0.0.7",
            "It does not build release artifacts and it does not publish GitHub releases.",
        ]
        diagnostics.extend(
            InstallerDiagnostic(WORKFLOWS_DOC, f"missing release publication documentation `{needle}`")
            for needle in required_doc
            if needle not in workflows_doc
        )

    if not RELEASE_WORKFLOW.is_file():
        diagnostics.append(InstallerDiagnostic(RELEASE_WORKFLOW, "release workflow is missing"))
    else:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        if "make release-artifact-current" in workflow:
            diagnostics.append(
                InstallerDiagnostic(
                    RELEASE_WORKFLOW,
                    "release workflow must validate only; artifact publication is local",
                )
            )
        if "make release-candidate-check" not in workflow:
            diagnostics.append(
                InstallerDiagnostic(
                    RELEASE_WORKFLOW,
                    "release workflow must run the canonical release candidate gate",
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
        'Version = "v0.0.7"',
        'terlc-windows-$terlanArch.zip',
        "Invoke-WebRequest",
        "Expand-Archive",
        "TERLAN_INSTALL_DRY_RUN",
        "Get-FileHash",
        '"Arm64"',
        "shareSource",
        "terlc.exe",
        "terlan-vm.exe",
        "terlan-native-worker.exe",
        "terlan-lsp.exe",
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
    diagnostics.extend(check_release_publication_contract())
    diagnostics.extend(check_install_ps1())
    if diagnostics:
        for diagnostic in diagnostics:
            print(diagnostic.render(), file=sys.stderr)
        return 1
    print("Installer contract checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
