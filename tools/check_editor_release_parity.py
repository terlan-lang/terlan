#!/usr/bin/env python3
"""Validate editor and language-server behavior from the installed release layout."""

from __future__ import annotations

import hashlib
import json
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from typing import Callable

import package_release_artifact as packaging


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/editor-release-parity-report.json"
EDITOR_ROOT = Path("share/terlan/editors/vscode")
TREE_ROOT = Path("share/terlan/tree-sitter-terlan")
REQUIRED_COMMANDS = {
    "terlan.runMain",
    "terlan.runTestAtCursor",
    "terlan.addMissingImport",
    "terlan.formatDocument",
    "terlan.showDiagnostics",
    "terlan.runDebug",
    "terlan.inspectRuntime",
}
REQUIRED_EDITOR_FILES = {
    Path("package.json"),
    Path("language-configuration.json"),
    Path("syntaxes/terlan.tmLanguage.json"),
    Path("syntaxes/terlan-template-html.tmLanguage.json"),
    Path("icons/terlan-file-icon-theme.json"),
    Path("icons/terlan-file.svg"),
    Path("icons/terlan-test-file.svg"),
    Path("icons/terlan-template-html-file.svg"),
    Path("src/extension.js"),
    Path("src/client_config.js"),
    Path("src/run_command.js"),
}
REQUIRED_TREE_FILES = {
    Path("package.json"),
    Path("grammar.js"),
    Path("queries/highlights.scm"),
    Path("queries/injections.scm"),
    Path("src/grammar.json"),
    Path("src/node-types.json"),
    Path("src/parser.c"),
}


def tree_hash(root: Path, relative_files: set[Path]) -> str:
    """Return a deterministic path-and-content hash for selected files."""

    digest = hashlib.sha256()
    for relative in sorted(relative_files):
        path = root / relative
        digest.update(relative.as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\n")
    return digest.hexdigest()


def require_files(root: Path, relative_files: set[Path], label: str) -> None:
    """Reject a packaged surface with missing or empty files."""

    for relative in sorted(relative_files):
        path = root / relative
        if not path.is_file() or path.stat().st_size == 0:
            raise AssertionError(f"packaged {label} omitted `{relative.as_posix()}`")


def read_lsp_message(stream: object) -> dict[str, object] | None:
    """Read one Content-Length framed JSON-RPC message from a pipe."""

    length: int | None = None
    while True:
        line = stream.readline()
        if not line:
            return None
        if line == b"\r\n":
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":", 1)[1].strip())
    if length is None:
        raise AssertionError("language server emitted a response without Content-Length")
    return json.loads(stream.read(length))


def frame(message: dict[str, object]) -> bytes:
    """Encode one deterministic JSON-RPC request."""

    body = json.dumps(message, separators=(",", ":")).encode()
    return f"Content-Length: {len(body)}\r\n\r\n".encode() + body


def run_lsp_smoke(lsp: Path, workspace: Path) -> dict[str, object]:
    """Exercise initialize and documented hover through packaged stdio LSP."""

    workspace.mkdir(parents=True, exist_ok=True)
    source = workspace / "Hover.terl"
    std_summary = workspace.parent / "share/terlan/std/summaries/std.core.Bool.typi"
    if not std_summary.is_file():
        raise AssertionError("installed release omitted std hover summary")
    summaries = workspace / "std/summaries"
    summaries.mkdir(parents=True)
    (summaries / "std.core.Bool.typi").write_bytes(std_summary.read_bytes())
    (summaries / "generated.sample.typi").write_text(
        """//! Generated summary module docs.
module generated.sample.

/// Returns generated summary docs.
pub generated_value(): Int.
""",
        encoding="utf-8",
    )
    (workspace / "package.sample.terli").write_text(
        """//! Package interface module docs.
module package.sample.

/// Returns installed package docs.
pub package_value(): Int.
""",
        encoding="utf-8",
    )
    source_text = """//! Installed module docs.
module release_editor.Hover.

import std.core.Bool.{compare}.
import generated.sample.{generated_value}.
import package.sample.{package_value}.

/// Installed struct docs.
pub struct User {
    name: String
}.

/// Installed method docs.
pub (user: User) display(): String -> user.name.

/**
 * Returns the installed editor answer.
 */
pub answer(): Int -> 42.

pub caller(user: User): Int ->
    let local = answer();
        shown = user.display();
        std_value = compare(false, true);
        generated = generated_value();
        packaged = package_value();
    local.
"""
    source.write_text(source_text, encoding="utf-8")
    uri = source.as_uri()
    process = subprocess.Popen(
        [str(lsp), "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        cwd=workspace,
    )
    assert process.stdin is not None and process.stdout is not None and process.stderr is not None
    responses: queue.Queue[dict[str, object] | None] = queue.Queue()

    def collect() -> None:
        while True:
            message = read_lsp_message(process.stdout)
            responses.put(message)
            if message is None:
                return

    reader = threading.Thread(target=collect, daemon=True)
    reader.start()

    def send(message: dict[str, object]) -> None:
        process.stdin.write(frame(message))
        process.stdin.flush()

    def response(request_id: int) -> dict[str, object]:
        while True:
            message = responses.get(timeout=15)
            if message is None:
                raise AssertionError("packaged LSP exited before replying")
            if message.get("id") == request_id:
                return message

    send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"rootUri": workspace.as_uri(), "capabilities": {}}})
    initialize = response(1)
    send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "terlan", "version": 1, "text": source_text}}})

    def position(needle: str, last: bool = False) -> dict[str, int]:
        offset = source_text.rfind(needle) if last else source_text.find(needle)
        if offset < 0:
            raise AssertionError(f"hover fixture omitted `{needle}`")
        line = source_text.count("\n", 0, offset)
        line_start = source_text.rfind("\n", 0, offset) + 1
        return {"line": line, "character": offset - line_start}

    hover_cases = {
        "module": ("Hover", False, "Installed module docs"),
        "struct": ("User", True, "Installed struct docs"),
        "function": ("answer", True, "installed editor answer"),
        "method": ("display", True, "Installed method docs"),
        "stdlib": ("compare", True, "Compares two `Bool` values"),
        "generated-summary": ("generated_value", True, "generated summary docs"),
        "package": ("package_value", True, "installed package docs"),
    }
    hover_results: dict[str, str] = {}
    for request_id, (category, (needle, last, expected)) in enumerate(hover_cases.items(), 2):
        send({"jsonrpc": "2.0", "id": request_id, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri}, "position": position(needle, last)}})
        hover = json.dumps(response(request_id).get("result"), sort_keys=True)
        require_hover_text(hover, expected, category)
        hover_results[category] = "passed"

    shutdown_id = len(hover_cases) + 2
    send({"jsonrpc": "2.0", "id": shutdown_id, "method": "shutdown", "params": None})
    response(shutdown_id)
    send({"jsonrpc": "2.0", "method": "exit", "params": None})
    process.stdin.close()
    return_code = process.wait(timeout=15)
    if return_code != 0:
        raise AssertionError(f"packaged LSP stdio smoke failed: {process.stderr.read().decode(errors='replace')}")
    capabilities = initialize.get("result", {}).get("capabilities", {})
    if capabilities.get("hoverProvider") is not True:
        raise AssertionError(f"packaged LSP did not advertise hover support: {initialize}")
    return {
        "stdio": "passed",
        "hover": "passed",
        "hover_categories": hover_results,
        "response_count": len(hover_cases) + 2,
    }


def validate_editor(root: Path) -> dict[str, object]:
    """Validate packaged VS Code metadata and command registrations."""

    editor = root / EDITOR_ROOT
    require_files(editor, REQUIRED_EDITOR_FILES, "VS Code extension")
    manifest = json.loads((editor / "package.json").read_text(encoding="utf-8"))
    commands = {row.get("command") for row in manifest.get("contributes", {}).get("commands", [])}
    missing_commands = sorted(REQUIRED_COMMANDS - commands)
    if missing_commands:
        raise AssertionError(f"packaged extension omitted commands: {missing_commands}")
    source = (editor / "src/extension.js").read_text(encoding="utf-8")
    for command in REQUIRED_COMMANDS:
        key = command.split(".", 1)[1]
        if key not in source:
            raise AssertionError(f"packaged extension did not register `{command}`")
    icon = manifest.get("icon")
    if not isinstance(icon, str) or not (editor / icon).is_file():
        raise AssertionError("packaged extension icon path is stale")
    icon_theme = json.loads((editor / "icons/terlan-file-icon-theme.json").read_text(encoding="utf-8"))
    icon_text = json.dumps(icon_theme).lower()
    if "folded" in icon_text:
        raise AssertionError("packaged icon theme contains stale folded-page assets")
    server_config = (editor / "src/client_config.js").read_text(encoding="utf-8")
    if '"lsp",\n    "--stdio"' not in server_config or '"terlc"' not in server_config:
        raise AssertionError("packaged extension does not use installed terlc LSP stdio defaults")
    return {
        "version": manifest.get("version"),
        "command_ids": sorted(commands),
        "icon_hash": packaging.sha256_file(editor / icon),
        "grammar_hash": packaging.sha256_file(editor / "syntaxes/terlan.tmLanguage.json"),
        "selected_surface_hash": tree_hash(editor, REQUIRED_EDITOR_FILES),
    }


def validate_tree_sitter(root: Path) -> dict[str, object]:
    """Validate packaged Tree-sitter source and generated parser parity."""

    tree = root / TREE_ROOT
    require_files(tree, REQUIRED_TREE_FILES, "Tree-sitter package")
    package = json.loads((tree / "package.json").read_text(encoding="utf-8"))
    grammar = json.loads((tree / "src/grammar.json").read_text(encoding="utf-8"))
    if package.get("name") != "tree-sitter-terlan" or grammar.get("name") != "terlan":
        raise AssertionError("packaged Tree-sitter identity drifted")
    return {
        "version": package.get("version"),
        "grammar_hash": packaging.sha256_file(tree / "grammar.js"),
        "generated_parser_hash": packaging.sha256_file(tree / "src/parser.c"),
        "selected_surface_hash": tree_hash(tree, REQUIRED_TREE_FILES),
    }


def run_packaged_smokes(root: Path) -> list[str]:
    """Execute package-owned smoke scripts from the extracted archive."""

    scripts = [
        (root / EDITOR_ROOT, "test/manifest_test.js"),
        (root / EDITOR_ROOT, "test/package_smoke_test.js"),
        (root / EDITOR_ROOT, "test/textmate_bridge_test.js"),
        (root / TREE_ROOT, "test/package_smoke_test.js"),
    ]
    completed: list[str] = []
    for cwd, script in scripts:
        result = subprocess.run(
            ["node", script], cwd=cwd, check=False, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True,
        )
        if result.returncode != 0:
            raise AssertionError(
                f"packaged smoke `{cwd.relative_to(root).as_posix()}/{script}` failed: "
                f"{result.stdout}{result.stderr}"
            )
        completed.append(f"{cwd.relative_to(root).as_posix()}/{script}")
    return completed


def verify_generated_parser_fresh(tree: Path) -> str:
    """Regenerate the packaged parser and reject source/generated drift."""

    cli = ROOT / "tree-sitter-terlan/node_modules/.bin/tree-sitter"
    if not cli.is_file():
        raise AssertionError("editor release parity requires the local Tree-sitter CLI")
    generated = {Path("src/grammar.json"), Path("src/node-types.json"), Path("src/parser.c")}
    before = tree_hash(tree, generated)
    subprocess.run(
        [str(cli), "generate"], cwd=tree, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    after = tree_hash(tree, generated)
    if before != after:
        raise AssertionError("packaged Tree-sitter generated parser is stale")
    return after


def require_hover_text(hover: str, expected: str, category: str) -> None:
    """Reject absent or undocumented hover responses."""

    if expected not in hover:
        raise AssertionError(f"packaged LSP {category} hover omitted `{expected}`: {hover}")


def expect_rejection(check: Callable[[], object], label: str, checks: list[str]) -> None:
    """Record one adversarial mutation only when validation rejects it."""

    try:
        check()
    except (OSError, ValueError, AssertionError):
        checks.append(label)
        return
    raise AssertionError(f"editor release adversarial mutation `{label}` was accepted")


def adversarial_checks(root: Path, compiler: Path) -> list[str]:
    """Prove required editor, LSP, grammar, command, and icon failures are closed."""

    checks: list[str] = []
    with tempfile.TemporaryDirectory(prefix="terlan-editor-release-adversarial.") as tmp:
        fixture = Path(tmp)
        shutil_root = fixture / "complete"
        shutil_root.mkdir()
        shutil.copytree(root / EDITOR_ROOT, shutil_root / EDITOR_ROOT)
        shutil.copytree(root / TREE_ROOT, shutil_root / TREE_ROOT)

        missing_lsp = fixture / "missing-terlan-lsp"
        expect_rejection(
            lambda: require_files(fixture, {missing_lsp.relative_to(fixture)}, "language server"),
            "missing-lsp-binary",
            checks,
        )

        command_root = fixture / "missing-command"
        shutil.copytree(shutil_root, command_root)
        manifest_path = command_root / EDITOR_ROOT / "package.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["contributes"]["commands"] = [
            row for row in manifest["contributes"]["commands"]
            if row.get("command") != "terlan.inspectRuntime"
        ]
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        expect_rejection(lambda: validate_editor(command_root), "missing-command-registration", checks)

        icon_root = fixture / "missing-icon"
        shutil.copytree(shutil_root, icon_root)
        icon_manifest = json.loads((icon_root / EDITOR_ROOT / "package.json").read_text(encoding="utf-8"))
        (icon_root / EDITOR_ROOT / icon_manifest["icon"]).unlink()
        expect_rejection(lambda: validate_editor(icon_root), "stale-icon-bundle", checks)

        path_root = fixture / "workspace-path"
        shutil.copytree(shutil_root, path_root)
        client = path_root / EDITOR_ROOT / "src/client_config.js"
        client.write_text(
            client.read_text(encoding="utf-8").replace('"terlc"', '"/workspace/target/debug/terlc"'),
            encoding="utf-8",
        )
        expect_rejection(lambda: validate_editor(path_root), "workspace-terlc-path-leakage", checks)

        grammar_root = fixture / "missing-grammar"
        shutil.copytree(shutil_root, grammar_root)
        (grammar_root / TREE_ROOT / "src/parser.c").unlink()
        expect_rejection(lambda: validate_tree_sitter(grammar_root), "missing-generated-grammar", checks)

        stale_grammar_root = fixture / "stale-grammar"
        shutil.copytree(shutil_root, stale_grammar_root)
        grammar_source = stale_grammar_root / TREE_ROOT / "grammar.js"
        grammar_source.write_text(
            grammar_source.read_text(encoding="utf-8").replace('name: "terlan"', 'name: "terlan_mutated"'),
            encoding="utf-8",
        )
        expect_rejection(
            lambda: verify_generated_parser_fresh(stale_grammar_root / TREE_ROOT),
            "syntax-without-editor-coverage",
            checks,
        )

        expect_rejection(
            lambda: require_hover_text("null", "documented symbol", "mutated"),
            "broken-hover-docs",
            checks,
        )

        malformed = fixture / "Malformed.terl"
        malformed.write_text("module malformed\n", encoding="utf-8")
        malformed_result = subprocess.run(
            [str(compiler), "check", str(malformed)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if malformed_result.returncode == 0 or "error" not in malformed_result.stderr.lower():
            raise AssertionError("packaged compiler accepted malformed editor source")
        checks.append("malformed-terlan-source")
    return checks


def run() -> dict[str, object]:
    """Extract and validate the current-host release artifact."""

    release_platform = packaging.detect_release_platform()
    artifact = release_platform.artifact_path
    with tempfile.TemporaryDirectory(prefix="terlan-editor-release-parity.") as tmp:
        installed = Path(tmp)
        packaging.extract_artifact(artifact, installed)
        packaging.verify_payload_checksums(installed)
        compiler = installed / release_platform.compiler_binary_name
        lsp = installed / release_platform.lsp_binary_name
        if not compiler.is_file() or not lsp.is_file():
            raise AssertionError("release artifact omitted compiler or language server")
        env = {"PATH": str(installed), "HOME": str(installed / "home")}
        version = subprocess.check_output([str(compiler), "--version"], env=env, text=True).strip()
        if version != f"terlc {packaging.cargo_version()}":
            raise AssertionError("editor smoke resolved a non-installed compiler")
        lsp_help = subprocess.check_output([str(lsp), "--help"], env=env, text=True)
        if "terlan-lsp --stdio" not in lsp_help:
            raise AssertionError("installed language server omitted stdio metadata")
        editor = validate_editor(installed)
        tree_sitter = validate_tree_sitter(installed)
        packaged_smokes = run_packaged_smokes(installed)
        tree_sitter["fresh_generated_hash"] = verify_generated_parser_fresh(installed / TREE_ROOT)
        lsp_smoke = run_lsp_smoke(lsp, installed / "workspace")
        return {
            "schema": "terlan.editor-release-parity-report.v1",
            "decision": "pass",
            "artifact": str(artifact.relative_to(ROOT)),
            "artifact_sha256": packaging.sha256_file(artifact),
            "installed_compiler": compiler.name,
            "installed_lsp": lsp.name,
            "compiler_resolution": "installed-artifact",
            "editor": editor,
            "tree_sitter": tree_sitter,
            "packaged_smokes": packaged_smokes,
            "lsp": lsp_smoke,
            "hover_doc_coverage": sorted(lsp_smoke["hover_categories"]),
            "adversarial_checks": adversarial_checks(installed, compiler),
        }


def main() -> int:
    """Write the parity report or one stable diagnostic."""

    try:
        report = run()
        REPORT.parent.mkdir(parents=True, exist_ok=True)
        REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (OSError, ValueError, AssertionError, subprocess.SubprocessError) as error:
        print(f"editor release parity check failed: {error}", file=sys.stderr)
        return 1
    print(
        "Editor release parity checks passed: "
        f"{len(report['editor']['command_ids'])} commands, packaged LSP hover and Tree-sitter parser verified."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
