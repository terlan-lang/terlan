#!/usr/bin/env python3
"""Validate the Rust-backed standard-library operation manifest."""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "std" / "RUST_BACKED_MANIFEST.tsv"
EXPECTED_HEADER = ["module", "source", "crate", "operation", "function", "arity"]
ALLOWED_CRATES = {
    "serde_json",
    "base64",
    "std::path",
    "url",
    "std::http",
    "tokio-postgres",
    "std::vec",
}
ADAPTERS = {
    "std.data.Json": (
        "serde_json",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "json.rs",
    ),
    "std.encoding.Base64": (
        "base64",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "base64.rs",
    ),
    "std.io.Path": (
        "std::path",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "path.rs",
    ),
    "std.net.Uri": (
        "url",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "uri.rs",
    ),
    "std.http.Request": (
        "std::http",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "http.rs",
    ),
    "std.http.Cookies": (
        "std::http",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "http.rs",
    ),
    "std.http.Response": (
        "std::http",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "http.rs",
    ),
    "std.db.Postgres": (
        "tokio-postgres",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "postgres.rs",
    ),
    "std.native.collections.Vector": (
        "std::vec",
        ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "vector.rs",
    ),
}


@dataclass(frozen=True)
class ManifestRow:
    """One Rust-backed operation row loaded from the manifest."""

    module: str
    source: Path
    crate: str
    operation: str
    function: str
    arity: int


@dataclass(frozen=True)
class NativeOperation:
    """One `@compiler.native` operation parsed from a Terlan source file."""

    module: str
    source: Path
    operation: str
    function: str
    arity: int


def load_manifest(path: Path) -> tuple[list[ManifestRow], list[str]]:
    """Load and validate manifest rows.

    Inputs:
    - `path`: manifest TSV path.

    Outputs:
    - Parsed manifest rows.
    - Human-readable validation errors.

    Transformation:
    - Reads the TSV, skips comments, validates the header and arity field, and
      normalizes source paths relative to the repository root.
    """

    rows: list[ManifestRow] = []
    errors: list[str] = []
    if not path.is_file():
        return rows, [f"missing manifest: {path}"]

    header_seen = False
    for line_no, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw.split("\t")
        if not header_seen:
            header_seen = True
            if parts != EXPECTED_HEADER:
                errors.append(f"{path}:{line_no}: invalid header {parts!r}")
            continue
        if len(parts) != len(EXPECTED_HEADER):
            errors.append(f"{path}:{line_no}: expected {len(EXPECTED_HEADER)} columns")
            continue
        module, source, crate, operation, function, arity_text = parts
        try:
            arity = int(arity_text)
        except ValueError:
            errors.append(f"{path}:{line_no}: invalid arity `{arity_text}`")
            continue
        rows.append(
            ManifestRow(
                module=module,
                source=Path(source),
                crate=crate,
                operation=operation,
                function=function,
                arity=arity,
            )
        )

    if not header_seen:
        errors.append(f"{path}: missing header")

    return rows, errors


def declared_module(source: str) -> str | None:
    """Extract a source module declaration.

    Inputs:
    - `source`: Terlan source text.

    Outputs:
    - Module name without the trailing period, or `None` when absent.

    Transformation:
    - Scans line by line for the first `module ... .` declaration.
    """

    for raw in source.splitlines():
        line = raw.strip()
        if line.startswith("module ") and line.endswith("."):
            return line[len("module ") : -1].strip()
    return None


def split_top_level(input_text: str, delimiter: str) -> list[str]:
    """Split source text at top-level delimiters.

    Inputs:
    - `input_text`: text to split.
    - `delimiter`: single-character delimiter.

    Outputs:
    - Source fragments split outside nested parentheses, brackets, or braces.

    Transformation:
    - Walks the string while tracking nesting depth so generic types such as
      `Result[String, Error]` do not split their internal comma.
    """

    parts: list[str] = []
    start = 0
    paren = 0
    bracket = 0
    brace = 0
    for index, char in enumerate(input_text):
        if char == "(":
            paren += 1
        elif char == ")":
            paren -= 1
        elif char == "[":
            bracket += 1
        elif char == "]":
            bracket -= 1
        elif char == "{":
            brace += 1
        elif char == "}":
            brace -= 1
        elif char == delimiter and paren == 0 and bracket == 0 and brace == 0:
            parts.append(input_text[start:index])
            start = index + len(char)
    parts.append(input_text[start:])
    return parts


def count_params(params: str) -> int:
    """Count function parameters in a source signature.

    Inputs:
    - `params`: text between the signature's outer parentheses.

    Outputs:
    - Number of top-level parameter entries.

    Transformation:
    - Treats an empty parameter list as zero; otherwise delegates to
      `split_top_level` and counts non-empty fragments.
    """

    if not params.strip():
        return 0
    return len([part for part in split_top_level(params, ",") if part.strip()])


def parse_pub_signature(line: str) -> tuple[str, int] | None:
    """Parse a public function signature line.

    Inputs:
    - `line`: trimmed Terlan source line beginning with `pub`.

    Outputs:
    - Function name and arity, including receiver arity for receiver methods.
    - `None` when the line is not a supported one-line function signature.

    Transformation:
    - Recognizes normal functions and receiver methods, then counts top-level
      value parameters.
    """

    receiver = re.match(
        r"^pub\s+\([^)]*\)\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:\[[^\]]+\])?\((.*)\)\s*:",
        line,
    )
    if receiver:
        return receiver.group(1), count_params(receiver.group(2)) + 1

    normal = re.match(
        r"^pub\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:\[[^\]]+\])?\((.*)\)\s*:",
        line,
    )
    if normal:
        return normal.group(1), count_params(normal.group(2))
    return None


def parse_native_operations(path: Path) -> tuple[str | None, list[NativeOperation], list[str]]:
    """Parse compiler-native operations from one Terlan source file.

    Inputs:
    - `path`: source path relative to the repository root.

    Outputs:
    - Declared module name, parsed operations, and validation errors.

    Transformation:
    - Scans `@compiler.native {operation}` annotations and pairs each one with
      the following public function or receiver-method signature.
    """

    absolute = ROOT / path
    if not absolute.is_file():
        return None, [], [f"missing source: {path}"]

    source = absolute.read_text(encoding="utf-8")
    module = declared_module(source)
    pending: tuple[str, int] | None = None
    operations: list[NativeOperation] = []
    errors: list[str] = []

    for line_no, raw in enumerate(source.splitlines(), 1):
        line = raw.strip()
        native = re.match(r"^@compiler\.native\s+\{([^}]+)\}$", line)
        if native:
            if pending is not None:
                errors.append(f"{path}:{line_no}: previous @compiler.native has no signature")
            pending = (native.group(1).strip(), line_no)
            continue

        if pending is None:
            continue
        signature = parse_pub_signature(line)
        if signature is None:
            if line.startswith("@compiler.") or line.startswith("pub "):
                errors.append(f"{path}:{line_no}: cannot parse native signature `{line}`")
                pending = None
            continue

        operation, _annotation_line = pending
        function, arity = signature
        operations.append(
            NativeOperation(
                module=module or "",
                source=path,
                operation=operation,
                function=function,
                arity=arity,
            )
        )
        pending = None

    if pending is not None:
        operation, line_no = pending
        errors.append(f"{path}:{line_no}: @compiler.native `{operation}` has no signature")

    return module, operations, errors


def validate_manifest(rows: list[ManifestRow]) -> list[str]:
    """Validate manifest rows against source annotations.

    Inputs:
    - `rows`: parsed manifest rows.

    Outputs:
    - Human-readable validation errors.

    Transformation:
    - Compares manifest operations to parsed source operations in both
      directions and checks crate/module/source consistency.
    """

    errors: list[str] = []
    seen_keys: set[tuple[str, str]] = set()
    rows_by_source: dict[Path, list[ManifestRow]] = {}
    for row in rows:
        if row.crate not in ALLOWED_CRATES:
            errors.append(f"{row.source}: unsupported Rust crate `{row.crate}`")
        key = (row.module, row.operation)
        if key in seen_keys:
            errors.append(f"{row.source}: duplicate operation `{row.operation}`")
        seen_keys.add(key)
        rows_by_source.setdefault(row.source, []).append(row)

    for source, source_rows in sorted(rows_by_source.items()):
        module, operations, parse_errors = parse_native_operations(source)
        errors.extend(parse_errors)
        if module is None:
            continue

        declared_modules = {row.module for row in source_rows}
        if declared_modules != {module}:
            errors.append(
                f"{source}: manifest module(s) {sorted(declared_modules)} do not match `{module}`"
            )

        parsed_by_operation = {operation.operation: operation for operation in operations}
        row_by_operation = {row.operation: row for row in source_rows}

        for operation, parsed in sorted(parsed_by_operation.items()):
            row = row_by_operation.get(operation)
            if row is None:
                errors.append(f"{source}: missing manifest row for `{operation}`")
                continue
            if row.function != parsed.function or row.arity != parsed.arity:
                errors.append(
                    f"{source}: `{operation}` manifest has {row.function}/{row.arity}, "
                    f"source has {parsed.function}/{parsed.arity}"
                )

        for operation in sorted(set(row_by_operation) - set(parsed_by_operation)):
            errors.append(f"{source}: manifest row `{operation}` is not in source")

    return errors


def rust_public_functions(path: Path) -> tuple[set[str], list[str]]:
    """Parse public Rust adapter function names from one SafeNative file.

    Inputs:
    - `path`: absolute path to a Rust SafeNative adapter module.

    Outputs:
    - Public function names defined in the adapter module.
    - Human-readable validation errors.

    Transformation:
    - Reads the Rust file and extracts stable `pub fn name(...)` declarations.
      This intentionally checks only adapter symbol presence; Rust's own type
      checker remains responsible for function signatures.
    """

    if not path.is_file():
        return set(), [f"missing SafeNative adapter: {path.relative_to(ROOT)}"]

    source = path.read_text(encoding="utf-8")
    functions = set()
    for match in re.finditer(
        r"(?m)^\s*pub\s+fn\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)(?:<[^>]+>)?\s*\(",
        source,
    ):
        functions.add(match.group(1))
    return functions, []


def validate_adapter_symbols(rows: list[ManifestRow]) -> list[str]:
    """Validate manifest rows against concrete SafeNative adapter modules.

    Inputs:
    - `rows`: parsed manifest rows.

    Outputs:
    - Human-readable validation errors.

    Transformation:
    - Groups rows by Terlan module, checks each module has a known Rust adapter
      mapping, verifies the selected backend crate name, and confirms every
      manifest function is present as a public Rust function in the adapter.
    """

    errors: list[str] = []
    rows_by_module: dict[str, list[ManifestRow]] = {}
    for row in rows:
        rows_by_module.setdefault(row.module, []).append(row)

    function_cache: dict[Path, set[str]] = {}
    for module, module_rows in sorted(rows_by_module.items()):
        adapter = ADAPTERS.get(module)
        if adapter is None:
            errors.append(f"{module}: missing SafeNative adapter mapping")
            continue

        expected_crate, adapter_path = adapter
        crates = {row.crate for row in module_rows}
        if crates != {expected_crate}:
            errors.append(
                f"{module}: manifest crate(s) {sorted(crates)} do not match `{expected_crate}`"
            )

        functions = function_cache.get(adapter_path)
        if functions is None:
            functions, parse_errors = rust_public_functions(adapter_path)
            function_cache[adapter_path] = functions
            errors.extend(parse_errors)
        for row in module_rows:
            if row.function not in functions:
                errors.append(
                    f"{row.source}: `{row.operation}` maps to missing SafeNative "
                    f"function `{row.function}` in {adapter_path.relative_to(ROOT)}"
                )

    return errors


def rust_test_path(adapter_path: Path) -> Path:
    """Return the adjacent Rust test module path for one adapter.

    Inputs:
    - `adapter_path`: absolute SafeNative adapter path.

    Outputs:
    - Absolute adjacent test path with `_test.rs` suffix.

    Transformation:
    - Replaces the adapter file suffix with the project's adjacent test-module
      naming convention.
    """

    return adapter_path.with_name(f"{adapter_path.stem}_test.rs")


def rust_test_references(path: Path) -> tuple[str, list[str]]:
    """Read one adjacent Rust test module.

    Inputs:
    - `path`: absolute test module path.

    Outputs:
    - Test source text and validation diagnostics.

    Transformation:
    - Converts a missing or non-test file into normal manifest diagnostics so
      the release gate explains which adapter lacks executable test coverage.
    """

    if not path.is_file():
        return "", [f"missing SafeNative adapter test: {path.relative_to(ROOT)}"]
    source = path.read_text(encoding="utf-8")
    if "#[test]" not in source:
        return source, [f"{path.relative_to(ROOT)}: missing #[test] functions"]
    return source, []


def references_function(source: str, function: str) -> bool:
    """Return whether a Rust test source references an adapter function.

    Inputs:
    - `source`: adjacent Rust test source text.
    - `function`: manifest function name without raw-identifier prefix.

    Outputs:
    - `True` when the test source calls the function by normal or raw
      identifier spelling.

    Transformation:
    - Uses a lightweight Rust-call pattern because the manifest gate only needs
      coverage evidence; Rust's own test runner validates the call typechecks.
    """

    pattern = rf"(?<![A-Za-z0-9_])(?:r#)?{re.escape(function)}\s*\("
    return re.search(pattern, source) is not None


def validate_adapter_tests(rows: list[ManifestRow]) -> list[str]:
    """Validate adjacent SafeNative test coverage for manifest functions.

    Inputs:
    - `rows`: parsed Rust-backed manifest rows.

    Outputs:
    - Human-readable diagnostics for missing adapter tests or unreferenced
      manifest functions.

    Transformation:
    - Groups rows by Terlan module, resolves the known adapter mapping, loads
      the adjacent `_test.rs` module once, and requires each manifest function
      to appear in an executable test source.
    """

    errors: list[str] = []
    rows_by_module: dict[str, list[ManifestRow]] = {}
    for row in rows:
        rows_by_module.setdefault(row.module, []).append(row)

    test_cache: dict[Path, str] = {}
    for module, module_rows in sorted(rows_by_module.items()):
        adapter = ADAPTERS.get(module)
        if adapter is None:
            continue
        _expected_crate, adapter_path = adapter
        test_path = rust_test_path(adapter_path)
        source = test_cache.get(test_path)
        if source is None:
            source, read_errors = rust_test_references(test_path)
            test_cache[test_path] = source
            errors.extend(read_errors)
        if not source:
            continue

        for row in module_rows:
            if not references_function(source, row.function):
                errors.append(
                    f"{row.source}: `{row.operation}` maps to `{row.function}`, "
                    f"but {test_path.relative_to(ROOT)} does not reference it"
                )

    return errors


def main() -> int:
    """Validate the checked-in Rust-backed std operation manifest.

    Inputs:
    - `std/RUST_BACKED_MANIFEST.tsv`.
    - Referenced Terlan source files.

    Outputs:
    - Exit status 0 when the manifest and source annotations match.
    - Exit status 1 with diagnostics when any row is stale or missing.

    Transformation:
    - Loads the manifest, parses source annotations, compares operation
      coverage, and prints a compact success or failure summary.
    """

    rows, errors = load_manifest(MANIFEST)
    errors.extend(validate_manifest(rows))
    errors.extend(validate_adapter_symbols(rows))
    errors.extend(validate_adapter_tests(rows))
    if errors:
        print("[rust-backed-std-manifest] failures:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print(f"[rust-backed-std-manifest] {len(rows)} operations are covered.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
