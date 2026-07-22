#!/usr/bin/env python3
"""Exercise packaged native-image admission through archive and install layouts."""

from __future__ import annotations

import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Callable

from release_transition_scan import assert_no_transition_payloads


ROOT = Path(__file__).resolve().parents[1]
TARGET_DIR = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
EXECUTABLE_SUFFIX = ".exe" if os.name == "nt" else ""
COMPILER = TARGET_DIR / "debug" / f"terlc{EXECUTABLE_SUFFIX}"
VM = TARGET_DIR / "debug" / f"terlan-vm{EXECUTABLE_SUFFIX}"
PACKAGE_IMAGE_PATH = Path("runtime/release-self-test.tvm")
ENTRY = "release_validation.Main.main"


def host_abi_contract() -> dict[str, str]:
    """Return the executable and calling-convention contract for this host."""

    system = platform.system().lower()
    machine = platform.machine().lower()
    architecture = {
        "amd64": "x86_64",
        "x86_64": "x86_64",
        "arm64": "aarch64",
        "aarch64": "aarch64",
    }.get(machine)
    if architecture is None:
        raise AssertionError(f"unsupported platform architecture: {machine}")
    if system == "linux":
        return {
            "object_format": "elf",
            "architecture": architecture,
            "operating_system": "linux",
            "calling_convention": "system_v",
        }
    if system == "darwin":
        return {
            "object_format": "mach-o",
            "architecture": architecture,
            "operating_system": "darwin",
            "calling_convention": "apple_aarch64" if architecture == "aarch64" else "system_v",
        }
    if system == "windows":
        return {
            "object_format": "pe",
            "architecture": architecture,
            "operating_system": "windows",
            "calling_convention": "windows_fastcall",
        }
    raise AssertionError(f"unsupported platform operating system: {system}")


def run(command: list[str], *, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
    """Run one package-consumer command and enforce its expected disposition."""

    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if expect_success and result.returncode != 0:
        raise AssertionError(
            f"command failed: {' '.join(command)}\n{result.stdout}{result.stderr}"
        )
    if not expect_success and result.returncode == 0:
        raise AssertionError(f"command unexpectedly succeeded: {' '.join(command)}")
    return result


def compile_image(root: Path) -> tuple[Path, dict[str, object]]:
    """Compile the release fixture and return its image plus canonical metadata."""

    source = root / "source" / "Main.terl"
    source.parent.mkdir(parents=True)
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
    build = root / "build"
    run(
        [
            str(COMPILER),
            "--out-dir",
            str(build),
            "build",
            str(source),
            "--target",
            "terlan-vm",
        ]
    )
    images = list((build / "vm").glob("*.tvm"))
    if len(images) != 1:
        raise AssertionError(f"expected one compiler-emitted .tvm image, found {images}")
    metadata_result = run(
        [
            str(VM),
            "package-image-metadata",
            str(images[0]),
            "--entry",
            ENTRY,
            "--package-path",
            PACKAGE_IMAGE_PATH.as_posix(),
        ]
    )
    metadata = json.loads(metadata_result.stdout)
    for field, expected in host_abi_contract().items():
        if metadata.get(field) != expected:
            raise AssertionError(
                f"package fixture {field} expected `{expected}`, found `{metadata.get(field)}`"
            )
    if not metadata.get("continuation_ids"):
        raise AssertionError("package fixture omitted continuation metadata")
    if int(metadata.get("native_debug_record_count", 0)) < 2:
        raise AssertionError("package fixture omitted native debug records")
    return images[0], metadata


def check_support_bundle(image: Path, metadata: dict[str, object], root: Path) -> None:
    """Validate deterministic structural support metadata for one admitted image."""

    command = [str(VM), "support-bundle", str(image)]
    first = run(command).stdout
    second = run(command).stdout
    if first != second:
        raise AssertionError("native support bundle changed across identical captures")
    bundle = json.loads(first)
    if bundle.get("schema") != "terlan.vm.native-support-bundle.v1":
        raise AssertionError("native support bundle has an unsupported schema")
    native = bundle.get("nativeImage", {})
    expected_identity = ":".join(
        str(metadata[field]) for field in ("compiler", "build", "package", "module")
    )
    if native.get("imageIdentity") != expected_identity:
        raise AssertionError("native support bundle image identity drifted")
    if native.get("descriptorDigest") != metadata.get("descriptor_digest"):
        raise AssertionError("native support bundle descriptor digest drifted")
    if native.get("continuationIds") != metadata.get("continuation_ids"):
        raise AssertionError("native support bundle continuation identity drifted")
    if native.get("generationEpoch") != 1:
        raise AssertionError("native support bundle omitted its admitted generation")
    if native.get("generationQuiescent") is not True:
        raise AssertionError("idle native support bundle must report a quiescent generation")
    rendered = json.dumps(bundle, sort_keys=True)
    for forbidden in ("coreIr", "coreIR", "instructions", "executableBytes", "sourcePath"):
        if forbidden in rendered:
            raise AssertionError(f"native support bundle leaked `{forbidden}`")

    renamed_json = root / "renamed-support-bundle.tvm"
    renamed_json.write_text("{}\n", encoding="utf-8")
    result = run([str(VM), "support-bundle", str(renamed_json)], expect_success=False)
    if "tvm.image.native_format" not in result.stderr:
        raise AssertionError("support-bundle consumer did not reject renamed JSON")


def check_direct_admission_rejections(image: Path, root: Path) -> None:
    """Reject mutable sidecars and unrelated host executables on the run path."""

    sidecars = (
        image.with_suffix(".json"),
        image.with_suffix(".tvm.json"),
        image.with_name(f"{image.name}.reuse"),
    )
    for sidecar in sidecars:
        sidecar.write_text("{}\n", encoding="utf-8")
        try:
            result = run(
                [str(VM), "run", str(image), "--entry", ENTRY, "--test-eval"],
                expect_success=False,
            )
            if "tvm.image.sidecar" not in result.stderr:
                raise AssertionError(
                    f"direct image consumer accepted mutable sidecar `{sidecar.name}`"
                )
        finally:
            sidecar.unlink(missing_ok=True)

    third_party = root / f"third-party{image.suffix}"
    shutil.copy2(VM, third_party)
    result = run(
        [str(VM), "run", str(third_party), "--entry", ENTRY, "--test-eval"],
        expect_success=False,
    )
    if "tvm.image.descriptor_section" not in result.stderr:
        raise AssertionError(
            "direct image consumer did not reject an unrelated native executable"
        )


def write_archive_package(
    root: Path, image: Path, metadata: dict[str, object]
) -> Path:
    """Write an extracted release archive layout around one native image."""

    package = root / "archive"
    destination = package / "share" / "terlan" / PACKAGE_IMAGE_PATH
    destination.parent.mkdir(parents=True)
    shutil.copy2(image, destination)
    release = {
        "schema": "terlan.release-artifact.v1",
        "target_triple": metadata["target_triple"],
        "native_self_test": metadata,
    }
    (package / "terlan-release.json").write_text(
        json.dumps(release, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return package


def validate_success(package: Path) -> None:
    """Require one package root to execute its exact native self-test image."""

    result = run([str(VM), "validate-package", str(package)])
    report = json.loads(result.stdout)
    if report.get("entry") != ENTRY:
        raise AssertionError("package validation report returned the wrong entry")


def expect_rejection(package: Path, diagnostic: str) -> None:
    """Require package admission to fail with one stable diagnostic family."""

    result = run([str(VM), "validate-package", str(package)], expect_success=False)
    if diagnostic not in result.stderr:
        raise AssertionError(
            f"expected `{diagnostic}` rejection, got:\n{result.stdout}{result.stderr}"
        )


def expect_transition_scan_rejection(package: Path, diagnostic: str) -> None:
    """Require the release-tree scan to reject one retired payload class."""

    try:
        assert_no_transition_payloads(package)
    except AssertionError as error:
        if diagnostic not in str(error):
            raise AssertionError(
                f"expected transition scan diagnostic `{diagnostic}`, got: {error}"
            ) from error
        return
    raise AssertionError("transition scan unexpectedly accepted retired runtime payload")


def clone_package(source: Path, root: Path, name: str) -> Path:
    """Clone a package fixture for one isolated negative case."""

    destination = root / name
    shutil.copytree(source, destination)
    return destination


def mutate_release(
    package: Path, mutation: Callable[[dict[str, object]], None]
) -> None:
    """Apply one metadata mutation to an archive-layout fixture."""

    path = package / "terlan-release.json"
    metadata = json.loads(path.read_text(encoding="utf-8"))
    mutation(metadata)
    path.write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def check() -> None:
    """Run positive installed/archive cycles and required adversarial rejections."""

    for binary in (COMPILER, VM):
        if not binary.is_file():
            raise FileNotFoundError(f"required debug binary is missing: {binary}")
    with tempfile.TemporaryDirectory(prefix="terlan-tvm-package-consumer.") as tmp:
        root = Path(tmp)
        image, metadata = compile_image(root)
        check_direct_admission_rejections(image, root)
        check_support_bundle(image, metadata, root)
        archive = write_archive_package(root, image, metadata)
        validate_success(archive)
        assert_no_transition_payloads(archive)

        installed = root / "installed"
        shutil.copytree(archive / "share" / "terlan", installed)
        shutil.copy2(archive / "terlan-release.json", installed / "terlan-release.json")
        validate_success(installed)
        assert_no_transition_payloads(installed)

        stale = clone_package(archive, root, "stale")
        mutate_release(
            stale,
            lambda release: release["native_self_test"].update({"sha256": "00" * 32}),
        )
        expect_rejection(stale, "tvm.package.metadata_drift")

        incompatible = clone_package(archive, root, "incompatible")
        mutate_release(
            incompatible,
            lambda release: release.update({"target_triple": "incompatible-unknown-target"}),
        )
        expect_rejection(incompatible, "tvm.package.release_target")

        sidecar = clone_package(archive, root, "sidecar")
        image_path = sidecar / "share" / "terlan" / PACKAGE_IMAGE_PATH
        image_path.with_suffix(".tvm.json").write_text("{}\n", encoding="utf-8")
        expect_rejection(sidecar, "tvm.package.sidecar")
        expect_transition_scan_rejection(sidecar, "retired runtime artifact filename")

        renamed_json = clone_package(archive, root, "renamed-json")
        renamed_path = renamed_json / "share" / "terlan" / PACKAGE_IMAGE_PATH
        renamed_path.write_text("{}\n", encoding="utf-8")
        expect_rejection(renamed_json, "tvm.image.native_format")
        expect_transition_scan_rejection(renamed_json, "JSON payload renamed as native image")

        serialized = clone_package(archive, root, "serialized-vmir")
        serialized_path = serialized / "share" / "terlan" / "runtime" / "legacy.json"
        serialized_path.write_text(
            '{"vm_ir":{"functions":[{"instructions":["interpret-me"]}]}}\n',
            encoding="utf-8",
        )
        expect_transition_scan_rejection(serialized, "serialized VMIR instruction payload")

        stale_descriptor = clone_package(archive, root, "stale-descriptor")
        mutate_release(
            stale_descriptor,
            lambda release: release["native_self_test"].update(
                {"descriptor_digest": "ff" * 32}
            ),
        )
        expect_rejection(stale_descriptor, "tvm.package.metadata_drift")

        for field in host_abi_contract():
            forged = clone_package(archive, root, f"forged-{field}")
            mutate_release(
                forged,
                lambda release, field=field: release["native_self_test"].update(
                    {field: f"forged-{field}"}
                ),
            )
            expect_rejection(forged, "tvm.package.metadata_drift")


def main() -> int:
    """Run the gate with a concise stable failure diagnostic."""

    try:
        check()
    except Exception as error:  # noqa: BLE001 - gate reports one stable diagnostic.
        print(f"TVM package/install consumer check failed: {error}", file=sys.stderr)
        return 1
    print("TVM package/install consumer check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
