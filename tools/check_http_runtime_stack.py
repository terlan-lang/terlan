#!/usr/bin/env python3
"""Check that `terlc serve` stays on the approved VM-owned HTTP stack.

Inputs:
- `crates/terlan/Cargo.toml`.
- Rust source files under `crates/terlan/src/commands/serve`.
- `Makefile`.

Outputs:
- Exit status 0 when the serve command uses the VM TCP/HTTP runtime with
  maintained protocol parsers and boundary crates.
- Exit status 1 with diagnostics when required stack markers are missing or
  obvious manual TCP/HTTP parsing paths appear in the serve implementation.

Transformation:
- Scans Cargo dependencies for the approved protocol/runtime-boundary crates.
- Scans serve implementation files for required VM TCP/HTTP markers.
- Scans the Makefile for the VM-stream serve gate that proves the
  production-facing Hyper-free adapter path.
- Rejects ad hoc HTTP parsing while allowing the transitional synchronous host
  socket/rustls adapters. Socket readiness migration is tracked separately;
  this gate proves that request parsing and dispatch enter VM-owned HTTP.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys

from makefile_contract import (
    make_target_body,
    make_target_prerequisites,
    make_targets_from_text,
)


ROOT = Path(__file__).resolve().parents[1]
CLI_MANIFEST = ROOT / "crates" / "terlan" / "Cargo.toml"
MAKEFILE = ROOT / "Makefile"
SERVE_ROOT = ROOT / "crates" / "terlan" / "src" / "commands" / "serve"
SERVE_MAIN = SERVE_ROOT / "mod.rs"
SERVE_WATCH = SERVE_ROOT / "watch.rs"
NATIVE_HTTP = ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "http.rs"
NATIVE_HTTP_COOKIES = (
    ROOT / "crates" / "terlan" / "src" / "runtime" / "native" / "http" / "cookies.rs"
)
REQUIRED_DEPENDENCIES = ("http", "httparse")
REQUIRED_SERVE_MARKERS = (
    "http::{write_http1_response, VmHttpTcpServer}",
    "tcp::{VmTcpRuntime, VmTcpStream}",
    "fn serve_bound_directory_vm_stream(",
    "fn serve_vm_plain_http1_connection<S>",
    "httparse::Request::new(&mut headers)",
    "fn handle_vm_stream_http1_request(",
    "VmHttpTcpServer::new(",
)
REQUIRED_TEST_ONLY_HYPER_MARKERS = (
    "#[cfg(test)]\nuse hyper::{Request, Response};",
    "#[cfg(test)]\nasync fn handle_hyper_request",
)
REQUIRED_NATIVE_BOUNDARY_HTTP_MARKERS = (
    "pub fn content_type_for_path",
    "mime_guess::from_path(path)",
)
REQUIRED_COOKIE_BOUNDARY_MARKERS = (
    (
        NATIVE_HTTP_COOKIES,
        "Cookie::split_parse(cookie_header.to_string())",
    ),
)
REQUIRED_RELOAD_WATCH_BOUNDARY_MARKERS = (
    "pub(super) enum ReloadWatchBackend",
    "RecommendedWatcher::new",
    "RecursiveMode::Recursive",
    "fn watch_web_package_for_reload",
    "fn should_reload_for_event",
)
VM_STREAM_GATE = "vm-http-stream-serve-check"
VM_HTTP_LANE_GATE = "terlan-vm-http-lane-check"
VM_STREAM_TEST_COMMAND = (
    "$(RUST_TEST) -p terlan --bin terlc "
    "commands::serve::serve_test::vm_stream_ -- --quiet"
)
FORBIDDEN_IMPLEMENTATION_PATTERNS = (
    re.compile(r"\bread_line\s*\("),
    re.compile(r"\bread_to_end\s*\("),
    re.compile(r"\bparse_http_request\b"),
    re.compile(r"\bmatch\b.*\.extension\(\)"),
    re.compile(r"\.split\(['\"];\s*['\"]\)"),
)
FORBIDDEN_RELOAD_POLLING_PATTERNS = (
    re.compile(r"\btokio::time::interval\s*\("),
    re.compile(r"\bDefaultHasher::new\s*\("),
    re.compile(r"\bweb_package_snapshot\s*\("),
)


@dataclass(frozen=True)
class Finding:
    """HTTP runtime stack finding.

    Inputs:
    - `path`: repository-relative path to the file that owns the finding.
    - `line`: optional one-based line number.
    - `message`: human-readable explanation.

    Outputs:
    - Immutable diagnostic record.

    Transformation:
    - Keeps source location and diagnostic text together for stable checker
      output.
    """

    path: Path
    line: int | None
    message: str

    def render(self) -> str:
        """Return a stable diagnostic line.

        Inputs:
        - Finding path, optional line, and message.

        Outputs:
        - `path: message` or `path:line: message`.

        Transformation:
        - Formats line-aware findings without exposing unrelated file content.
        """

        if self.line is None:
            return f"{self.path}: {self.message}"
        return f"{self.path}:{self.line}: {self.message}"


def relative(path: Path) -> Path:
    """Return a repository-relative path.

    Inputs:
    - Absolute path inside the repository.

    Outputs:
    - Path relative to `ROOT`.

    Transformation:
    - Normalizes diagnostics so output is stable across machines.
    """

    return path.relative_to(ROOT)


def read_text(path: Path) -> str:
    """Read UTF-8 source text.

    Inputs:
    - Existing repository path.

    Outputs:
    - File contents as text.

    Transformation:
    - Uses explicit UTF-8 decoding because all checked files are source files.
    """

    return path.read_text(encoding="utf-8")


def dependency_findings() -> list[Finding]:
    """Return missing approved HTTP dependency findings.

    Inputs:
    - `crates/terlan/Cargo.toml`.

    Outputs:
    - Finding records for missing dependency declarations.

    Transformation:
    - Uses a conservative TOML-line regex because this checker validates
      dependency presence, not manifest semantics.
    """

    text = read_text(CLI_MANIFEST)
    findings: list[Finding] = []
    for name in REQUIRED_DEPENDENCIES:
        pattern = re.compile(rf"^\s*{re.escape(name)}\s*=", re.MULTILINE)
        if not pattern.search(text):
            findings.append(
                Finding(
                    path=relative(CLI_MANIFEST),
                    line=None,
                    message=f"missing approved HTTP runtime dependency `{name}`",
                )
            )
    return findings


def serve_marker_findings() -> list[Finding]:
    """Return missing VM HTTP serve marker findings.

    Inputs:
    - Main serve implementation source.

    Outputs:
    - Finding records for missing required implementation markers.

    Transformation:
    - Checks for explicit marker strings that encode the VM-owned TCP/HTTP
      request path and keep the comparison-only Hyper adapter test-scoped.
    """

    text = read_text(SERVE_MAIN)
    findings: list[Finding] = []
    for marker in REQUIRED_SERVE_MARKERS:
        if marker not in text:
            findings.append(
                Finding(
                    path=relative(SERVE_MAIN),
                    line=None,
                    message=f"missing HTTP runtime marker `{marker}`",
                )
            )
    for marker in REQUIRED_TEST_ONLY_HYPER_MARKERS:
        if marker not in text:
            findings.append(
                Finding(
                    path=relative(SERVE_MAIN),
                    line=None,
                    message=f"missing test-only Hyper boundary marker `{marker}`",
                )
            )
    return findings


def serve_source_files() -> list[Path]:
    """Return serve implementation Rust files.

    Inputs:
    - `crates/terlan/src/commands/serve`.

    Outputs:
    - Sorted Rust implementation paths excluding tests.

    Transformation:
    - Keeps test fixtures out of forbidden-pattern scanning because tests may
      contain raw request text while production code must not parse HTTP text.
    """

    return [
        path
        for path in sorted(SERVE_ROOT.rglob("*.rs"))
        if not path.name.endswith("_test.rs")
    ]


def forbidden_pattern_findings() -> list[Finding]:
    """Return ad hoc HTTP implementation findings.

    Inputs:
    - Non-test serve implementation Rust files.

    Outputs:
    - Finding records for obvious ad hoc request parsing paths.

    Transformation:
    - Searches line by line so violations point at the exact regression.
    """

    findings: list[Finding] = []
    for path in serve_source_files():
        text = read_text(path)
        for line_no, line in enumerate(text.splitlines(), 1):
            for pattern in FORBIDDEN_IMPLEMENTATION_PATTERNS:
                if pattern.search(line):
                    findings.append(
                        Finding(
                            path=relative(path),
                            line=line_no,
                            message="ad hoc HTTP implementation marker is forbidden in serve runtime",
                        )
                    )
    return findings


def native_http_boundary_findings() -> list[Finding]:
    """Return native HTTP boundary findings.

    Inputs:
    - `crates/terlan/src/runtime/native/http.rs`.

    Outputs:
    - Finding records when the temporary MIME boundary is not centralized in
      the native HTTP adapter.

    Transformation:
    - Requires the single adapter-owned `content_type_for_path` helper and its
      replacement note so manual MIME lookup does not creep back into `terlc
      serve` while the release waits for a maintained `mime_guess` dependency.
    """

    text = read_text(NATIVE_HTTP)
    findings: list[Finding] = []
    for marker in REQUIRED_NATIVE_BOUNDARY_HTTP_MARKERS:
        if marker not in text:
            findings.append(
                Finding(
                    path=relative(NATIVE_HTTP),
                    line=None,
                    message=f"missing native HTTP boundary marker `{marker}`",
                )
            )
    return findings


def cookie_boundary_findings() -> list[Finding]:
    """Return cookie parsing boundary findings.

    Inputs:
    - `crates/terlan/src/runtime/native/http/cookies.rs`.

    Outputs:
    - Finding records when request-cookie parsing is not routed through the
      NativeBoundary HTTP boundary.

    Transformation:
    - Requires the native parser to retain its maintained-crate replacement
      note. Non-test serve files are also covered by the general forbidden
      pattern scan, which rejects local semicolon splitting.
    """

    findings: list[Finding] = []
    for path, marker in REQUIRED_COOKIE_BOUNDARY_MARKERS:
        text = read_text(path)
        if marker not in text:
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message=f"missing cookie boundary marker `{marker}`",
                )
            )
    return findings


def reload_watch_boundary_findings() -> list[Finding]:
    """Return live-reload watcher boundary findings.

    Inputs:
    - `crates/terlan/src/commands/serve/watch.rs`.
    - Non-test serve implementation Rust files.

    Outputs:
    - Finding records when maintained notify watcher integration is missing or
      polling/hash snapshot implementation markers appear in production serve
      modules.

    Transformation:
    - Requires the explicit notify backend markers in `watch.rs` and rejects
      polling/hash snapshot implementation markers in production serve modules.
      The HTTP request path may start the watcher, but it must not own watch
      implementation details.
    """

    findings: list[Finding] = []
    watch_text = read_text(SERVE_WATCH)
    for marker in REQUIRED_RELOAD_WATCH_BOUNDARY_MARKERS:
        if marker not in watch_text:
            findings.append(
                Finding(
                    path=relative(SERVE_WATCH),
                    line=None,
                    message=f"missing reload watch boundary marker `{marker}`",
                )
            )

    for path in serve_source_files():
        text = read_text(path)
        for line_no, line in enumerate(text.splitlines(), 1):
            for pattern in FORBIDDEN_RELOAD_POLLING_PATTERNS:
                if pattern.search(line):
                    findings.append(
                        Finding(
                            path=relative(path),
                            line=line_no,
                            message="reload polling/hash snapshot implementation is forbidden in serve runtime",
                        )
                    )
    return findings


def vm_stream_gate_findings() -> list[Finding]:
    """Return VM-stream serve gate findings.

    Inputs:
    - Repository `Makefile`.

    Outputs:
    - Finding records when the Hyper-free VM stream serve matrix is not exposed
      as a named gate or prerequisite of the VM HTTP lane.

    Transformation:
    - Requires the target declaration, broad VM stream selector, and lane
      prerequisite so production-facing VM stream behavior cannot remain
      hidden as ad hoc inline tests.
    """

    text = read_text(MAKEFILE)
    findings: list[Finding] = []
    targets = make_targets_from_text(text)
    if VM_STREAM_GATE not in targets:
        findings.append(
            Finding(
                path=relative(MAKEFILE),
                line=None,
                message=f"missing VM stream serve gate `{VM_STREAM_GATE}`",
            )
        )
        return findings

    gate_body = make_target_body(text, VM_STREAM_GATE) or []
    if VM_STREAM_TEST_COMMAND not in gate_body:
        findings.append(
            Finding(
                path=relative(MAKEFILE),
                line=None,
                message=(
                    f"VM stream serve gate `{VM_STREAM_GATE}` must run canonical "
                    f"grouped selector `{VM_STREAM_TEST_COMMAND}`"
                ),
            )
        )

    lane_prerequisites = make_target_prerequisites(text, VM_HTTP_LANE_GATE) or []
    if VM_STREAM_GATE not in lane_prerequisites:
        findings.append(
            Finding(
                path=relative(MAKEFILE),
                line=None,
                message=(
                    f"VM HTTP lane `{VM_HTTP_LANE_GATE}` must declare "
                    f"`{VM_STREAM_GATE}` as a prerequisite"
                ),
            )
        )
    return findings


def check_http_runtime_stack() -> list[Finding]:
    """Return all HTTP runtime stack findings.

    Inputs:
    - CLI manifest and serve implementation source files.

    Outputs:
    - Finding records for every stack-boundary violation.

    Transformation:
    - Combines dependency, required marker, and forbidden pattern checks.
    """

    return (
        dependency_findings()
        + serve_marker_findings()
        + forbidden_pattern_findings()
        + native_http_boundary_findings()
        + cookie_boundary_findings()
        + reload_watch_boundary_findings()
        + vm_stream_gate_findings()
    )


def main() -> int:
    """Run the HTTP runtime stack checker.

    Inputs:
    - Repository files addressed by module constants.

    Outputs:
    - Process exit code.

    Transformation:
    - Prints stable diagnostics for findings and a compact success message when
      the approved stack boundary holds.
    """

    findings = check_http_runtime_stack()
    if findings:
        for finding in findings:
            print(finding.render())
        return 1
    print("HTTP runtime stack boundary OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
