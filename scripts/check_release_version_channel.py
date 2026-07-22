#!/usr/bin/env python3
"""Validate and update Terlan release version and channel metadata."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


RELEASE_BASE_URL = "https://github.com/terlan-lang/terlan/releases/download"


@dataclass(frozen=True)
class CheckedField:
    path: str
    field: str
    observed: str
    expected: str
    status: str


@dataclass(frozen=True)
class TextField:
    path: str
    field: str
    pattern: str
    replacement: str


def workspace_version(root: Path) -> str:
    source = (root / "Cargo.toml").read_text(encoding="utf-8")
    workspace = re.search(r"(?ms)^\[workspace\.package\]\s*(.*?)(?=^\[|\Z)", source)
    version = re.search(r'(?m)^version\s*=\s*"([^"]+)"$', workspace.group(1)) if workspace else None
    if version is None:
        raise ValueError("Cargo.toml workspace.package.version is missing")
    return version.group(1)


def expected_channel(version: str) -> str:
    return "rc" if "-" in version else "stable"


def text_fields(version: str) -> list[TextField]:
    return [
        TextField(
            "install.sh", "default_tag", r'VERSION="\$\{TERLAN_VERSION:-v[^}]+\}"',
            f'VERSION="${{TERLAN_VERSION:-v{version}}}"',
        ),
        TextField(
            "install.sh", "release_base_url",
            r'RELEASE_BASE_URL="\$\{TERLAN_RELEASE_BASE_URL:-[^}]+\}"',
            f'RELEASE_BASE_URL="${{TERLAN_RELEASE_BASE_URL:-{RELEASE_BASE_URL}}}"',
        ),
        TextField("install.ps1", "default_tag", r'\$Version = "v[^"]+"', f'$Version = "v{version}"'),
        TextField(
            "install.ps1", "release_base_url", r'\$releaseBaseUrl = "https://[^"]+"',
            f'$releaseBaseUrl = "{RELEASE_BASE_URL}"',
        ),
        TextField(
            "README.md", "current_version", r'(?m)^Current version: `[^`]+`\.?$',
            f'Current version: `{version}`.',
        ),
        TextField("README.md", "install_version", r'TERLAN_VERSION=v[^ ]+ sh', f'TERLAN_VERSION=v{version} sh'),
        TextField("CHANGELOG.md", "release_heading", rf'(?m)^## {re.escape(version)}$', f'## {version}'),
        TextField("editors/intellij/build.gradle.kts", "version", r'(?m)^version = "[^"]+"$', f'version = "{version}"'),
    ]


def checked(path: str, name: str, observed: str, expected: str) -> CheckedField:
    return CheckedField(path, name, observed, expected, "ok" if observed == expected else "mismatch")


def read_match(root: Path, spec: TextField) -> str:
    path = root / spec.path
    if not path.is_file():
        return "<missing>"
    match = re.search(spec.pattern, path.read_text(encoding="utf-8"))
    return match.group(0) if match else "<missing>"


def json_version_fields(root: Path, version: str) -> list[CheckedField]:
    fields: list[CheckedField] = []
    for relative in ("editors/vscode/package.json", "tree-sitter-terlan/package.json"):
        value = json.loads((root / relative).read_text(encoding="utf-8"))
        fields.append(checked(relative, "version", str(value.get("version", "<missing>")), version))
    for relative in ("editors/vscode/package-lock.json", "tree-sitter-terlan/package-lock.json"):
        value = json.loads((root / relative).read_text(encoding="utf-8"))
        fields.append(checked(relative, "version", str(value.get("version", "<missing>")), version))
        package_version = value.get("packages", {}).get("", {}).get("version", "<missing>")
        fields.append(checked(relative, 'packages[""].version', str(package_version), version))
    return fields


def collect_fields(root: Path, version: str) -> list[CheckedField]:
    fields = [checked("Cargo.toml", "workspace.package.version", version, version)]
    std_source = (root / "std/manifest.toml").read_text(encoding="utf-8")
    std_match = re.search(r'(?m)^version\s*=\s*"([^"]+)"$', std_source)
    std_version = std_match.group(1) if std_match else "<missing>"
    fields.append(checked("std/manifest.toml", "package.version", std_version, version))
    fields.extend(json_version_fields(root, version))
    for spec in text_fields(version):
        fields.append(checked(spec.path, spec.field, read_match(root, spec), spec.replacement))
    crate = (root / "crates/terlan/Cargo.toml").read_text(encoding="utf-8")
    inherited = bool(re.search(r"(?m)^version\.workspace\s*=\s*true$", crate))
    fields.append(checked("crates/terlan/Cargo.toml", "package.version.workspace", str(inherited).lower(), "true"))
    for relative in ("crates/terlan/src/main.rs", "crates/terlan/src/vm/main_part_001.rs"):
        present = 'env!("CARGO_PKG_VERSION")' in (root / relative).read_text(encoding="utf-8")
        fields.append(checked(relative, "runtime_version_source", str(present).lower(), "true"))
    return fields


def write_versions(root: Path, version: str) -> None:
    for relative in ("editors/vscode/package.json", "tree-sitter-terlan/package.json"):
        path = root / relative
        value = json.loads(path.read_text(encoding="utf-8"))
        value["version"] = version
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    for relative in ("editors/vscode/package-lock.json", "tree-sitter-terlan/package-lock.json"):
        path = root / relative
        value = json.loads(path.read_text(encoding="utf-8"))
        value["version"] = version
        value.setdefault("packages", {}).setdefault("", {})["version"] = version
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    (root / "std/manifest.toml").write_text(
        f'[package]\nname = "terlan-stdlib"\nversion = "{version}"\n', encoding="utf-8"
    )
    for spec in text_fields(version):
        path = root / spec.path
        source = path.read_text(encoding="utf-8")
        updated, count = re.subn(spec.pattern, spec.replacement, source, count=1)
        if count == 0 and spec.path == "CHANGELOG.md":
            updated = source.replace("# Changelog\n", f"# Changelog\n\n## {version}\n")
        elif count == 0:
            raise ValueError(f"cannot update {spec.path}:{spec.field}")
        path.write_text(updated, encoding="utf-8")


def compiler_field(root: Path, compiler: str, version: str) -> CheckedField:
    path = Path(compiler).expanduser().resolve()
    try:
        result = subprocess.run(
            [str(path), "--version"], check=True, capture_output=True, text=True, timeout=10
        )
        observed = result.stdout.strip()
    except (OSError, subprocess.SubprocessError) as error:
        observed = f"<error: {error}>"
    display = str(path.relative_to(root)) if path.is_relative_to(root) else str(path)
    return checked(display, "compiler_binary_version", observed, f"terlc {version}")


def install_url_matrix(version: str) -> list[dict[str, str]]:
    targets = (
        ("linux", "x86_64", "tar.gz"), ("linux", "aarch64", "tar.gz"),
        ("macos", "x86_64", "tar.gz"), ("macos", "aarch64", "tar.gz"),
        ("windows", "x86_64", "zip"),
    )
    return [
        {
            "os": os_name,
            "arch": arch,
            "artifact": f"terlc-{os_name}-{arch}.{extension}",
            "url": f"{RELEASE_BASE_URL}/v{version}/terlc-{os_name}-{arch}.{extension}",
        }
        for os_name, arch, extension in targets
    ]


def run(args: argparse.Namespace) -> int:
    root = args.root.resolve()
    canonical = workspace_version(root)
    requested = args.version or canonical
    if requested.startswith("v"):
        raise ValueError("version must not include leading v")
    if requested != canonical and not args.write:
        raise ValueError(f"requested version {requested} does not match workspace version {canonical}")
    if args.write:
        cargo = root / "Cargo.toml"
        source, count = re.subn(
            r'(?m)^version = "[^"]+"$', f'version = "{requested}"',
            cargo.read_text(encoding="utf-8"), count=1,
        )
        if count != 1:
            raise ValueError("cannot update Cargo.toml workspace version")
        cargo.write_text(source, encoding="utf-8")
        write_versions(root, requested)
        canonical = requested
    fields = collect_fields(root, canonical)
    required_channel = expected_channel(canonical)
    channel_ok = args.channel == "dev" or args.channel == required_channel
    fields.append(checked("<release>", "channel", args.channel, args.channel if channel_ok else required_channel))
    if args.tag:
        fields.append(checked("<release>", "tag", args.tag, f"v{canonical}"))
    if args.compiler:
        fields.append(compiler_field(root, args.compiler, canonical))
    mismatches = [asdict(item) for item in fields if item.status != "ok"]
    report = {
        "schema": "terlan-release-version-channel-v1",
        "canonicalVersion": canonical,
        "channel": args.channel,
        "tag": args.tag or f"v{canonical}",
        "checkedFields": [asdict(item) for item in fields],
        "mismatches": mismatches,
        "installUrlMatrix": install_url_matrix(canonical),
        "packageIndexStatus": "not-published" if args.channel == "dev" else "candidate",
        "artifactFilenameCoverage": 5,
        "status": "ok" if not mismatches else "failed",
    }
    report_path = args.report or root / "build/artifacts/release-version-channel-report.json"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    for mismatch in mismatches:
        print(
            f"{mismatch['path']}:{mismatch['field']}: observed {mismatch['observed']!r}; "
            f"expected {mismatch['expected']!r}", file=sys.stderr,
        )
    if mismatches:
        return 1
    print(f"Release version metadata matches {canonical} ({args.channel}).")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("version", nargs="?")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--channel", choices=("dev", "rc", "stable"), default="dev")
    parser.add_argument("--tag")
    parser.add_argument("--compiler")
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    try:
        return run(args)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"release version channel check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
