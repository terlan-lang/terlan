#!/usr/bin/env python3
"""Check committed Rust-backed std SafeNative artifacts against generated output."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "std" / "RUST_BACKED_MANIFEST.tsv"
SUMMARIES = ROOT / "std" / "summaries"


@dataclass(frozen=True)
class SourceModule:
    """One source module that should emit checked SafeNative artifacts."""

    module: str
    source: Path


def load_sources() -> tuple[list[SourceModule], list[str]]:
    """Load unique Rust-backed std sources from the manifest.

    Inputs:
    - `std/RUST_BACKED_MANIFEST.tsv`.

    Outputs:
    - Unique `(module, source)` pairs in manifest order.
    - Human-readable validation errors.

    Transformation:
    - Reads the TSV, skips comments, validates the header minimally, and
      deduplicates repeated operation rows by module/source.
    """

    if not MANIFEST.is_file():
        return [], [f"missing manifest: {MANIFEST}"]

    sources: list[SourceModule] = []
    seen: set[tuple[str, Path]] = set()
    header_seen = False
    errors: list[str] = []
    for line_no, raw in enumerate(MANIFEST.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw.split("\t")
        if not header_seen:
            header_seen = True
            if parts[:2] != ["module", "source"]:
                errors.append(f"{MANIFEST}:{line_no}: invalid manifest header")
            continue
        if len(parts) != 6:
            errors.append(f"{MANIFEST}:{line_no}: expected 6 columns")
            continue
        module = parts[0]
        source = Path(parts[1])
        key = (module, source)
        if key not in seen:
            seen.add(key)
            sources.append(SourceModule(module=module, source=source))

    if not header_seen:
        errors.append(f"{MANIFEST}: missing header")

    return sources, errors


def run_emit_native_metadata(source: SourceModule, out_dir: Path) -> str | None:
    """Emit SafeNative metadata artifacts for one source module.

    Inputs:
    - `source`: manifest source module.
    - `out_dir`: temporary output directory.

    Outputs:
    - `None` when `terlc emit-native-metadata` succeeds.
    - Diagnostic text when emission fails.

    Transformation:
    - Runs the compiler with `safe_native_optional` policy so source-level
      `@compiler.native` annotations generate the same artifacts as release
      interface generation.
    """

    command = [
        "cargo",
        "run",
        "-p",
        "terlan_cli",
        "--",
        "--native-policy",
        "safe_native_optional",
        "emit-native-metadata",
        str(source.source),
        "--out-dir",
        str(out_dir),
    ]
    env = os.environ.copy()
    env.setdefault("CARGO_TERM_COLOR", "never")
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode == 0:
        return None
    output = (result.stdout + result.stderr).rstrip()
    return f"{source.source}: emit-native-metadata failed\n{output}"


def comparable_artifacts(out_dir: Path) -> list[Path]:
    """Lists generated artifacts that must be committed under `std/summaries`.

    Inputs:
    - `out_dir`: temporary output directory for one source module.

    Outputs:
    - Sorted generated `.safe_native.json` and `.safe_native.rs` files.

    Transformation:
    - Filters out `.erl` loader stubs because those are backend intermediates,
      not std summary artifacts.
    """

    return sorted(
        [
            path
            for path in out_dir.iterdir()
            if path.name.endswith(".safe_native.json")
            or path.name.endswith(".safe_native.rs")
        ]
    )


def erlang_loader_artifacts(out_dir: Path) -> list[Path]:
    """Lists generated Erlang loader artifacts.

    Inputs:
    - `out_dir`: temporary output directory for one source module.

    Outputs:
    - Sorted generated `.erl` loader files.

    Transformation:
    - Separates backend loader syntax validation from committed summary
      comparison because generated loader files are not release-owned summary
      artifacts yet.
    """

    return sorted([path for path in out_dir.iterdir() if path.suffix == ".erl"])


def compare_artifact(generated: Path) -> str | None:
    """Compare one generated artifact with its committed summary copy.

    Inputs:
    - `generated`: generated SafeNative artifact path.

    Outputs:
    - `None` when the committed file exists and matches exactly.
    - Diagnostic text when the committed file is missing or stale.

    Transformation:
    - Reads both files as UTF-8 text and compares exact contents so committed
      artifacts stay reproducible.
    """

    committed = SUMMARIES / generated.name
    if not committed.is_file():
        return f"missing committed SafeNative artifact: {committed.relative_to(ROOT)}"
    generated_text = generated.read_text(encoding="utf-8")
    committed_text = committed.read_text(encoding="utf-8")
    if generated_text != committed_text:
        return (
            "stale SafeNative artifact: "
            f"{committed.relative_to(ROOT)}; run `make stdlib-build-interfaces`"
        )
    return None


def rust_crate_name(path: Path) -> str:
    """Build a valid temporary Rust crate name for one generated artifact.

    Inputs:
    - `path`: generated Rust artifact path.

    Outputs:
    - Lowercase crate name containing only ASCII letters, digits, and
      underscores.

    Transformation:
    - Converts punctuation in generated artifact names such as
      `std_data_json_safe_native.safe_native.rs` into underscores so `rustc`
      can compile the file independently.
    """

    name = path.stem
    normalized = "".join(ch if ch.isalnum() else "_" for ch in name.lower())
    if not normalized or normalized[0].isdigit():
        return f"terlan_{normalized}"
    return normalized


def compile_rust_artifact(generated: Path, out_dir: Path) -> str | None:
    """Compile one generated SafeNative Rust artifact.

    Inputs:
    - `generated`: generated `.safe_native.rs` artifact.
    - `out_dir`: temporary directory for compiler output.

    Outputs:
    - `None` when `rustc --crate-type lib` succeeds.
    - Diagnostic text when the generated Rust file does not compile.

    Transformation:
    - Invokes `rustc` directly against the generated standalone skeleton so
      checked-in std SafeNative artifacts are guarded by syntax and
      `#![forbid(unsafe_code)]` validation, not only byte comparison.
    """

    if not generated.name.endswith(".safe_native.rs"):
        return None

    rustc = os.environ.get("RUSTC", "rustc")
    output_path = out_dir / f"{rust_crate_name(generated)}.rlib"
    result = subprocess.run(
        [
            rustc,
            "--crate-type",
            "lib",
            "--crate-name",
            rust_crate_name(generated),
            "-o",
            str(output_path),
            str(generated),
        ],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode == 0:
        return None
    output = (result.stdout + result.stderr).rstrip()
    return f"{generated.name}: rustc failed\n{output}"


def compile_erlang_loader(generated: Path, out_dir: Path) -> str | None:
    """Compile one generated Erlang SafeNative loader.

    Inputs:
    - `generated`: generated `.erl` loader path.
    - `out_dir`: temporary directory for `.beam` output.

    Outputs:
    - `None` when `erlc` accepts the loader.
    - Diagnostic text when the generated loader does not compile.

    Transformation:
    - Invokes OTP's `erlc` against every generated SafeNative loader so stdlib
      native artifact validation catches backend stub syntax drift for all
      Rust-backed std modules, not only a representative unit-test fixture.
    """

    erlc = os.environ.get("ERLC", "erlc")
    result = subprocess.run(
        [erlc, "-o", str(out_dir), str(generated)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode == 0:
        return None
    output = (result.stdout + result.stderr).rstrip()
    return f"{generated.name}: erlc failed\n{output}"


def check_source(source: SourceModule) -> list[str]:
    """Check generated artifacts for one Rust-backed source module.

    Inputs:
    - `source`: manifest source module.

    Outputs:
    - Validation errors for that module.

    Transformation:
    - Emits artifacts into a temporary directory and compares release-owned
      SafeNative JSON/Rust artifacts against `std/summaries`. It also requires
      and compiles generated Erlang SafeNative loaders so backend attachment
      scaffolding cannot disappear without failing the release gate.
    """

    errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix="terlan-std-native-artifacts-") as tmp:
        out_dir = Path(tmp)
        failure = run_emit_native_metadata(source, out_dir)
        if failure:
            return [failure]
        artifacts = comparable_artifacts(out_dir)
        if not artifacts:
            return [f"{source.source}: emitted no comparable SafeNative artifacts"]
        loaders = erlang_loader_artifacts(out_dir)
        if not loaders:
            errors.append(f"{source.source}: emitted no Erlang SafeNative loader artifact")
        for artifact in artifacts:
            error = compare_artifact(artifact)
            if error:
                errors.append(error)
            rust_error = compile_rust_artifact(artifact, out_dir)
            if rust_error:
                errors.append(rust_error)
        for loader in loaders:
            erlang_error = compile_erlang_loader(loader, out_dir)
            if erlang_error:
                errors.append(erlang_error)
    return errors


def main() -> int:
    """Run the std SafeNative artifact drift check.

    Inputs:
    - Rust-backed std manifest, source modules, compiler, and committed summary
      artifacts.

    Outputs:
    - Exit status 0 when generated artifacts match committed summaries.
    - Exit status 1 with diagnostics when any source fails or artifact drifts.

    Transformation:
    - Regenerates artifacts in temporary directories and compares them without
      modifying the working tree.
    """

    sources, errors = load_sources()
    if not errors:
        for source in sources:
            errors.extend(check_source(source))

    if errors:
        print("[stdlib-native-artifacts] failures:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print(f"[stdlib-native-artifacts] {len(sources)} Rust-backed modules match summaries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
