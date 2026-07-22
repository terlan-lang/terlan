#!/usr/bin/env python3
"""Seal and verify release artifacts before publication.

The publication path consumes only files named by ``dist/release-candidate.json``.
It never builds or regenerates release content.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[1]
DIST = Path("dist")
MANIFEST_NAME = "release-candidate.json"
REPORT = Path("target/quality/release-promotion-pipeline-report.json")
SCHEMA = "terlan.release-candidate.v1"
REPORT_SCHEMA = "terlan.release-promotion-pipeline.v1"
ARCHIVE_SUFFIXES = (".tar.gz", ".zip")
EXCLUDED_PARTS = {".git", ".lake", "__pycache__", "node_modules", "target"}


class PromotionError(ValueError):
    """Stable release-promotion contract failure."""


def sha256_bytes(payload: bytes) -> str:
    """Return a tagged SHA-256 digest."""

    return f"sha256:{hashlib.sha256(payload).hexdigest()}"


def sha256_file(path: Path) -> str:
    """Hash one release input."""

    return sha256_bytes(path.read_bytes())


def canonical_json(payload: dict[str, Any]) -> bytes:
    """Render deterministic JSON."""

    return (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()


def relative_files(root: Path, paths: Iterable[Path]) -> list[Path]:
    """Expand files and directories into a stable repository-relative list."""

    files: set[Path] = set()
    for relative in paths:
        candidate = root / relative
        if candidate.is_file():
            files.add(relative)
            continue
        if not candidate.is_dir():
            continue
        for path in candidate.rglob("*"):
            nested = path.relative_to(root)
            if path.is_file() and not EXCLUDED_PARTS.intersection(nested.parts):
                files.add(nested)
    return sorted(files, key=lambda path: path.as_posix())


def tree_digest(root: Path, paths: Iterable[Path]) -> dict[str, Any]:
    """Hash a stable set of files without embedding host paths."""

    files = relative_files(root, paths)
    rows = [
        {
            "path": path.as_posix(),
            "sha256": sha256_file(root / path),
            "size_bytes": (root / path).stat().st_size,
        }
        for path in files
    ]
    return {"file_count": len(rows), "sha256": sha256_bytes(canonical_json({"files": rows}))}


def release_archives(root: Path, dist: Path) -> list[Path]:
    """Return only publishable Terlan archives in stable order."""

    directory = root / dist
    if not directory.is_dir():
        return []
    return sorted(
        (
            path.relative_to(root)
            for path in directory.iterdir()
            if path.is_file()
            and path.name.startswith("terlc-")
            and path.name.endswith(ARCHIVE_SUFFIXES)
        ),
        key=lambda path: path.as_posix(),
    )


def read_release_metadata(root: Path, dist: Path) -> dict[str, Any]:
    """Load metadata emitted by the release artifact packager."""

    path = root / dist / "terlan-release.json"
    if not path.is_file():
        raise PromotionError(f"release metadata is missing: {path.relative_to(root)}")
    try:
        metadata = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise PromotionError(f"release metadata is invalid JSON: {error}") from error
    if metadata.get("schema") != "terlan.release-artifact.v1":
        raise PromotionError("release metadata has an unsupported schema")
    return metadata


def artifact_rows(root: Path, archives: list[Path]) -> list[dict[str, Any]]:
    """Build immutable upload rows for release archives."""

    return [
        {
            "path": path.as_posix(),
            "sha256": sha256_file(root / path),
            "size_bytes": (root / path).stat().st_size,
        }
        for path in archives
    ]


def benchmark_rows(root: Path) -> list[dict[str, str]]:
    """Record committed benchmark baselines referenced by the candidate."""

    paths = sorted(
        [*root.glob("benchmarks/**/*.baseline.json"), *root.glob("benchmarks/**/*.latest.json")],
        key=lambda path: path.as_posix(),
    )
    return [
        {"path": path.relative_to(root).as_posix(), "sha256": sha256_file(path)}
        for path in paths
        if path.is_file()
    ]


def gate_report_rows(root: Path) -> list[dict[str, str]]:
    """Record quality reports present when the candidate is sealed."""

    quality = root / "target" / "quality"
    if not quality.is_dir():
        return []
    return [
        {"path": path.relative_to(root).as_posix(), "sha256": sha256_file(path)}
        for path in sorted(quality.glob("*.json"), key=lambda item: item.as_posix())
        if path.is_file() and path.relative_to(root) != REPORT
    ]


def candidate_payload(root: Path, dist: Path, expected_version: str | None) -> dict[str, Any]:
    """Construct the unsigned candidate payload from already-built content."""

    metadata = read_release_metadata(root, dist)
    version = metadata.get("version")
    if not isinstance(version, str) or not version:
        raise PromotionError("release metadata has no version")
    if expected_version is not None and version != expected_version:
        raise PromotionError(f"release version mismatch: expected {expected_version}, found {version}")
    source_revision = metadata.get("source_revision")
    target_triple = metadata.get("target_triple")
    if not isinstance(source_revision, str) or not source_revision:
        raise PromotionError("release metadata has no source revision")
    if not isinstance(target_triple, str) or not target_triple:
        raise PromotionError("release metadata has no target triple")
    archives = release_archives(root, dist)
    if not archives:
        raise PromotionError(f"no release archives found under {dist.as_posix()}")
    vm_name = "terlan-vm.exe" if metadata.get("os") == "windows" else "terlan-vm"
    vm_path = dist / vm_name
    if not (root / vm_path).is_file():
        raise PromotionError(f"release VM binary is missing: {vm_path.as_posix()}")
    worker_name = (
        "terlan-native-worker.exe"
        if metadata.get("os") == "windows"
        else "terlan-native-worker"
    )
    worker_path = dist / worker_name
    if not (root / worker_path).is_file():
        raise PromotionError(f"release native worker is missing: {worker_path.as_posix()}")
    return {
        "schema": SCHEMA,
        "version": version,
        "source_revision": source_revision,
        "target_triples": [target_triple],
        "artifacts": artifact_rows(root, archives),
        "component_hashes": {
            "stdlib": tree_digest(root, [Path("std")]),
            "vm": {"path": vm_path.as_posix(), "sha256": sha256_file(root / vm_path)},
            "native_worker": {
                "path": worker_path.as_posix(),
                "sha256": sha256_file(root / worker_path),
            },
            "docs": tree_digest(root, [Path("README.md"), Path("docs")]),
            "editor": tree_digest(root, [Path("editors"), Path("tree-sitter-terlan")]),
        },
        "benchmark_baselines": benchmark_rows(root),
        "gate_reports": gate_report_rows(root),
        "release_notes": {
            "path": "CHANGELOG.md",
            "sha256": sha256_file(root / "CHANGELOG.md"),
        },
    }


def seal_candidate(root: Path, dist: Path, expected_version: str | None) -> Path:
    """Write a self-authenticating candidate manifest."""

    payload = candidate_payload(root, dist, expected_version)
    payload["seal"] = {
        "algorithm": "sha256",
        "digest": sha256_bytes(canonical_json(payload)),
    }
    path = root / dist / MANIFEST_NAME
    path.write_bytes(canonical_json(payload))
    return path


def read_candidate(root: Path, dist: Path) -> dict[str, Any]:
    """Load the sealed candidate manifest."""

    path = root / dist / MANIFEST_NAME
    if not path.is_file():
        raise PromotionError(f"sealed candidate manifest is missing: {path.relative_to(root)}")
    try:
        candidate = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise PromotionError(f"candidate manifest is invalid JSON: {error}") from error
    if candidate.get("schema") != SCHEMA:
        raise PromotionError("candidate manifest has an unsupported schema")
    seal = candidate.get("seal")
    unsigned = dict(candidate)
    unsigned.pop("seal", None)
    expected = sha256_bytes(canonical_json(unsigned))
    if not isinstance(seal, dict) or seal.get("algorithm") != "sha256" or seal.get("digest") != expected:
        raise PromotionError("candidate manifest seal is invalid")
    return candidate


def validate_rows(root: Path, rows: Any, label: str) -> None:
    """Validate stable path/digest rows against the current filesystem."""

    if not isinstance(rows, list):
        raise PromotionError(f"candidate {label} must be a list")
    paths = [row.get("path") for row in rows if isinstance(row, dict)]
    if len(paths) != len(rows) or paths != sorted(paths) or len(paths) != len(set(paths)):
        raise PromotionError(f"candidate {label} paths must be unique and sorted")
    for row in rows:
        path = Path(row["path"])
        if path.is_absolute() or ".." in path.parts or not (root / path).is_file():
            raise PromotionError(f"candidate {label} file is missing or unsafe: {path.as_posix()}")
        if row.get("sha256") != sha256_file(root / path):
            raise PromotionError(f"candidate {label} checksum drift: {path.as_posix()}")
        if "size_bytes" in row and row.get("size_bytes") != (root / path).stat().st_size:
            raise PromotionError(f"candidate {label} size drift: {path.as_posix()}")


def verify_candidate(root: Path, dist: Path, expected_version: str | None) -> dict[str, Any]:
    """Verify a candidate and every release input retained by its seal."""

    candidate = read_candidate(root, dist)
    if expected_version is not None and candidate.get("version") != expected_version:
        raise PromotionError(
            f"candidate version mismatch: expected {expected_version}, found {candidate.get('version')}"
        )
    validate_rows(root, candidate.get("artifacts"), "artifacts")
    listed = {Path(row["path"]) for row in candidate["artifacts"]}
    actual = set(release_archives(root, dist))
    if listed != actual:
        missing = sorted(path.as_posix() for path in listed - actual)
        extra = sorted(path.as_posix() for path in actual - listed)
        raise PromotionError(f"candidate artifact set drift: missing={missing}, extra={extra}")
    validate_rows(root, candidate.get("benchmark_baselines"), "benchmark baselines")
    validate_rows(root, candidate.get("gate_reports"), "gate reports")
    validate_rows(root, [candidate.get("release_notes")], "release notes")
    components = candidate.get("component_hashes")
    if not isinstance(components, dict):
        raise PromotionError("candidate component hashes are missing")
    expected_components = candidate_payload(root, dist, candidate["version"])["component_hashes"]
    if components != expected_components:
        raise PromotionError("candidate stdlib, VM, docs, or editor component hash drift")
    return candidate


def promotion_paths(candidate: dict[str, Any]) -> list[str]:
    """Return the exact archive and manifest upload order."""

    return [
        *[row["path"] for row in candidate["artifacts"]],
        f"{DIST.as_posix()}/{MANIFEST_NAME}",
    ]


def write_report(root: Path, candidate: dict[str, Any], synthetic: bool = False) -> Path:
    """Persist an offline promotion plan for the verified candidate."""

    report = {
        "schema": REPORT_SCHEMA,
        "decision": "pass",
        "synthetic_fixture": synthetic,
        "sealed_manifest": f"{DIST.as_posix()}/{MANIFEST_NAME}",
        "candidate_seal": candidate["seal"]["digest"],
        "version": candidate["version"],
        "artifact_hashes": candidate["artifacts"],
        "dry_run_upload_plan": promotion_paths(candidate),
        "publish_inputs": [f"{DIST.as_posix()}/{MANIFEST_NAME}"],
        "prebuilt_only": True,
        "rebuild_commands_executed": [],
    }
    path = root / REPORT
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_json(report))
    return path


def validate_contract_text(makefile: str, publisher: str) -> None:
    """Reject release wiring text that can bypass the sealed candidate."""

    required_make = (
        "release-promotion-pipeline-check:",
        "release-promotion-dry-run:",
        "tools/release_promotion_pipeline.py seal",
        "tools/release_promotion_pipeline.py dry-run",
    )
    for fragment in required_make:
        if fragment not in makefile:
            raise PromotionError(f"Makefile promotion contract is missing `{fragment}`")
    for forbidden in ("cargo build", "cargo run", "npm ", "make release-artifact"):
        if forbidden in publisher.lower():
            raise PromotionError(f"publisher contains rebuild command `{forbidden.strip()}`")
    for required in (
        "release_promotion_pipeline.py verify",
        "release_promotion_pipeline.py list",
        "release_promotion_pipeline.py digest",
    ):
        if required not in publisher:
            raise PromotionError(f"publisher does not consume sealed candidates through `{required}`")
    if "find dist" in publisher:
        raise PromotionError("publisher must not discover unsealed dist artifacts")


def validate_repository_contract(root: Path) -> None:
    """Reject repository release wiring that can bypass the sealed candidate."""

    makefile = (root / "Makefile").read_text(encoding="utf-8")
    publisher = (root / "scripts/publish_release_from_dist.sh").read_text(encoding="utf-8")
    validate_contract_text(makefile, publisher)


def create_fixture(root: Path) -> None:
    """Create a complete synthetic candidate workspace."""

    for directory in ("dist", "std", "docs", "editors", "tree-sitter-terlan", "benchmarks"):
        (root / directory).mkdir(parents=True, exist_ok=True)
    (root / "README.md").write_text("fixture\n", encoding="utf-8")
    (root / "CHANGELOG.md").write_text("## 0.0.7\n\nfixture\n", encoding="utf-8")
    for path in ("std/core.terl", "docs/index.md", "editors/editor.json", "tree-sitter-terlan/grammar.js"):
        (root / path).write_text(path + "\n", encoding="utf-8")
    (root / "benchmarks/http.baseline.json").write_text("{}\n", encoding="utf-8")
    (root / "dist/terlc-linux-x86_64.tar.gz").write_bytes(b"archive")
    (root / "dist/terlan-vm").write_bytes(b"vm")
    (root / "dist/terlan-native-worker").write_bytes(b"native-worker")
    metadata = {
        "schema": "terlan.release-artifact.v1",
        "version": "0.0.7",
        "source_revision": "a" * 40,
        "target_triple": "x86_64-unknown-linux-gnu",
        "os": "linux",
    }
    (root / "dist/terlan-release.json").write_bytes(canonical_json(metadata))


def self_test(report_root: Path | None) -> None:
    """Exercise release promotion adversarial cases with no built artifacts."""

    with tempfile.TemporaryDirectory(prefix="terlan-release-promotion-") as directory:
        root = Path(directory)
        create_fixture(root)
        seal_candidate(root, DIST, "0.0.7")
        candidate = verify_candidate(root, DIST, "0.0.7")
        assert promotion_paths(candidate) == [
            "dist/terlc-linux-x86_64.tar.gz",
            "dist/release-candidate.json",
        ]
        if report_root is not None:
            write_report(report_root, candidate, synthetic=True)
        archive = root / "dist/terlc-linux-x86_64.tar.gz"
        archive.write_bytes(b"changed")
        try:
            verify_candidate(root, DIST, "0.0.7")
        except PromotionError as error:
            assert "checksum drift" in str(error)
        else:
            raise AssertionError("checksum drift must fail")
        archive.write_bytes(b"archive")
        (root / "dist/terlc-linux-aarch64.tar.gz").write_bytes(b"extra")
        try:
            verify_candidate(root, DIST, "0.0.7")
        except PromotionError as error:
            assert "artifact set drift" in str(error)
        else:
            raise AssertionError("unsealed artifact must fail")
        (root / "dist/terlc-linux-aarch64.tar.gz").unlink()
        try:
            verify_candidate(root, DIST, "9.9.9")
        except PromotionError as error:
            assert "version mismatch" in str(error)
        else:
            raise AssertionError("version mismatch must fail")
        (root / "docs/index.md").write_text("stale\n", encoding="utf-8")
        try:
            verify_candidate(root, DIST, "0.0.7")
        except PromotionError as error:
            assert "component hash drift" in str(error)
        else:
            raise AssertionError("stale component must fail")
        (root / "docs/index.md").write_text("docs/index.md\n", encoding="utf-8")
        (root / "editors/editor.json").write_text("stale\n", encoding="utf-8")
        try:
            verify_candidate(root, DIST, "0.0.7")
        except PromotionError as error:
            assert "component hash drift" in str(error)
        else:
            raise AssertionError("stale editor package must fail")
        (root / "editors/editor.json").write_text("editors/editor.json\n", encoding="utf-8")
        (root / "CHANGELOG.md").write_text("## 0.0.7\n\nstale notes\n", encoding="utf-8")
        try:
            verify_candidate(root, DIST, "0.0.7")
        except PromotionError as error:
            assert "release notes checksum drift" in str(error)
        else:
            raise AssertionError("release notes from another candidate must fail")

    valid_makefile = """release-promotion-pipeline-check:
\tpython tools/release_promotion_pipeline.py self-test
release-promotion-dry-run:
\tpython tools/release_promotion_pipeline.py dry-run
release-artifact-current:
\tpython tools/release_promotion_pipeline.py seal
"""
    valid_publisher = """python tools/release_promotion_pipeline.py verify
python tools/release_promotion_pipeline.py list
python tools/release_promotion_pipeline.py digest
"""
    validate_contract_text(valid_makefile, valid_publisher)
    for forbidden in ("cargo build", "cargo run", "npm install", "make release-artifact-current"):
        try:
            validate_contract_text(valid_makefile, valid_publisher + forbidden)
        except PromotionError as error:
            assert "rebuild command" in str(error)
        else:
            raise AssertionError(f"publisher rebuild `{forbidden}` must fail")
    try:
        validate_contract_text(valid_makefile, valid_publisher.replace(" verify", " inspect"))
    except PromotionError as error:
        assert "does not consume sealed candidates" in str(error)
    else:
        raise AssertionError("publisher verification bypass must fail")


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse promotion helper commands."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=["seal", "verify", "dry-run", "list", "digest", "contract", "self-test"],
    )
    parser.add_argument("--version")
    parser.add_argument("--report", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    """Run one release promotion operation."""

    args = parse_args(argv)
    try:
        if args.command == "seal":
            path = seal_candidate(ROOT, DIST, args.version)
            print(f"sealed release candidate: {path.relative_to(ROOT)}")
        elif args.command == "contract":
            validate_repository_contract(ROOT)
            print("release promotion repository contract passed")
        elif args.command == "self-test":
            self_test(ROOT if args.report else None)
            print("release promotion adversarial self-tests passed")
        else:
            candidate = verify_candidate(ROOT, DIST, args.version)
            if args.command == "list":
                for path in promotion_paths(candidate):
                    sys.stdout.buffer.write(path.encode() + b"\0")
            elif args.command == "digest":
                print(candidate["seal"]["digest"])
            elif args.command == "dry-run":
                path = write_report(ROOT, candidate)
                print(f"release promotion dry-run passed; report written to {path.relative_to(ROOT)}")
            else:
                print(f"verified release candidate {candidate['version']} ({candidate['seal']['digest']})")
    except (OSError, PromotionError, json.JSONDecodeError) as error:
        print(f"release promotion failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
