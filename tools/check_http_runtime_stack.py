#!/usr/bin/env python3
"""Check that `terlc serve` stays on the approved HTTP runtime stack.

Inputs:
- `crates/terlan_cli/Cargo.toml`.
- Rust source files under `crates/terlan_cli/src/commands/serve`.

Outputs:
- Exit status 0 when the serve command depends on and uses Hyper/Tokio/http.
- Exit status 1 with diagnostics when required stack markers are missing or
  obvious manual TCP/HTTP parsing paths appear in the serve implementation.

Transformation:
- Scans Cargo dependencies for the approved runtime crates.
- Scans serve implementation files for required Hyper/Tokio markers.
- Rejects manual stream/request parsing symbols while allowing the current
  synchronous `std::net::TcpListener` bind that is immediately adopted by
  `tokio::net::TcpListener`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
CLI_MANIFEST = ROOT / "crates" / "terlan_cli" / "Cargo.toml"
SERVE_ROOT = ROOT / "crates" / "terlan_cli" / "src" / "commands" / "serve"
SERVE_MAIN = SERVE_ROOT / "mod.rs"
SERVE_BEAM_EVAL = SERVE_ROOT / "handler" / "beam_eval.rs"
SERVE_WATCH = SERVE_ROOT / "watch.rs"
SAFENATIVE_HTTP = ROOT / "crates" / "terlan_safenative" / "src" / "http.rs"
SAFENATIVE_HTTP_COOKIES = (
    ROOT / "crates" / "terlan_safenative" / "src" / "http" / "cookies.rs"
)
REQUIRED_DEPENDENCIES = ("http", "http-body-util", "hyper", "hyper-util", "tokio")
REQUIRED_SERVE_MARKERS = (
    "use hyper::server::conn::http1;",
    "use hyper::service::service_fn;",
    "use hyper_util::rt::TokioIo;",
    "use tokio::net::TcpListener;",
    "async fn handle_hyper_request",
    "http1::Builder::new().serve_connection",
)
REQUIRED_SAFENATIVE_HTTP_MARKERS = (
    "pub fn content_type_for_path",
    "mime_guess::from_path(path)",
)
REQUIRED_COOKIE_BOUNDARY_MARKERS = (
    (
        SERVE_BEAM_EVAL,
        "native_http::parse_request_cookie_header(cookie_header)",
    ),
    (
        SAFENATIVE_HTTP_COOKIES,
        "Cookie::parse(pair.trim().to_string())",
    ),
)
REQUIRED_RELOAD_WATCH_BOUNDARY_MARKERS = (
    "pub(super) enum ReloadWatchBackend",
    "RecommendedWatcher::new",
    "RecursiveMode::Recursive",
    "async fn watch_web_package_for_reload",
    "fn should_reload_for_event",
)
FORBIDDEN_IMPLEMENTATION_PATTERNS = (
    re.compile(r"\bTcpStream\b"),
    re.compile(r"\bstd::io::Read\b"),
    re.compile(r"\bstd::io::Write\b"),
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
    - `crates/terlan_cli/Cargo.toml`.

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
    """Return missing Hyper/Tokio serve marker findings.

    Inputs:
    - Main serve implementation source.

    Outputs:
    - Finding records for missing required implementation markers.

    Transformation:
    - Checks for explicit marker strings that encode the current approved
      Hyper/Tokio request path.
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
    return findings


def serve_source_files() -> list[Path]:
    """Return serve implementation Rust files.

    Inputs:
    - `crates/terlan_cli/src/commands/serve`.

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
    """Return manual TCP/HTTP implementation findings.

    Inputs:
    - Non-test serve implementation Rust files.

    Outputs:
    - Finding records for obvious manual stream/request parsing paths.

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
                            message="manual HTTP/TCP implementation marker is forbidden in serve runtime",
                        )
                    )
    return findings


def safenative_http_boundary_findings() -> list[Finding]:
    """Return SafeNative HTTP boundary findings.

    Inputs:
    - `crates/terlan_safenative/src/http.rs`.

    Outputs:
    - Finding records when the temporary MIME boundary is not centralized in
      SafeNative HTTP.

    Transformation:
    - Requires the single adapter-owned `content_type_for_path` helper and its
      replacement note so manual MIME lookup does not creep back into `terlc
      serve` while the release waits for a maintained `mime_guess` dependency.
    """

    text = read_text(SAFENATIVE_HTTP)
    findings: list[Finding] = []
    for marker in REQUIRED_SAFENATIVE_HTTP_MARKERS:
        if marker not in text:
            findings.append(
                Finding(
                    path=relative(SAFENATIVE_HTTP),
                    line=None,
                    message=f"missing SafeNative HTTP boundary marker `{marker}`",
                )
            )
    return findings


def cookie_boundary_findings() -> list[Finding]:
    """Return cookie parsing boundary findings.

    Inputs:
    - `crates/terlan_cli/src/commands/serve/handler/beam_eval.rs`.
    - `crates/terlan_safenative/src/http/cookies.rs`.

    Outputs:
    - Finding records when request-cookie parsing is not routed through the
      SafeNative HTTP boundary.

    Transformation:
    - Requires the BEAM handler bridge to call the SafeNative cookie parser and
      requires the SafeNative parser to retain its maintained-crate replacement
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
    - `crates/terlan_cli/src/commands/serve/watch.rs`.
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
        + safenative_http_boundary_findings()
        + cookie_boundary_findings()
        + reload_watch_boundary_findings()
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
