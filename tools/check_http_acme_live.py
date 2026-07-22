#!/usr/bin/env python3
"""Run the manually enabled live ACME smoke path for `terlc serve`.

Inputs:
- `TERLAN_HTTP_ACME_LIVE=1` to opt into the live smoke.
- `TERLAN_HTTP_ACME_DOMAIN`: real DNS name that points at this machine.
- `TERLAN_HTTP_ACME_EMAIL`: ACME account contact email.
- Optional `TERLAN_HTTP_ACME_HOST`, `TERLAN_HTTP_ACME_PORT`,
  `TERLAN_HTTP_ACME_TIMEOUT_SECONDS`, and `TERLAN_HTTP_ACME_TERLC`.

Outputs:
- Exit status 0 when the check is skipped or when live startup populates the
  ACME certificate cache.
- Exit status 1 with stable diagnostics when opt-in configuration is invalid,
  the compiler is missing, `terlc serve` exits, or the cache is not populated
  before the timeout.

Transformation:
- Creates a minimal production-shaped web package with `[server.tls] mode =
  "auto"`, starts `terlc serve`, and watches the project-local
  `.terlan/tls/acme` cache. The check never contacts public ACME unless the
  explicit opt-in environment variable is set.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TERLC = ROOT / "target" / "debug" / "terlc"


def env_value(name: str) -> str | None:
    """Return a trimmed environment value.

    Inputs:
    - `name`: environment variable name.

    Outputs:
    - Trimmed non-empty string, or `None`.

    Transformation:
    - Treats whitespace-only values as absent for stable validation.
    """

    value = os.environ.get(name)
    if value is None:
        return None
    value = value.strip()
    return value or None


def required_env(name: str) -> str:
    """Return a required live-smoke environment value.

    Inputs:
    - `name`: required environment variable name.

    Outputs:
    - Trimmed environment value.

    Transformation:
    - Emits one stable diagnostic and exits when the value is absent.
    """

    value = env_value(name)
    if value is None:
        print(f"http-acme-live-check: {name} is required when TERLAN_HTTP_ACME_LIVE=1")
        raise SystemExit(1)
    return value


def parse_port(value: str) -> str:
    """Validate and return a TCP port value.

    Inputs:
    - `value`: raw environment value.

    Outputs:
    - Original port string when it is in the valid TCP range.

    Transformation:
    - Keeps command rendering stable while rejecting malformed port settings
      before spawning the compiler.
    """

    try:
        port = int(value, 10)
    except ValueError:
        print(f"http-acme-live-check: TERLAN_HTTP_ACME_PORT must be an integer, got `{value}`")
        raise SystemExit(1)
    if port < 1 or port > 65535:
        print(f"http-acme-live-check: TERLAN_HTTP_ACME_PORT out of range: {port}")
        raise SystemExit(1)
    return str(port)


def parse_timeout(value: str) -> float:
    """Validate and return a timeout in seconds.

    Inputs:
    - `value`: raw timeout value.

    Outputs:
    - Positive timeout as a float.

    Transformation:
    - Rejects non-positive or malformed values before process startup.
    """

    try:
        timeout = float(value)
    except ValueError:
        print(
            "http-acme-live-check: TERLAN_HTTP_ACME_TIMEOUT_SECONDS "
            f"must be numeric, got `{value}`"
        )
        raise SystemExit(1)
    if timeout <= 0:
        print("http-acme-live-check: TERLAN_HTTP_ACME_TIMEOUT_SECONDS must be positive")
        raise SystemExit(1)
    return timeout


def write_package(project_root: Path, domain: str, email: str) -> Path:
    """Write a minimal auto-TLS web package.

    Inputs:
    - `project_root`: temporary project directory.
    - `domain`: ACME DNS identifier.
    - `email`: ACME account email.

    Outputs:
    - `_build/web` package path.

    Transformation:
    - Produces only the browser package and adjacent project metadata needed
      by `terlc serve`.
    """

    web_root = project_root / "_build" / "web"
    web_root.mkdir(parents=True, exist_ok=True)
    (web_root / "index.html").write_text("<!doctype html><main>Terlan ACME smoke</main>\n")
    (web_root / "manifest.json").write_text(
        """{
  "schema": "terlan-web-build-v1",
  "build_id": "http-acme-live-check",
  "index": "index.html",
  "assets": []
}
"""
    )
    (project_root / "terlan.toml").write_text(
        f"""[package]
name = "http_acme_live_check"
version = "0.0.0"

[server.tls]
mode = "auto"
domains = ["{domain}"]
email = "{email}"
"""
    )
    return web_root


def terminate(process: subprocess.Popen[str]) -> None:
    """Terminate a spawned smoke process.

    Inputs:
    - `process`: running `terlc serve` process.

    Outputs:
    - None.

    Transformation:
    - Tries graceful termination first, then kills if the process ignores it.
    """

    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def main() -> int:
    """Run the live ACME smoke contract.

    Inputs:
    - Process environment.

    Outputs:
    - Process exit code.

    Transformation:
    - Skips by default, otherwise starts a real serve process and waits for
      ACME cache material.
    """

    if env_value("TERLAN_HTTP_ACME_LIVE") != "1":
        print("http-acme-live-check: skipped; set TERLAN_HTTP_ACME_LIVE=1 to run")
        return 0

    domain = required_env("TERLAN_HTTP_ACME_DOMAIN")
    email = required_env("TERLAN_HTTP_ACME_EMAIL")
    host = env_value("TERLAN_HTTP_ACME_HOST") or "0.0.0.0"
    port = parse_port(env_value("TERLAN_HTTP_ACME_PORT") or "443")
    timeout = parse_timeout(env_value("TERLAN_HTTP_ACME_TIMEOUT_SECONDS") or "60")
    terlc = Path(env_value("TERLAN_HTTP_ACME_TERLC") or str(DEFAULT_TERLC))
    if not terlc.is_file():
        print(f"http-acme-live-check: terlc binary not found at `{terlc}`")
        print("http-acme-live-check: build it first with `cargo build --bin terlc --bin terlan-vm`")
        return 1

    with tempfile.TemporaryDirectory(prefix="terlan-http-acme-live.") as tmp:
        project_root = Path(tmp)
        web_root = write_package(project_root, domain, email)
        cert_path = project_root / ".terlan" / "tls" / "acme" / "fullchain.pem"
        key_path = project_root / ".terlan" / "tls" / "acme" / "privkey.pem"
        command = [str(terlc), "serve", str(web_root), "--host", host, "--port", port]
        process = subprocess.Popen(
            command,
            cwd=project_root,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        deadline = time.monotonic() + timeout
        try:
            while time.monotonic() < deadline:
                status = process.poll()
                if status is not None:
                    stdout, stderr = process.communicate()
                    print(f"http-acme-live-check: terlc serve exited with {status}")
                    if stdout:
                        print(stdout, end="")
                    if stderr:
                        print(stderr, end="", file=sys.stderr)
                    return 1
                if cert_path.is_file() and key_path.is_file():
                    print(
                        "http-acme-live-check: ACME cache populated for "
                        f"`{domain}` at `{cert_path.parent}`"
                    )
                    return 0
                time.sleep(0.25)
            print(
                "http-acme-live-check: timed out waiting for ACME cache "
                f"for `{domain}` after {timeout:g}s"
            )
            return 1
        finally:
            terminate(process)


if __name__ == "__main__":
    raise SystemExit(main())
