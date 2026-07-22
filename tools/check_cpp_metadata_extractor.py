#!/usr/bin/env python3
"""Validate and optionally reproduce the committed Clang metadata fixture."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools" / "cpp_metadata_extractor"
FIXTURE = TOOL / "fixtures"
EXPECTED = FIXTURE / "expected-metadata.json"
SOURCE = TOOL / "src" / "main.cpp"


def load_json(path: Path) -> object:
    """Load one UTF-8 JSON document."""

    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def require(condition: bool, message: str) -> None:
    """Raise a stable validation error when a contract condition is false."""

    if not condition:
        raise ValueError(message)


def validate_offline() -> None:
    """Validate the committed result and standalone LibTooling source."""

    metadata = load_json(EXPECTED)
    require(isinstance(metadata, dict), "metadata root must be an object")
    require(metadata.get("schema") == "terlan.cpp.metadata.v1", "metadata schema drift")
    producer = metadata.get("producer")
    require(isinstance(producer, dict), "metadata producer must be an object")
    require(producer.get("name") == "clang-libtooling", "producer must be clang-libtooling")
    require("mapping" not in metadata, "extractor output must not contain package policy")

    compile_config = metadata.get("compile")
    require(isinstance(compile_config, dict), "compile configuration must be structured")
    commands = load_json(FIXTURE / "compile_commands.json")
    require(isinstance(commands, list) and len(commands) == 1, "fixture needs one compile command")
    require(
        compile_config.get("arguments") == commands[0].get("arguments"),
        "compile argument provenance drift",
    )

    symbols = metadata.get("symbols")
    require(isinstance(symbols, list) and symbols, "metadata symbols must be non-empty")
    ids = [symbol.get("id") for symbol in symbols]
    require(ids == sorted(ids), "metadata symbols must use deterministic ID ordering")
    require(len(ids) == len(set(ids)), "metadata symbol IDs must be unique")
    require(
        any(
            parameter.get("ty", {}).get("pointer_depth", 0) > 0
            for symbol in symbols
            for parameter in symbol.get("parameters", [])
        ),
        "pointer facts are missing",
    )
    require(
        any(
            symbol.get("returns", {}).get("reference") in {"lvalue", "rvalue"}
            for symbol in symbols
        ),
        "reference facts are missing",
    )
    require(
        any(symbol.get("template_parameters") for symbol in symbols),
        "template facts are missing",
    )
    require(
        any(symbol.get("fields") for symbol in symbols if symbol.get("kind") == "record"),
        "record field facts are missing",
    )
    require(
        any(symbol.get("enum_values") for symbol in symbols if symbol.get("kind") == "enum"),
        "enum value facts are missing",
    )
    require(
        any(symbol.get("overload_candidates", 0) > 1 for symbol in symbols),
        "overload-set facts are missing",
    )
    require(
        any(symbol.get("noexcept") is False for symbol in symbols if symbol.get("kind") != "record"),
        "exception facts are missing",
    )

    source = SOURCE.read_text(encoding="utf-8")
    for required in (
        "clang/Tooling/CommonOptionsParser.h",
        "clang/ASTMatchers/ASTMatchFinder.h",
        "ClangTool",
        "MatchFinder",
        "llvm/Support/JSON.h",
    ):
        require(required in source, f"extractor source is missing `{required}`")
    require("ast-dump" not in source, "extractor must not parse textual AST dumps")


def normalize_live(metadata: object) -> object:
    """Normalize the one intentionally installation-specific producer field."""

    require(isinstance(metadata, dict), "live metadata root must be an object")
    normalized = json.loads(json.dumps(metadata))
    normalized["producer"]["version"] = "<clang-version>"
    return normalized


def run(command: list[str], cwd: Path, environment: dict[str, str] | None = None) -> None:
    """Run one live extraction command and preserve its diagnostics."""

    subprocess.run(command, cwd=cwd, check=True, env=environment)


def cpp_include_environment() -> dict[str, str]:
    """Return an environment exposing the host C++ include search to LibTooling."""

    environment = os.environ.copy()
    compiler = shutil.which("g++")
    if compiler is None or environment.get("CPLUS_INCLUDE_PATH"):
        return environment
    probe = subprocess.run(
        [compiler, "-E", "-x", "c++", "-v", "-"],
        input="",
        text=True,
        capture_output=True,
        check=True,
    )
    collecting = False
    includes: list[str] = []
    for line in probe.stderr.splitlines():
        if line == "#include <...> search starts here:":
            collecting = True
        elif line == "End of search list.":
            break
        elif collecting:
            candidate = line.strip()
            path = Path(candidate)
            if path.is_dir() and "c++" in path.parts:
                includes.append(candidate)
    require(bool(includes), "host C++ compiler did not report include search paths")
    environment["CPLUS_INCLUDE_PATH"] = os.pathsep.join(includes)
    return environment


def validate_live() -> None:
    """Build LibTooling, rerun extraction, and compare normalized metadata."""

    if os.environ.get("TERLAN_CPP_METADATA_LIVE") != "1":
        print("cpp-binding-metadata-extractor-live-check: skipped; set TERLAN_CPP_METADATA_LIVE=1")
        return
    for executable in ("cmake", "clang++"):
        require(shutil.which(executable) is not None, f"live extractor requires `{executable}`")

    build = ROOT / "target" / "cpp-metadata-extractor"
    output = build / "live-metadata.json"
    live_database = build / "live-compile-database"
    live_database.mkdir(parents=True, exist_ok=True)
    commands = load_json(FIXTURE / "compile_commands.json")
    require(isinstance(commands, list), "live compilation database must be a list")
    for command in commands:
        require(isinstance(command, dict), "live compile command must be an object")
        command["directory"] = str(FIXTURE)
    with (live_database / "compile_commands.json").open("w", encoding="utf-8") as handle:
        json.dump(commands, handle, indent=2)
        handle.write("\n")
    run(["cmake", "-S", str(TOOL), "-B", str(build), "-DCMAKE_BUILD_TYPE=Release"], ROOT)
    run(["cmake", "--build", str(build), "--config", "Release"], ROOT)
    executable = build / "terlan-cpp-metadata-extractor"
    require(executable.is_file(), f"missing extractor executable `{executable}`")
    run(
        [
            str(executable),
            "-p",
            str(live_database),
            "--output",
            str(output),
            "--header",
            "extractor_fixture.hpp",
            "--namespace",
            "extractor_fixture",
            str(FIXTURE / "extractor_fixture.cpp"),
        ],
        FIXTURE,
        cpp_include_environment(),
    )
    require(normalize_live(load_json(output)) == load_json(EXPECTED), "live Clang metadata drift")


def main() -> int:
    """Run the selected offline or opt-in live extractor gate."""

    parser = argparse.ArgumentParser()
    parser.add_argument("--live", action="store_true")
    args = parser.parse_args()
    try:
        validate_offline()
        if args.live:
            validate_live()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"cpp-binding-metadata-extractor-check: {error}", file=sys.stderr)
        return 1
    print("cpp-binding-metadata-extractor-check: normalized Clang contract verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
