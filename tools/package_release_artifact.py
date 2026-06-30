#!/usr/bin/env python3
"""Package and smoke-test Terlan release artifacts.

Inputs:
- Compiled `terlc` and `terlan-vm` binaries under the Cargo release target
  directory.
- Optional `TERLAN_RELEASE_OS` and `TERLAN_RELEASE_ARCH` overrides.

Outputs:
- `dist/terlc-<os>-<arch>.tar.gz` for Linux and macOS.
- `dist/terlc-windows-<arch>.zip` for Windows.
- Exit status 0 when packaging or smoke validation succeeds.

Transformation:
- Detects the host platform, maps it to the installer artifact naming contract,
  and writes a single release archive containing the compiler and standalone VM
  binaries.
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DIST_DIR = ROOT / "dist"
RELEASE_TARGET_DIR = ROOT / "target" / "release"


@dataclass(frozen=True)
class ReleasePlatform:
    """Normalized release platform identity.

    Inputs:
    - `os_name`: installer-facing operating-system name.
    - `arch`: installer-facing architecture name.

    Outputs:
    - Artifact names and binary names for release packaging.

    Transformation:
    - Keeps the release workflow, Makefile, and installer on one naming scheme.
    """

    os_name: str
    arch: str

    @property
    def compiler_binary_name(self) -> str:
        """Return the compiler executable filename used inside the artifact."""

        if self.os_name == "windows":
            return "terlc.exe"
        return "terlc"

    @property
    def vm_binary_name(self) -> str:
        """Return the standalone VM executable filename used inside the artifact."""

        if self.os_name == "windows":
            return "terlan-vm.exe"
        return "terlan-vm"

    @property
    def artifact_name(self) -> str:
        """Return the platform artifact filename."""

        if self.os_name == "windows":
            return f"terlc-windows-{self.arch}.zip"
        return f"terlc-{self.os_name}-{self.arch}.tar.gz"

    @property
    def artifact_path(self) -> Path:
        """Return the platform artifact path under `dist/`."""

        return DIST_DIR / self.artifact_name


def normalize_os(raw_os: str) -> str:
    """Normalize an operating-system name to the release contract.

    Inputs:
    - Raw platform name from Python or `TERLAN_RELEASE_OS`.

    Outputs:
    - `linux`, `macos`, or `windows`.

    Transformation:
    - Accepts common platform spellings and rejects unsupported targets with a
      stable diagnostic.
    """

    normalized = raw_os.strip().lower()
    if normalized in {"linux"}:
        return "linux"
    if normalized in {"darwin", "macos", "mac"}:
        return "macos"
    if normalized in {"windows", "win32", "mingw", "msys"}:
        return "windows"
    raise ValueError(f"unsupported release OS `{raw_os}`")


def normalize_arch(raw_arch: str) -> str:
    """Normalize an architecture name to the release contract."""

    normalized = raw_arch.strip().lower()
    if normalized in {"x86_64", "amd64"}:
        return "x86_64"
    if normalized in {"aarch64", "arm64"}:
        return "aarch64"
    raise ValueError(f"unsupported release architecture `{raw_arch}`")


def detect_release_platform() -> ReleasePlatform:
    """Detect the release platform from environment or host metadata."""

    raw_os = os.environ.get("TERLAN_RELEASE_OS", platform.system())
    raw_arch = os.environ.get("TERLAN_RELEASE_ARCH", platform.machine())
    return ReleasePlatform(normalize_os(raw_os), normalize_arch(raw_arch))


def release_binary_paths(release_platform: ReleasePlatform) -> list[Path]:
    """Return compiled release binaries expected for the platform."""

    return [
        RELEASE_TARGET_DIR / release_platform.compiler_binary_name,
        RELEASE_TARGET_DIR / release_platform.vm_binary_name,
    ]


def copy_release_binaries_to_dist(release_platform: ReleasePlatform) -> list[Path]:
    """Copy the compiled release binaries into `dist/`.

    Inputs:
    - Normalized release platform.

    Outputs:
    - Paths to copied binaries under `dist/`.

    Transformation:
    - Keeps artifact construction independent from Cargo's target directory.
    """

    DIST_DIR.mkdir(parents=True, exist_ok=True)
    copied: list[Path] = []
    for source in release_binary_paths(release_platform):
        if not source.is_file():
            raise FileNotFoundError(f"release binary is missing: {source}")
        destination = DIST_DIR / source.name
        shutil.copy2(source, destination)
        destination.chmod(destination.stat().st_mode | 0o755)
        copied.append(destination)
    return copied


def write_tar_artifact(release_platform: ReleasePlatform, binaries: list[Path]) -> Path:
    """Write a `.tar.gz` release artifact."""

    artifact = release_platform.artifact_path
    with tarfile.open(artifact, "w:gz") as archive:
        for binary in binaries:
            archive.add(binary, arcname=binary.name)
    return artifact


def write_zip_artifact(release_platform: ReleasePlatform, binaries: list[Path]) -> Path:
    """Write a `.zip` release artifact."""

    artifact = release_platform.artifact_path
    with zipfile.ZipFile(artifact, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for binary in binaries:
            archive.write(binary, arcname=binary.name)
    return artifact


def package_artifact() -> Path:
    """Package the current release artifact and print its path."""

    release_platform = detect_release_platform()
    binaries = copy_release_binaries_to_dist(release_platform)
    if release_platform.os_name == "windows":
        artifact = write_zip_artifact(release_platform, binaries)
    else:
        artifact = write_tar_artifact(release_platform, binaries)
    print(artifact.relative_to(ROOT))
    return artifact


def describe_artifact() -> None:
    """Print the current release platform artifact identity.

    Inputs:
    - Optional `TERLAN_RELEASE_OS` and `TERLAN_RELEASE_ARCH` overrides.

    Outputs:
    - Stable `key=value` lines for release contract checks.

    Transformation:
    - Exposes the same platform normalization used by packaging without
      requiring a compiled binary.
    """

    release_platform = detect_release_platform()
    print(f"os={release_platform.os_name}")
    print(f"arch={release_platform.arch}")
    print(f"artifact={release_platform.artifact_name}")
    print(f"binary={release_platform.compiler_binary_name}")
    print(f"vm_binary={release_platform.vm_binary_name}")


def extract_artifact(artifact: Path, destination: Path) -> None:
    """Extract a release artifact into a temporary directory."""

    if artifact.suffix == ".zip":
        with zipfile.ZipFile(artifact) as archive:
            archive.extractall(destination)
        return
    if artifact.name.endswith(".tar.gz"):
        with tarfile.open(artifact, "r:gz") as archive:
            archive.extractall(destination)
        return
    raise ValueError(f"unsupported release artifact format: {artifact}")


def cargo_version() -> str:
    """Return the workspace package version from `Cargo.toml`."""

    for line in (ROOT / "Cargo.toml").read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if stripped.startswith("version = "):
            return stripped.split('"', maxsplit=2)[1]
    raise ValueError("Cargo.toml is missing workspace package version")


def run_smoke_command(command: list[str], cwd: Path | None = None) -> None:
    """Run one release artifact smoke command."""

    subprocess.run(command, cwd=cwd, check=True)


def smoke_artifact() -> None:
    """Smoke-test the packaged artifact for the current release platform."""

    release_platform = detect_release_platform()
    artifact = release_platform.artifact_path
    if not artifact.is_file():
        raise FileNotFoundError(f"release artifact is missing: {artifact}")
    with tempfile.TemporaryDirectory(prefix="terlan-release-artifact-smoke.") as tmp:
        tmpdir = Path(tmp)
        extract_artifact(artifact, tmpdir)
        binary = tmpdir / release_platform.compiler_binary_name
        vm_binary = tmpdir / release_platform.vm_binary_name
        if not binary.is_file():
            raise FileNotFoundError(f"artifact did not contain {release_platform.compiler_binary_name}")
        if not vm_binary.is_file():
            raise FileNotFoundError(f"artifact did not contain {release_platform.vm_binary_name}")
        binary.chmod(binary.stat().st_mode | 0o755)
        vm_binary.chmod(vm_binary.stat().st_mode | 0o755)
        version_output = subprocess.check_output([str(binary), "--version"], text=True).strip()
        expected = f"terlc {cargo_version()}"
        if version_output != expected:
            raise AssertionError(f"expected `{expected}`, got `{version_output}`")
        vm_version_output = subprocess.check_output([str(vm_binary), "--version"], text=True).strip()
        expected_vm = f"terlan-vm {cargo_version()}"
        if vm_version_output != expected_vm:
            raise AssertionError(f"expected `{expected_vm}`, got `{vm_version_output}`")

        hello = tmpdir / "hello"
        run_smoke_command([str(binary), "init", str(hello), "--profile", "web"])
        asset = hello / "assets" / "hello.txt"
        asset.write_text("hello asset\n", encoding="utf-8")
        run_smoke_command(
            [
                str(binary),
                "--out-dir",
                str(hello / "_build"),
                "build",
                str(hello),
                "--target",
                "erlang",
            ]
        )
        run_smoke_command(
            [
                str(binary),
                "--target-profile",
                "js.browser",
                "--out-dir",
                str(hello / "_build"),
                "build",
                str(hello),
                "--target",
                "js.browser",
            ]
        )
        web_asset = hello / "_build" / "web" / "assets" / "hello.txt"
        if not web_asset.is_file():
            raise FileNotFoundError(f"web asset was not packaged: {web_asset}")
        run_smoke_command([str(binary), "serve", str(hello / "_build" / "web"), "--check"])
        vm_source = tmpdir / "vm_hello.terl"
        vm_source.write_text(
            "\n".join(
                [
                    "module vm_release.Main.",
                    "",
                    "import std.io.Console.{println}.",
                    "",
                    "pub main(): Unit ->",
                    '    println("hello from release terlan-vm").',
                    "",
                ]
            ),
            encoding="utf-8",
        )
        vm_output = subprocess.check_output([str(vm_binary), "run", str(vm_source)], text=True).strip()
        if vm_output != "hello from release terlan-vm":
            raise AssertionError(f"expected release VM hello output, got `{vm_output}`")


def run_installer_smoke_command(command: list[str], env: dict[str, str]) -> None:
    """Run one installer smoke command with stable diagnostics."""

    subprocess.run(command, cwd=ROOT, env=env, text=True, check=True)


def smoke_installer_from_artifact() -> None:
    """Smoke-test the public installer against a local release download.

    Inputs:
    - Packaged current-platform artifact under `dist/`.

    Outputs:
    - Exit status 0 when the installer downloads, extracts, installs, and runs
      `terlc` plus `terlan-vm`.

    Transformation:
    - Serves the already-packaged artifact from a local file-backed release
      mirror and runs the same installer entrypoint users run for the current
      platform.
    """

    release_platform = detect_release_platform()
    artifact = release_platform.artifact_path
    if not artifact.is_file():
        raise FileNotFoundError(f"release artifact is missing: {artifact}")
    version = f"v{cargo_version()}"
    with tempfile.TemporaryDirectory(prefix="terlan-installer-artifact-smoke.") as tmp:
        tmpdir = Path(tmp)
        release_dir = tmpdir / "releases" / version
        release_dir.mkdir(parents=True)
        shutil.copy2(artifact, release_dir / artifact.name)
        install_dir = tmpdir / "install" / "bin"
        release_base_url = (tmpdir / "releases").as_uri()
        env = os.environ.copy()
        env.update(
            {
                "TERLAN_VERSION": version,
                "TERLAN_INSTALL_DIR": str(install_dir),
                "TERLAN_RELEASE_BASE_URL": release_base_url,
            }
        )
        if release_platform.os_name == "windows":
            run_installer_smoke_command(
                [
                    "powershell",
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    str(ROOT / "install.ps1"),
                ],
                env,
            )
        else:
            run_installer_smoke_command(["sh", str(ROOT / "install.sh")], env)
        compiler = install_dir / release_platform.compiler_binary_name
        vm = install_dir / release_platform.vm_binary_name
        if not compiler.is_file():
            raise FileNotFoundError(f"installer did not install {compiler.name}")
        if not vm.is_file():
            raise FileNotFoundError(f"installer did not install {vm.name}")
        expected = f"terlc {cargo_version()}"
        actual = subprocess.check_output([str(compiler), "--version"], text=True).strip()
        if actual != expected:
            raise AssertionError(f"expected `{expected}`, got `{actual}`")
        expected_vm = f"terlan-vm {cargo_version()}"
        actual_vm = subprocess.check_output([str(vm), "--version"], text=True).strip()
        if actual_vm != expected_vm:
            raise AssertionError(f"expected `{expected_vm}`, got `{actual_vm}`")


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse release artifact helper arguments."""

    parser = argparse.ArgumentParser(description="Package or smoke-test terlc release artifacts.")
    parser.add_argument("command", choices=["describe", "package", "smoke", "installer-smoke"])
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Run the release artifact helper."""

    args = parse_args(argv)
    try:
        if args.command == "describe":
            describe_artifact()
        elif args.command == "package":
            package_artifact()
        elif args.command == "smoke":
            smoke_artifact()
        else:
            smoke_installer_from_artifact()
    except Exception as exc:  # noqa: BLE001 - stable CLI diagnostics for release gates.
        print(f"release artifact helper failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
