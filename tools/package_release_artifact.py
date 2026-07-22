#!/usr/bin/env python3
"""Package and smoke-test Terlan release artifacts.

Inputs:
- Compiled `terlc`, `terlan-vm`, `terlan-native-worker`, and `terlan-lsp` binaries under the Cargo release target
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
import hashlib
import json
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

from release_transition_scan import assert_no_transition_payloads


ROOT = Path(__file__).resolve().parents[1]
DIST_DIR = ROOT / "dist"
RELEASE_TARGET_DIR = ROOT / "target" / "release"
RUST_FEATURE_MANIFEST = ROOT / "docs" / "package" / "RUST_BUILD_FEATURES.json"
RELEASE_METADATA_NAME = "terlan-release.json"
RELEASE_CHECKSUMS_NAME = "SHA256SUMS"
INSTALL_MANIFEST_NAME = "terlan-install-manifest.json"
RELEASE_SELF_TEST_PACKAGE_PATH = Path("runtime/release-self-test.tvm")
RELEASE_SELF_TEST_ARCHIVE_PATH = Path("share/terlan") / RELEASE_SELF_TEST_PACKAGE_PATH
RELEASE_SELF_TEST_ENTRY = "release_validation.Main.main"
PAYLOAD_ROOTS = (
    (ROOT / "std", Path("share/terlan/std")),
    (ROOT / "editors" / "vscode", Path("share/terlan/editors/vscode")),
    (ROOT / "editors" / "shared", Path("share/terlan/editors/shared")),
    (ROOT / "tree-sitter-terlan", Path("share/terlan/tree-sitter-terlan")),
    (ROOT / "docs" / "compiler", Path("share/terlan/docs/compiler")),
    (ROOT / "docs" / "editor", Path("share/terlan/docs/editor")),
    (ROOT / "docs" / "grammar", Path("share/terlan/docs/grammar")),
    (ROOT / "docs" / "language", Path("share/terlan/docs/language")),
    (ROOT / "docs" / "package", Path("share/terlan/docs/package")),
    (ROOT / "docs" / "release", Path("share/terlan/docs/release")),
    (ROOT / "docs" / "runtime", Path("share/terlan/docs/runtime")),
)
PAYLOAD_FILES = (
    (ROOT / "README.md", Path("share/terlan/README.md")),
    (ROOT / "CHANGELOG.md", Path("share/terlan/CHANGELOG.md")),
    (
        ROOT / "tests" / "template" / "INTERPOLATION_TOOLING_FIXTURES.tsv",
        Path("share/terlan/tests/template/INTERPOLATION_TOOLING_FIXTURES.tsv"),
    ),
)
EXCLUDED_PAYLOAD_PARTS = {"node_modules", "target", "__pycache__"}
EXCLUDED_PAYLOAD_SUFFIXES = {".pyc", ".vsix"}


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
    def native_worker_binary_name(self) -> str:
        """Return the crash-isolated native worker executable filename."""

        if self.os_name == "windows":
            return "terlan-native-worker.exe"
        return "terlan-native-worker"

    @property
    def lsp_binary_name(self) -> str:
        """Return the standalone language-server executable filename."""

        if self.os_name == "windows":
            return "terlan-lsp.exe"
        return "terlan-lsp"

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
        RELEASE_TARGET_DIR / release_platform.native_worker_binary_name,
        RELEASE_TARGET_DIR / release_platform.lsp_binary_name,
    ]


def default_target_triple(release_platform: ReleasePlatform) -> str:
    """Return the default Rust target triple for release metadata."""

    override = (
        os.environ.get("TERLAN_RELEASE_TARGET_TRIPLE")
        or os.environ.get("CARGO_BUILD_TARGET")
        or os.environ.get("TARGET")
    )
    if override:
        return override
    triples = {
        ("linux", "x86_64"): "x86_64-unknown-linux-gnu",
        ("linux", "aarch64"): "aarch64-unknown-linux-gnu",
        ("macos", "x86_64"): "x86_64-apple-darwin",
        ("macos", "aarch64"): "aarch64-apple-darwin",
        ("windows", "x86_64"): "x86_64-pc-windows-msvc",
        ("windows", "aarch64"): "aarch64-pc-windows-msvc",
    }
    return triples[(release_platform.os_name, release_platform.arch)]


def source_revision() -> str:
    """Return the git/source revision recorded in release metadata."""

    override = os.environ.get("TERLAN_SOURCE_REVISION")
    if override:
        return override
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short=12", "HEAD"],
            cwd=ROOT,
            stderr=subprocess.DEVNULL,
            text=True,
        ).strip()
    except Exception:  # noqa: BLE001 - release metadata should still be printable from source archives.
        return "unknown"


def read_rust_feature_manifest() -> dict[str, object]:
    """Read the canonical Rust build feature manifest."""

    with RUST_FEATURE_MANIFEST.open(encoding="utf-8") as handle:
        return json.load(handle)


def release_feature_set() -> list[str]:
    """Return the feature set used for release builds."""

    manifest = read_rust_feature_manifest()
    for profile in manifest.get("release_profiles", []):
        if isinstance(profile, dict) and profile.get("name") == "release":
            features = profile.get("feature_set", [])
            if isinstance(features, list):
                return [str(feature) for feature in features]
    return []


def sha256_file(path: Path) -> str:
    """Return the SHA-256 digest of one release payload file."""

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def release_payload_files() -> list[tuple[Path, Path]]:
    """Return deterministic source/archive paths for shared release payloads."""

    payloads: list[tuple[Path, Path]] = []
    for source, archive in PAYLOAD_FILES:
        if not source.is_file():
            raise FileNotFoundError(f"release payload file is missing: {source}")
        payloads.append((source, archive))
    for source_root, archive_root in PAYLOAD_ROOTS:
        if not source_root.is_dir():
            raise FileNotFoundError(f"release payload directory is missing: {source_root}")
        for source in sorted(path for path in source_root.rglob("*") if path.is_file()):
            relative = source.relative_to(source_root)
            if EXCLUDED_PAYLOAD_PARTS.intersection(relative.parts):
                continue
            if source.suffix in EXCLUDED_PAYLOAD_SUFFIXES:
                continue
            payloads.append((source, archive_root / relative))
    return payloads


def payload_tree_hash(payloads: list[tuple[Path, Path]], prefix: str) -> str:
    """Hash archive paths and contents below one release payload prefix."""

    digest = hashlib.sha256()
    selected = [(source, archive) for source, archive in payloads if archive.as_posix().startswith(prefix)]
    for source, archive in selected:
        digest.update(archive.as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(sha256_file(source).encode("ascii"))
        digest.update(b"\n")
    if not selected:
        raise ValueError(f"release payload prefix `{prefix}` is empty")
    return digest.hexdigest()


def release_metadata(
    release_platform: ReleasePlatform,
    payloads: list[tuple[Path, Path]] | None = None,
    binaries: list[Path] | None = None,
    native_self_test: dict[str, object] | None = None,
) -> dict[str, object]:
    """Return exact metadata for the current release artifact."""

    binary_paths = [] if binaries is None else binaries
    binary_descriptors = [
        {
            "name": "terlc",
            "path": release_platform.compiler_binary_name,
        },
        {
            "name": "terlan-vm",
            "path": release_platform.vm_binary_name,
        },
        {
            "name": "terlan-native-worker",
            "path": release_platform.native_worker_binary_name,
        },
        {
            "name": "terlan-lsp",
            "path": release_platform.lsp_binary_name,
        },
    ]
    payloads = release_payload_files() if payloads is None else payloads
    binary_hashes = {binary.name: sha256_file(binary) for binary in binary_paths}
    candidate_id = hashlib.sha256(
        f"{cargo_version()}\0{source_revision()}\0{default_target_triple(release_platform)}".encode()
    ).hexdigest()
    metadata: dict[str, object] = {
        "schema": "terlan.release-artifact.v1",
        "package": "terlan",
        "version": cargo_version(),
        "target_triple": default_target_triple(release_platform),
        "os": release_platform.os_name,
        "arch": release_platform.arch,
        "profile": "release",
        "cargo_package": "terlan",
        "cargo_features": release_feature_set(),
        "source_revision": source_revision(),
        "release_candidate_id": candidate_id,
        "binary_hashes": binary_hashes,
        "payload_hashes": {
            "stdlib": payload_tree_hash(payloads, "share/terlan/std/"),
            "editor": payload_tree_hash(payloads, "share/terlan/editors/"),
            "tree_sitter": payload_tree_hash(payloads, "share/terlan/tree-sitter-terlan/"),
            "docs": payload_tree_hash(payloads, "share/terlan/docs/"),
        },
        "payload_file_count": len(payloads),
        "crate_versions": {
            "terlan": cargo_version(),
        },
        "binaries": binary_descriptors,
    }
    if native_self_test is not None:
        metadata["native_self_test"] = native_self_test
    return metadata


def write_release_metadata_to_dist(
    release_platform: ReleasePlatform,
    payloads: list[tuple[Path, Path]],
    binaries: list[Path],
    native_self_test: dict[str, object],
) -> Path:
    """Write exact release metadata beside copied binaries."""

    DIST_DIR.mkdir(parents=True, exist_ok=True)
    metadata_path = DIST_DIR / RELEASE_METADATA_NAME
    metadata_path.write_text(
        json.dumps(
            release_metadata(release_platform, payloads, binaries, native_self_test),
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return metadata_path


def write_install_metadata(
    release_platform: ReleasePlatform,
    binaries: list[Path],
    metadata_path: Path,
    payloads: list[tuple[Path, Path]],
) -> tuple[Path, Path]:
    """Write deterministic checksums and the uninstall/install file manifest."""

    entries: list[tuple[Path, Path]] = [
        (binary, Path(binary.name)) for binary in binaries
    ]
    entries.append((metadata_path, Path(RELEASE_METADATA_NAME)))
    entries.extend(payloads)
    checksums_path = DIST_DIR / RELEASE_CHECKSUMS_NAME
    checksums_path.write_text(
        "".join(f"{sha256_file(source)}  {archive.as_posix()}\n" for source, archive in entries),
        encoding="utf-8",
    )
    install_manifest_path = DIST_DIR / INSTALL_MANIFEST_NAME
    install_manifest_path.write_text(
        json.dumps(
            {
                "schema": "terlan.install-manifest.v1",
                "version": cargo_version(),
                "target_triple": default_target_triple(release_platform),
                "files": [archive.as_posix() for _, archive in entries]
                + [RELEASE_CHECKSUMS_NAME, INSTALL_MANIFEST_NAME],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    return checksums_path, install_manifest_path


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


def build_release_self_test(
    release_platform: ReleasePlatform, binaries: list[Path]
) -> tuple[Path, dict[str, object]]:
    """Compile and inspect the target-native image shipped with every release."""

    compiler = next(binary for binary in binaries if binary.name == release_platform.compiler_binary_name)
    vm = next(binary for binary in binaries if binary.name == release_platform.vm_binary_name)
    source_root = DIST_DIR / "release-self-test-source"
    build_root = DIST_DIR / "release-self-test-build"
    shutil.rmtree(source_root, ignore_errors=True)
    shutil.rmtree(build_root, ignore_errors=True)
    source_root.mkdir(parents=True)
    source = source_root / "Main.terl"
    source.write_text(
        "\n".join(
            [
                "module release_validation.Main.",
                "",
                "pub main(): Bool ->",
                "    true.",
                "",
                "continuation_probe(): Unit ->",
                '    std.io.Console.println("release native continuation probe").',
                "",
            ]
        ),
        encoding="utf-8",
    )
    subprocess.run(
        [
            str(compiler),
            "--out-dir",
            str(build_root),
            "build",
            str(source),
            "--target",
            "terlan-vm",
        ],
        cwd=ROOT,
        check=True,
    )
    images = list((build_root / "vm").glob("*.tvm"))
    if len(images) != 1:
        raise FileNotFoundError(
            f"release self-test build must emit exactly one .tvm image, found {images}"
        )
    packaged_image = DIST_DIR / RELEASE_SELF_TEST_PACKAGE_PATH.name
    shutil.copy2(images[0], packaged_image)
    metadata_output = subprocess.check_output(
        [
            str(vm),
            "package-image-metadata",
            str(packaged_image),
            "--entry",
            RELEASE_SELF_TEST_ENTRY,
            "--package-path",
            RELEASE_SELF_TEST_PACKAGE_PATH.as_posix(),
        ],
        cwd=ROOT,
        text=True,
    )
    metadata = json.loads(metadata_output)
    expected_target = default_target_triple(release_platform)
    if metadata.get("target_triple") != expected_target:
        raise AssertionError(
            "release self-test image target does not match release target "
            f"`{expected_target}`"
        )
    if not metadata.get("continuation_ids"):
        raise AssertionError("release self-test image must exercise continuation metadata")
    if int(metadata.get("native_debug_record_count", 0)) < 2:
        raise AssertionError("release self-test image must carry native debug records")
    return packaged_image, metadata


def write_tar_artifact(
    release_platform: ReleasePlatform,
    binaries: list[Path],
    metadata_path: Path,
    payloads: list[tuple[Path, Path]],
    generated_metadata: tuple[Path, Path],
) -> Path:
    """Write a `.tar.gz` release artifact."""

    artifact = release_platform.artifact_path
    with tarfile.open(artifact, "w:gz") as archive:
        for binary in binaries:
            archive.add(binary, arcname=binary.name)
        archive.add(metadata_path, arcname=metadata_path.name)
        for source, archive_path in payloads:
            archive.add(source, arcname=archive_path.as_posix())
        for path in generated_metadata:
            archive.add(path, arcname=path.name)
    return artifact


def write_zip_artifact(
    release_platform: ReleasePlatform,
    binaries: list[Path],
    metadata_path: Path,
    payloads: list[tuple[Path, Path]],
    generated_metadata: tuple[Path, Path],
) -> Path:
    """Write a `.zip` release artifact."""

    artifact = release_platform.artifact_path
    with zipfile.ZipFile(artifact, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for binary in binaries:
            archive.write(binary, arcname=binary.name)
        archive.write(metadata_path, arcname=metadata_path.name)
        for source, archive_path in payloads:
            archive.write(source, arcname=archive_path.as_posix())
        for path in generated_metadata:
            archive.write(path, arcname=path.name)
    return artifact


def package_artifact() -> Path:
    """Package the current release artifact and print its path."""

    release_platform = detect_release_platform()
    binaries = copy_release_binaries_to_dist(release_platform)
    payloads = release_payload_files()
    self_test_image, self_test_metadata = build_release_self_test(release_platform, binaries)
    payloads.append((self_test_image, RELEASE_SELF_TEST_ARCHIVE_PATH))
    metadata_path = write_release_metadata_to_dist(
        release_platform, payloads, binaries, self_test_metadata
    )
    generated_metadata = write_install_metadata(release_platform, binaries, metadata_path, payloads)
    if release_platform.os_name == "windows":
        artifact = write_zip_artifact(
            release_platform, binaries, metadata_path, payloads, generated_metadata
        )
    else:
        artifact = write_tar_artifact(
            release_platform, binaries, metadata_path, payloads, generated_metadata
        )
    artifact.with_name(f"{artifact.name}.sha256").write_text(
        f"{sha256_file(artifact)}  {artifact.name}\n", encoding="utf-8"
    )
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
    print(f"native_worker_binary={release_platform.native_worker_binary_name}")
    print(f"lsp_binary={release_platform.lsp_binary_name}")


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
        native_worker_binary = tmpdir / release_platform.native_worker_binary_name
        lsp_binary = tmpdir / release_platform.lsp_binary_name
        if not binary.is_file():
            raise FileNotFoundError(f"artifact did not contain {release_platform.compiler_binary_name}")
        if not vm_binary.is_file():
            raise FileNotFoundError(f"artifact did not contain {release_platform.vm_binary_name}")
        if not native_worker_binary.is_file():
            raise FileNotFoundError(
                f"artifact did not contain {release_platform.native_worker_binary_name}"
            )
        if not lsp_binary.is_file():
            raise FileNotFoundError(f"artifact did not contain {release_platform.lsp_binary_name}")
        metadata_file = tmpdir / RELEASE_METADATA_NAME
        if not metadata_file.is_file():
            raise FileNotFoundError(f"artifact did not contain {RELEASE_METADATA_NAME}")
        metadata = json.loads(metadata_file.read_text(encoding="utf-8"))
        if metadata.get("schema") != "terlan.release-artifact.v1":
            raise AssertionError("release metadata has an unsupported schema")
        if metadata.get("cargo_features") != release_feature_set():
            raise AssertionError("release metadata cargo feature set does not match manifest")
        binary_hashes = metadata.get("binary_hashes", {})
        for packaged_binary in (binary, vm_binary, native_worker_binary, lsp_binary):
            if binary_hashes.get(packaged_binary.name) != sha256_file(packaged_binary):
                raise AssertionError(f"release metadata hash drift for `{packaged_binary.name}`")
        metadata_binary_paths = {
            row.get("path") for row in metadata.get("binaries", []) if isinstance(row, dict)
        }
        for binary_name in (
            release_platform.compiler_binary_name,
            release_platform.vm_binary_name,
            release_platform.native_worker_binary_name,
            release_platform.lsp_binary_name,
        ):
            if binary_name not in metadata_binary_paths:
                raise AssertionError(f"release metadata omitted binary `{binary_name}`")
        for required in (
            tmpdir / "share/terlan/std",
            tmpdir / "share/terlan/editors/vscode/package.json",
            tmpdir / "share/terlan/tree-sitter-terlan/grammar.js",
            tmpdir / "share/terlan/docs/grammar/TERLAN_SYNTAX_SPEC.ebnf",
            tmpdir / "share/terlan/README.md",
            tmpdir / "share/terlan/CHANGELOG.md",
            tmpdir / RELEASE_CHECKSUMS_NAME,
            tmpdir / INSTALL_MANIFEST_NAME,
        ):
            if not required.exists():
                raise FileNotFoundError(f"artifact omitted release payload `{required.relative_to(tmpdir)}`")
        verify_payload_checksums(tmpdir)
        assert_no_transition_payloads(tmpdir)
        binary.chmod(binary.stat().st_mode | 0o755)
        vm_binary.chmod(vm_binary.stat().st_mode | 0o755)
        native_worker_binary.chmod(native_worker_binary.stat().st_mode | 0o755)
        lsp_binary.chmod(lsp_binary.stat().st_mode | 0o755)
        run_smoke_command([str(vm_binary), "validate-package", str(tmpdir)])
        version_output = subprocess.check_output([str(binary), "--version"], text=True).strip()
        expected = f"terlc {cargo_version()}"
        if version_output != expected:
            raise AssertionError(f"expected `{expected}`, got `{version_output}`")
        vm_version_output = subprocess.check_output([str(vm_binary), "--version"], text=True).strip()
        expected_vm = f"terlan-vm {cargo_version()}"
        if vm_version_output != expected_vm:
            raise AssertionError(f"expected `{expected_vm}`, got `{vm_version_output}`")
        worker_version_output = subprocess.check_output(
            [str(native_worker_binary), "--version"], text=True
        ).strip()
        expected_worker = f"terlan-native-worker {cargo_version()}"
        if worker_version_output != expected_worker:
            raise AssertionError(f"expected `{expected_worker}`, got `{worker_version_output}`")
        lsp_help = subprocess.check_output([str(lsp_binary), "--help"], text=True)
        if "terlan-lsp --stdio" not in lsp_help:
            raise AssertionError("packaged language server omitted its stdio entrypoint")

        hello = tmpdir / "hello"
        run_smoke_command([str(binary), "init", str(hello), "--profile", "web"])
        asset = hello / "assets" / "hello.txt"
        asset.write_text("hello asset\n", encoding="utf-8")
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
        run_smoke_command(
            [str(binary), "serve", str(hello / "_build" / "web"), "--check-config"]
        )
        vm_source = tmpdir / "vm_hello.terl"
        vm_source.write_text(
            "\n".join(
                [
                    "module vm_release.Main.",
                    "",
                    "pub main(): Bool -> true.",
                    "",
                ]
            ),
            encoding="utf-8",
        )
        vm_build = tmpdir / "vm_build"
        run_smoke_command(
            [
                str(binary),
                "--out-dir",
                str(vm_build),
                "build",
                str(vm_source),
                "--target",
                "terlan-vm",
            ]
        )
        vm_images = list((vm_build / "vm").glob("*.tvm"))
        if len(vm_images) != 1:
            raise FileNotFoundError(
                f"release compiler must emit exactly one TVM image, found {vm_images}"
            )
        run_smoke_command(
            [
                str(vm_binary),
                "run",
                str(vm_images[0]),
                "--entry",
                "vm_release.Main.main",
                "--test-eval",
            ]
        )
        test_source = tmpdir / "ReleaseArtifactTest.terl"
        test_source.write_text(
            "\n".join(
                [
                    "module vm_release.ReleaseArtifactTest.",
                    "",
                    "@test",
                    "pub release_artifact_test(): Bool -> true.",
                    "",
                ]
            ),
            encoding="utf-8",
        )
        run_smoke_command(
            [str(binary), "test", str(test_source), "--name", "release_artifact_test"]
        )
        inspect_output = subprocess.check_output(
            [str(binary), "inspect", str(tmpdir), "--snapshot"], text=True
        )
        inspect = json.loads(inspect_output)
        if inspect.get("runtime") != "vm":
            raise AssertionError("installed release inspection did not report VM runtime")
        layout = inspect.get("release_layout", {})
        for key in ("stdlib", "editor", "tree_sitter"):
            if layout.get(key) is not True:
                raise AssertionError(f"installed release inspection omitted `{key}` discovery")


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
        checksum = artifact.with_name(f"{artifact.name}.sha256")
        if not checksum.is_file():
            raise FileNotFoundError(f"release artifact checksum is missing: {checksum}")
        shutil.copy2(checksum, release_dir / checksum.name)
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
        native_worker = install_dir / release_platform.native_worker_binary_name
        lsp = install_dir / release_platform.lsp_binary_name
        if not compiler.is_file():
            raise FileNotFoundError(f"installer did not install {compiler.name}")
        if not vm.is_file():
            raise FileNotFoundError(f"installer did not install {vm.name}")
        if not native_worker.is_file():
            raise FileNotFoundError(f"installer did not install {native_worker.name}")
        if not lsp.is_file():
            raise FileNotFoundError(f"installer did not install {lsp.name}")
        expected = f"terlc {cargo_version()}"
        actual = subprocess.check_output([str(compiler), "--version"], text=True).strip()
        if actual != expected:
            raise AssertionError(f"expected `{expected}`, got `{actual}`")
        expected_vm = f"terlan-vm {cargo_version()}"
        actual_vm = subprocess.check_output([str(vm), "--version"], text=True).strip()
        if actual_vm != expected_vm:
            raise AssertionError(f"expected `{expected_vm}`, got `{actual_vm}`")
        expected_worker = f"terlan-native-worker {cargo_version()}"
        actual_worker = subprocess.check_output(
            [str(native_worker), "--version"], text=True
        ).strip()
        if actual_worker != expected_worker:
            raise AssertionError(f"expected `{expected_worker}`, got `{actual_worker}`")
        if "terlan-lsp --stdio" not in subprocess.check_output([str(lsp), "--help"], text=True):
            raise AssertionError("installed language server omitted its stdio entrypoint")
        share_dir = install_dir.parent / "share" / "terlan"
        run_installer_smoke_command([str(vm), "validate-package", str(share_dir)], env)
        assert_no_transition_payloads(install_dir.parent)


def print_release_metadata() -> None:
    """Print release metadata without requiring compiled binaries."""

    print(json.dumps(release_metadata(detect_release_platform()), indent=2, sort_keys=True))


def verify_payload_checksums(root: Path) -> None:
    """Verify every payload digest declared by an extracted archive."""

    checksum_path = root / RELEASE_CHECKSUMS_NAME
    for line in checksum_path.read_text(encoding="utf-8").splitlines():
        expected, relative = line.split("  ", 1)
        payload = root / relative
        if not payload.is_file():
            raise FileNotFoundError(f"checksummed release payload is missing: {relative}")
        actual = sha256_file(payload)
        if actual != expected:
            raise AssertionError(f"release payload checksum mismatch for `{relative}`")


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse release artifact helper arguments."""

    parser = argparse.ArgumentParser(description="Package or smoke-test terlc release artifacts.")
    parser.add_argument(
        "command", choices=["describe", "metadata", "package", "smoke", "installer-smoke"]
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Run the release artifact helper."""

    args = parse_args(argv)
    try:
        if args.command == "describe":
            describe_artifact()
        elif args.command == "metadata":
            print_release_metadata()
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
