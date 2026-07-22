#!/usr/bin/env python3
"""Generate and validate deterministic documentation from an installed release."""

from __future__ import annotations

import hashlib
import html
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path
from typing import Callable

import package_release_artifact as packaging


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/docs-static-release-parity-report.json"
COMMANDS = (
    "init", "check", "build", "run", "scripts", "clean", "doctor", "inspect",
    "serve", "test", "doc", "api", "db", "debug", "repl", "fmt", "lint",
)
HOVER_CATEGORIES = {
    "module", "struct", "function", "method", "stdlib", "generated-summary", "package"
}
LOCAL_HREF = re.compile(r'href=["\']([^"\']+)["\']')


def hash_file(path: Path) -> str:
    """Return the SHA-256 digest of one file."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def hash_tree(root: Path) -> str:
    """Return a deterministic path-and-content hash for one site tree."""

    digest = hashlib.sha256()
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\n")
    return digest.hexdigest()


def write_json(path: Path, value: object) -> None:
    """Write deterministic formatted JSON."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def command_output(command: list[str], cwd: Path, env: dict[str, str]) -> str:
    """Run an installed command and return stdout with stable diagnostics."""

    result = subprocess.run(
        command, cwd=cwd, env=env, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, check=False,
    )
    if result.returncode != 0:
        raise AssertionError(f"installed docs command failed: {' '.join(command)}\n{result.stderr}")
    return result.stdout


def api_coverage(model: dict[str, object], share: Path) -> dict[str, object]:
    """Summarize generated public API categories and explicit availability."""

    modules = model.get("modules", [])
    if not isinstance(modules, list) or not modules:
        raise AssertionError("generated docs contain no modules")
    declarations = [
        declaration
        for module in modules if isinstance(module, dict)
        for declaration in module.get("declarations", []) if isinstance(declaration, dict)
    ]
    kinds = Counter(str(declaration.get("kind")) for declaration in declarations)
    test_modules = sum(
        1 for module in modules
        if isinstance(module, dict) and str(module.get("module", "")).endswith("Test")
    )
    native_manifests = list((share / "std/summaries").glob("*.native_boundary.json"))
    coverage = {
        "modules": len(modules),
        "structs": kinds["struct"],
        "shapes": kinds["shape"],
        "functions": kinds["function"],
        "methods": kinds["method"],
        "tests": test_modules,
        "native_boundary_capabilities": len(native_manifests),
        "packages": len(list((share / "docs/package").glob("*.md"))),
        "vm_runtime_commands": 3,
        "planned_unavailable": ["shape-runtime-extraction", "cuda-execution", "wasm-hosted-runtime"],
    }
    for key in ("modules", "structs", "functions", "methods", "tests", "native_boundary_capabilities"):
        if not coverage[key]:
            raise AssertionError(f"generated API coverage omitted `{key}`")
    return coverage


def build_search_index(model: dict[str, object], version: str) -> list[dict[str, str]]:
    """Build stable searchable module/declaration records."""

    rows: list[dict[str, str]] = []
    for module in model["modules"]:
        module_name = str(module["module"])
        page = f"{version}/api/{module_name}.html"
        rows.append({"kind": "module", "module": module_name, "name": module_name, "url": page})
        for declaration in module.get("declarations", []):
            rows.append({
                "kind": str(declaration.get("kind", "unknown")),
                "module": module_name,
                "name": str(declaration.get("name", "")),
                "url": page,
            })
    return rows


def validate_links(site: Path) -> int:
    """Reject broken local HTML links and checkout-only absolute references."""

    checked = 0
    for page in sorted(site.rglob("*.html")):
        text = page.read_text(encoding="utf-8")
        if str(ROOT) in text or "/home/" in text or "target/debug" in text:
            raise AssertionError(f"generated docs leak a source-checkout path: {page}")
        for href in LOCAL_HREF.findall(text):
            if href.startswith(("http://", "https://", "mailto:", "#", "data:")):
                continue
            target = href.split("#", 1)[0]
            if not target:
                continue
            resolved = (page.parent / target).resolve()
            if not resolved.is_file() or site.resolve() not in resolved.parents:
                raise AssertionError(f"broken generated docs link `{href}` in {page}")
            checked += 1
    return checked


def validate_readme(readme: Path, version: str) -> None:
    """Reject stale release state in the packaged README."""

    text = readme.read_text(encoding="utf-8")
    if f"Current version: `{version}`" not in text or "terlan-vm" not in text:
        raise AssertionError("packaged README does not describe the installed VM release")


def require_cli_help(help_rows: dict[str, object], version: str) -> None:
    """Reject stale or incomplete installed CLI help snapshots."""

    top_level = str(help_rows.get("top_level", ""))
    reported_version = str(help_rows.get("version", ""))
    commands = help_rows.get("commands", {})
    if reported_version.strip() != f"terlc {version}" or "terlc help" not in top_level:
        raise AssertionError("installed CLI help version is stale")
    if not isinstance(commands, dict):
        raise AssertionError("installed CLI help version or command map is stale")
    if set(COMMANDS) - commands.keys():
        raise AssertionError("installed CLI help omitted public commands")


def require_equal_hashes(first: str, second: str) -> None:
    """Reject nondeterministic generated site hashes."""

    if first != second:
        raise AssertionError("generated documentation hashes differ")


def generate_site(installed: Path, destination: Path) -> dict[str, object]:
    """Generate one offline versioned documentation site from installed inputs."""

    share = installed / "share/terlan"
    compiler = installed / ("terlc.exe" if os.name == "nt" else "terlc")
    metadata = json.loads((installed / packaging.RELEASE_METADATA_NAME).read_text(encoding="utf-8"))
    version = str(metadata["version"])
    version_root = destination / version
    api_html = version_root / "api"
    api_json = version_root / "api-model"
    env = os.environ.copy()
    env["PATH"] = f"{installed}{os.pathsep}{env.get('PATH', '')}"
    work = destination / ".work"
    work.mkdir(parents=True)

    command_output(
        [str(compiler), "--out-dir", str(api_html), "doc", "std", "--format", "html"],
        work,
        env,
    )
    command_output(
        [str(compiler), "--out-dir", str(api_json), "doc", "std", "--format", "json"],
        work,
        env,
    )
    model_path = api_json / "model.json"
    model = json.loads(model_path.read_text(encoding="utf-8"))
    coverage = api_coverage(model, share)
    search = build_search_index(model, version)

    help_rows = {
        "version": command_output([str(compiler), "--version"], work, env),
        "top_level": command_output([str(compiler), "--help"], work, env),
        "commands": {
            command: command_output([str(compiler), command, "--help"], work, env)
            for command in COMMANDS
        },
    }
    require_cli_help(help_rows, version)
    inspect = json.loads(command_output([str(compiler), "inspect", str(work), "--snapshot"], work, env))
    inspect["project"] = "<documentation-workspace>"
    inspect.get("release_layout", {})["root"] = "<installed-share-root>"
    validate_readme(share / "README.md", version)

    source_docs = version_root / "sources"
    shutil.copytree(share / "docs", source_docs / "docs")
    shutil.copy2(share / "README.md", source_docs / "README.md")
    shutil.copy2(share / "CHANGELOG.md", source_docs / "CHANGELOG.md")
    write_json(version_root / "metadata/release.json", metadata)
    write_json(version_root / "metadata/cli-help.json", help_rows)
    write_json(version_root / "metadata/runtime.json", inspect)
    write_json(version_root / "metadata/api-availability.json", coverage)
    write_json(destination / "search-index.json", {"schema": "terlan.docs-search.v1", "entries": search})
    index = (
        "<!doctype html><meta charset=\"utf-8\"><title>Terlan "
        + html.escape(version)
        + " documentation</title><h1>Terlan "
        + html.escape(version)
        + "</h1><nav><a href=\""
        + version
        + "/api/index.html\">Standard library API</a> "
        + "<a href=\"search-index.json\">Search index</a> "
        + "<a href=\""
        + version
        + "/metadata/release.json\">Release provenance</a></nav>"
    )
    (destination / "index.html").write_text(index + "\n", encoding="utf-8")
    shutil.rmtree(work)
    return {
        "version": version,
        "module_pages": len(model["modules"]),
        "generated_files": sum(1 for path in destination.rglob("*") if path.is_file()),
        "search_entries": len(search),
        "api_coverage": coverage,
        "command_help_coverage": len(COMMANDS) + 1,
        "runtime": inspect,
    }


def expect_rejection(check: Callable[[], object], label: str, results: list[str]) -> None:
    """Record an adversarial case only when validation rejects it."""

    try:
        check()
    except (OSError, ValueError, AssertionError):
        results.append(label)
        return
    raise AssertionError(f"docs adversarial mutation `{label}` was accepted")


def adversarial_checks(site: Path, version: str) -> list[str]:
    """Exercise stale inputs, broken links, missing sections, and path leakage."""

    results: list[str] = []
    with tempfile.TemporaryDirectory(prefix="terlan-docs-adversarial.") as tmp:
        root = Path(tmp)
        stale = root / "README.md"
        stale.write_text("Current version: `0.0.0`.\n", encoding="utf-8")
        expect_rejection(lambda: validate_readme(stale, version), "stale-readme", results)

        empty_model = {"modules": []}
        expect_rejection(lambda: api_coverage(empty_model, site), "missing-std-docs", results)

        broken = root / "site"
        broken.mkdir()
        (broken / "index.html").write_text('<a href="missing.html">missing</a>', encoding="utf-8")
        expect_rejection(lambda: validate_links(broken), "broken-link", results)

        leaking = root / "leaking"
        leaking.mkdir()
        (leaking / "index.html").write_text(f"<p>{ROOT}</p>", encoding="utf-8")
        expect_rejection(lambda: validate_links(leaking), "source-path-leakage", results)

        asset_leak = root / "asset-leak"
        asset_leak.mkdir()
        (asset_leak / "index.html").write_text(
            '<link href="target/debug/site.css">', encoding="utf-8"
        )
        expect_rejection(
            lambda: validate_links(asset_leak), "static-asset-checkout-reference", results
        )

        expect_rejection(
            lambda: require_hover_categories({"function"}), "missing-hover-docs", results
        )
        expect_rejection(
            lambda: require_api_sections({"modules": 1}), "missing-api-sections", results
        )
        expect_rejection(
            lambda: require_cli_help({"version": "terlc 0.0.0", "top_level": "terlc help", "commands": {}}, version),
            "stale-cli-help",
            results,
        )
        expect_rejection(
            lambda: require_equal_hashes("first", "second"),
            "nondeterministic-output",
            results,
        )
    return results


def require_hover_categories(categories: set[str]) -> None:
    """Reject incomplete installed-hover documentation coverage."""

    if categories != HOVER_CATEGORIES:
        raise AssertionError("installed hover documentation categories are incomplete")


def require_api_sections(coverage: dict[str, object]) -> None:
    """Reject API reports that omit required reference categories."""

    required = {
        "modules", "structs", "shapes", "functions", "methods", "tests",
        "native_boundary_capabilities", "packages", "vm_runtime_commands", "planned_unavailable",
    }
    if required - coverage.keys():
        raise AssertionError("generated API reference categories are incomplete")


def run() -> dict[str, object]:
    """Generate two clean sites from the current artifact and persist evidence."""

    release_platform = packaging.detect_release_platform()
    artifact = release_platform.artifact_path
    with tempfile.TemporaryDirectory(prefix="terlan-docs-release-parity.") as tmp:
        root = Path(tmp)
        installed = root / "installed"
        packaging.extract_artifact(artifact, installed)
        packaging.verify_payload_checksums(installed)
        first = root / "first"
        second = root / "second"
        first_summary = generate_site(installed, first)
        second_summary = generate_site(installed, second)
        first_hash = hash_tree(first)
        second_hash = hash_tree(second)
        if first_hash != second_hash or first_summary != second_summary:
            raise AssertionError(
                "installed documentation generation is nondeterministic: "
                f"first={first_hash} second={second_hash} "
                f"summaries_equal={first_summary == second_summary}"
            )
        links = validate_links(first)
        require_api_sections(first_summary["api_coverage"])
        editor_report = json.loads(
            (ROOT / "target/quality/editor-release-parity-report.json").read_text(encoding="utf-8")
        )
        hover_categories = set(editor_report.get("hover_doc_coverage", []))
        require_hover_categories(hover_categories)
        version = str(first_summary["version"])
        return {
            "schema": "terlan.docs-static-release-parity-report.v1",
            "decision": "pass",
            "artifact": str(artifact.relative_to(ROOT)),
            "artifact_sha256": hash_file(artifact),
            "installed_compiler": release_platform.compiler_binary_name,
            "source_layout": "installed-artifact",
            "versioned_root": f"{version}/",
            "site_sha256": first_hash,
            "deterministic_runs": 2,
            "link_checks": links,
            "hover_doc_coverage": sorted(hover_categories),
            **first_summary,
            "adversarial_checks": adversarial_checks(first, version),
        }


def main() -> int:
    """Run the gate and write its report."""

    try:
        report = run()
        REPORT.parent.mkdir(parents=True, exist_ok=True)
        write_json(REPORT, report)
    except (OSError, ValueError, AssertionError, subprocess.SubprocessError) as error:
        print(f"docs static release parity check failed: {error}", file=sys.stderr)
        return 1
    print(
        "Docs static release parity checks passed: "
        f"{report['module_pages']} modules, {report['search_entries']} search entries, "
        f"{report['link_checks']} links."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
