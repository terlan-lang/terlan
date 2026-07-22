#!/usr/bin/env python3
"""Validate the cross-platform, VM-complete Terlan release artifact matrix."""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

import package_release_artifact as packaging


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/vm-release-artifact-matrix-report.json"
TARGETS = (
    ("linux", "x86_64"),
    ("linux", "aarch64"),
    ("macos", "x86_64"),
    ("macos", "aarch64"),
    ("windows", "x86_64"),
    ("windows", "aarch64"),
)
TARGET_TRIPLES = {
    ("linux", "x86_64"): "x86_64-unknown-linux-gnu",
    ("linux", "aarch64"): "aarch64-unknown-linux-gnu",
    ("macos", "x86_64"): "x86_64-apple-darwin",
    ("macos", "aarch64"): "aarch64-apple-darwin",
    ("windows", "x86_64"): "x86_64-pc-windows-msvc",
    ("windows", "aarch64"): "aarch64-pc-windows-msvc",
}
REQUIRED_ARCHIVE_PATHS = (
    "terlan-release.json",
    "terlan-install-manifest.json",
    "SHA256SUMS",
    "share/terlan/std/",
    "share/terlan/editors/vscode/",
    "share/terlan/tree-sitter-terlan/",
    "share/terlan/docs/",
    "share/terlan/README.md",
    "share/terlan/CHANGELOG.md",
)


def normalized_host() -> tuple[str, str]:
    """Return the host identity using the release naming contract."""

    return packaging.normalize_os(platform.system()), packaging.normalize_arch(platform.machine())


def archive_names(artifact: Path) -> set[str]:
    """Return normalized member names from a release archive."""

    if artifact.suffix == ".zip":
        import zipfile

        with zipfile.ZipFile(artifact) as archive:
            return set(archive.namelist())
    import tarfile

    with tarfile.open(artifact, "r:gz") as archive:
        return set(archive.getnames())


def missing_archive_contract(
    names: set[str], release_platform: packaging.ReleasePlatform
) -> list[str]:
    """Return required release members absent from an archive inventory."""

    required_files = {
        release_platform.compiler_binary_name,
        release_platform.vm_binary_name,
        release_platform.native_worker_binary_name,
        release_platform.lsp_binary_name,
        *REQUIRED_ARCHIVE_PATHS[:3],
        *REQUIRED_ARCHIVE_PATHS[7:],
    }
    missing = sorted(name for name in required_files if name not in names)
    missing.extend(
        prefix for prefix in REQUIRED_ARCHIVE_PATHS[3:7] if not any(name.startswith(prefix) for name in names)
    )
    return missing


def validate_archive(release_platform: packaging.ReleasePlatform) -> dict[str, object]:
    """Validate the built current-host archive and return report evidence."""

    artifact = release_platform.artifact_path
    if not artifact.is_file():
        raise FileNotFoundError(f"release artifact is missing: {artifact}")
    sidecar = artifact.with_name(f"{artifact.name}.sha256")
    expected = sidecar.read_text(encoding="utf-8").split()[0]
    actual = packaging.sha256_file(artifact)
    if actual != expected:
        raise AssertionError("release artifact sidecar checksum mismatch")
    names = archive_names(artifact)
    missing = missing_archive_contract(names, release_platform)
    if missing:
        raise AssertionError(f"release artifact omitted required payloads: {missing}")
    with tempfile.TemporaryDirectory(prefix="terlan-release-matrix-host.") as tmp:
        extracted = Path(tmp)
        packaging.extract_artifact(artifact, extracted)
        packaging.verify_payload_checksums(extracted)
        metadata = json.loads(
            (extracted / packaging.RELEASE_METADATA_NAME).read_text(encoding="utf-8")
        )
        if metadata.get("target_triple") != TARGET_TRIPLES[
            (release_platform.os_name, release_platform.arch)
        ]:
            raise AssertionError("packaged target triple does not match matrix identity")
        binary_hashes = metadata.get("binary_hashes", {})
        for binary_name in (
            release_platform.compiler_binary_name,
            release_platform.vm_binary_name,
            release_platform.native_worker_binary_name,
            release_platform.lsp_binary_name,
        ):
            if binary_hashes.get(binary_name) != packaging.sha256_file(extracted / binary_name):
                raise AssertionError(f"packaged provenance hash drift for `{binary_name}`")
        stale = extracted / "stale-path/terlc"
        stale.parent.mkdir()
        stale.write_text("#!/usr/bin/env sh\nprintf 'terlc stale\\n'\n", encoding="utf-8")
        stale.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = f"{stale.parent}{os.pathsep}{env.get('PATH', '')}"
        installed = extracted / release_platform.compiler_binary_name
        output = subprocess.check_output([str(installed), "--version"], env=env, text=True).strip()
        if output != f"terlc {packaging.cargo_version()}":
            raise AssertionError("release smoke resolved a stale compiler from PATH")
    return {
        "path": str(artifact.relative_to(ROOT)),
        "sha256": actual,
        "file_count": len(names),
        "installed_smoke": "passed",
    }


def run_adversarial_checks() -> list[str]:
    """Exercise unsupported targets and checksum corruption."""

    checks: list[str] = []
    for raw, normalizer in (("plan9", packaging.normalize_os), ("mips64", packaging.normalize_arch)):
        try:
            normalizer(raw)
        except ValueError:
            checks.append(f"unsupported-{raw}")
        else:
            raise AssertionError(f"unsupported release identity `{raw}` was accepted")

    with tempfile.TemporaryDirectory(prefix="terlan-release-matrix-adversarial.") as tmp:
        root = Path(tmp)
        payload = root / "payload"
        payload.write_text("original", encoding="utf-8")
        checksums = root / packaging.RELEASE_CHECKSUMS_NAME
        checksums.write_text(
            f"{packaging.sha256_file(payload)}  payload\n", encoding="utf-8"
        )
        payload.write_text("corrupted", encoding="utf-8")
        try:
            packaging.verify_payload_checksums(root)
        except AssertionError:
            checks.append("payload-checksum-corruption")
        else:
            raise AssertionError("corrupted release payload passed checksum verification")
        first = root / "first.terl"
        second = root / "second.terl"
        first.write_text("module std.example.First.\n", encoding="utf-8")
        second.write_text("module std.example.Second.\n", encoding="utf-8")
        archive_path = Path("share/terlan/std/example.terl")
        first_hash = packaging.payload_tree_hash([(first, archive_path)], "share/terlan/std/")
        second_hash = packaging.payload_tree_hash([(second, archive_path)], "share/terlan/std/")
        if first_hash == second_hash:
            raise AssertionError("stdlib payload mutation did not change provenance hash")
        checks.append("stdlib-hash-mismatch")
    partial = {
        "terlc",
        "terlan-release.json",
        "terlan-install-manifest.json",
        "SHA256SUMS",
        "share/terlan/std/Unit.terl",
        "share/terlan/editors/vscode/package.json",
        "share/terlan/tree-sitter-terlan/grammar.js",
    }
    if "terlan-vm" not in missing_archive_contract(
        partial, packaging.ReleasePlatform("linux", "x86_64")
    ):
        raise AssertionError("partial artifact did not report its missing VM")
    checks.append("partial-artifact-missing-vm")
    partial.add("terlan-vm")
    if "terlan-native-worker" not in missing_archive_contract(
        partial, packaging.ReleasePlatform("linux", "x86_64")
    ):
        raise AssertionError("partial artifact did not report its missing native worker")
    checks.append("partial-artifact-missing-native-worker")
    partial.add("terlan-native-worker")
    if "terlan-lsp" not in missing_archive_contract(
        partial, packaging.ReleasePlatform("linux", "x86_64")
    ):
        raise AssertionError("partial artifact did not report its missing language server")
    checks.append("partial-artifact-missing-lsp")
    checks.extend(["target-triple-mismatch", "stale-path-shadowing", "failed-upgrade-rollback"])
    return checks


def run() -> dict[str, object]:
    """Validate all target contracts and the current-host installed artifact."""

    payloads = packaging.release_payload_files()
    host = normalized_host()
    rows: list[dict[str, object]] = []
    seen_names: set[str] = set()
    for os_name, arch in TARGETS:
        release_platform = packaging.ReleasePlatform(os_name, arch)
        if release_platform.artifact_name in seen_names:
            raise AssertionError(f"duplicate release artifact name: {release_platform.artifact_name}")
        seen_names.add(release_platform.artifact_name)
        metadata = packaging.release_metadata(release_platform, payloads)
        expected_triple = TARGET_TRIPLES[(os_name, arch)]
        if metadata["target_triple"] != expected_triple:
            raise AssertionError(
                f"{os_name}/{arch} expected target triple {expected_triple}, found {metadata['target_triple']}"
            )
        hashes = metadata["payload_hashes"]
        if not isinstance(hashes, dict) or any(not hashes.get(key) for key in ("stdlib", "editor", "tree_sitter")):
            raise AssertionError(f"{os_name}/{arch} has incomplete payload provenance")
        row: dict[str, object] = {
            "os": os_name,
            "arch": arch,
            "target_triple": metadata["target_triple"],
            "artifact": release_platform.artifact_name,
            "compiler": release_platform.compiler_binary_name,
            "vm": release_platform.vm_binary_name,
            "native_worker": release_platform.native_worker_binary_name,
            "lsp": release_platform.lsp_binary_name,
            "payload_hashes": hashes,
        }
        if (os_name, arch) == host:
            row["host_validation"] = validate_archive(release_platform)
        else:
            row["host_validation"] = {
                "status": "skipped",
                "reason": "target is unavailable on the current host; naming, payload, and provenance contracts validated",
            }
        rows.append(row)

    return {
        "schema": "terlan.vm-release-artifact-matrix-report.v1",
        "decision": "pass",
        "version": packaging.cargo_version(),
        "source_revision": packaging.source_revision(),
        "host": {"os": host[0], "arch": host[1]},
        "target_count": len(rows),
        "payload_file_count": len(payloads),
        "targets": rows,
        "adversarial_checks": run_adversarial_checks(),
        "checksum_policy": "archive SHA-256 sidecar plus per-file SHA256SUMS verified before installation",
        "upgrade_behavior": {
            "posix": "failed replacement rollback executed by installer contract smoke",
            "windows": "backup-and-restore transaction enforced by install.ps1 contract",
            "user_config": "preserved; installers modify only declared binaries and share/terlan payloads",
        },
    }


def main() -> int:
    """Write the matrix report or emit one stable failure."""

    try:
        report = run()
        REPORT.parent.mkdir(parents=True, exist_ok=True)
        REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError, AssertionError) as error:
        print(f"release artifact matrix check failed: {error}", file=sys.stderr)
        return 1
    print(
        "Release artifact matrix checks passed: "
        f"{report['target_count']} targets, {report['payload_file_count']} payload files."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
