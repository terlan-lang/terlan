//! Loopback server and request orchestration for the HTTP AOT benchmark.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::HttpTiming;

/// Measures individual request latency and aggregate wall-clock throughput.
pub(super) fn measure_requests(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let started = Instant::now();
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        workers.push(thread::spawn(move || {
            let mut durations = Vec::with_capacity(requests_per_worker);
            for _ in 0..requests_per_worker {
                let request_started = Instant::now();
                let body = request(port, payload_bytes)?;
                if !body.starts_with("generation-") {
                    return Err(format!("unexpected benchmark response body `{body}`"));
                }
                durations.push(request_started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    let mut durations = Vec::with_capacity(concurrency * requests_per_worker);
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP benchmark worker panicked".to_string())??,
        );
    }
    HttpTiming::from_durations(&durations, started.elapsed())
}

/// Sends one complete loopback request and returns its validated response body.
fn request(port: u16, payload_bytes: usize) -> Result<String, String> {
    let payload = "x".repeat(payload_bytes);
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("HTTP benchmark connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP benchmark read timeout setup failed: {error}"))?;
    write!(
        stream,
        "POST /api/bench HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    )
    .map_err(|error| format!("HTTP benchmark request write failed: {error}"))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("HTTP benchmark response read failed: {error}"))?;
    let response = String::from_utf8(bytes)
        .map_err(|error| format!("HTTP benchmark response was not UTF-8: {error}"))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP benchmark response lacked a header terminator".to_string())?;
    if !head.lines().next().unwrap_or_default().contains(" 200 ") {
        return Err(format!("HTTP benchmark returned non-200 response `{head}`"));
    }
    Ok(body.to_string())
}

/// Waits for one named source generation to become visible through HTTP.
pub(super) fn wait_for_generation(
    port: u16,
    generation: &str,
    payload_bytes: usize,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    while Instant::now() < deadline {
        match request(port, payload_bytes) {
            Ok(body) if body == format!("{generation}:{}", "x".repeat(payload_bytes)) => {
                return Ok(())
            }
            Ok(body) => last = body,
            Err(error) => last = error,
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(format!(
        "HTTP benchmark generation `{generation}` did not become ready; last result `{last}`"
    ))
}

/// Creates an isolated benchmark package and returns its web root.
pub(super) fn write_package(
    workspace: &Path,
    generation: &str,
    _payload_bytes: usize,
) -> Result<PathBuf, String> {
    let web_root = workspace.join("_build/web");
    fs::create_dir_all(web_root.join("assets/js/modules"))
        .map_err(|error| format!("failed to create HTTP benchmark web root: {error}"))?;
    fs::create_dir_all(workspace.join("src/app"))
        .map_err(|error| format!("failed to create HTTP benchmark source root: {error}"))?;
    fs::write(
        workspace.join("terlan.toml"),
        "[package]\nname = \"http_aot_performance\"\nversion = \"0.0.7\"\n",
    )
    .map_err(|error| format!("failed to write HTTP benchmark manifest: {error}"))?;
    fs::write(web_root.join("index.html"), "<!doctype html>\n")
        .map_err(|error| format!("failed to write HTTP benchmark index: {error}"))?;
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const benchmark = true;\n",
    )
    .map_err(|error| format!("failed to write HTTP benchmark asset: {error}"))?;
    write_handler_source(workspace, generation)?;
    fs::write(
        web_root.join("manifest.json"),
        r#"{
  "schema": "terlan-web-build-v1",
  "target_profile": "js.browser",
  "source_js_manifest": "../js/manifest.json",
  "index": "index.html",
  "handlers": [{
    "method": "POST",
    "route": "/api/bench",
    "module": "app.Api",
    "function": "handle",
    "arity": 1,
    "source": {"path": "src/app/Api.terl", "line": 7, "column": 5}
  }],
  "assets": [{
    "module": "app",
    "kind": "javascript-module",
    "source_relative_path": "modules/app.js",
    "web_relative_path": "assets/js/modules/app.js",
    "fingerprint": 1
  }]
}
"#,
    )
    .map_err(|error| format!("failed to write HTTP benchmark web manifest: {error}"))?;
    Ok(web_root)
}

/// Writes the source handler for one distinguishable generation.
pub(super) fn write_handler_source(workspace: &Path, generation: &str) -> Result<(), String> {
    fs::write(
        workspace.join("src/app/Api.terl"),
        format!(
            "module app.Api.\n\nimport std.http.Response.\nimport type std.http.Request.{{Request}}.\nimport type std.http.Response.{{Response}}.\n\npub handle(request: Request): Response ->\n    Response.text(\"{generation}:\" + request.body_text()).\n"
        ),
    )
    .map_err(|error| format!("failed to write HTTP benchmark handler source: {error}"))
}

/// Spawns `terlc serve` with fast source generation polling.
pub(super) fn spawn_server(
    compiler: &Path,
    web_root: &Path,
    port: u16,
    readiness_reactors: usize,
) -> Result<Child, String> {
    let web_root = web_root.to_string_lossy().to_string();
    let port = port.to_string();
    Command::new(compiler)
        .env("TERLAN_VM_SCHEDULERS", readiness_reactors.to_string())
        .args([
            "serve",
            &web_root,
            "--host",
            "127.0.0.1",
            "--port",
            &port,
            "--poll-ms",
            "25",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to start HTTP benchmark server: {error}"))
}

/// Drop guard that always terminates and reaps a benchmark server.
pub(super) struct ServerGuard {
    child: Child,
}

impl ServerGuard {
    pub(super) fn new(child: Child) -> Self {
        Self { child }
    }

    pub(super) fn id(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reserves a currently unused loopback port.
pub(super) fn reserve_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to reserve HTTP benchmark port: {error}"))?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| format!("failed to inspect HTTP benchmark port: {error}"))
}

/// Creates a unique temporary benchmark workspace.
pub(super) fn create_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-http-aot-performance-{}-{}",
        std::process::id(),
        super::unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create HTTP benchmark workspace: {error}"))?;
    Ok(path)
}

/// Reads resident-set bytes for a child process through procfs or `ps`.
pub(super) fn resident_bytes(pid: u32) -> Option<u64> {
    let procfs_kilobytes = fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        });
    let kilobytes = procfs_kilobytes.or_else(|| {
        Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
    })?;
    kilobytes.checked_mul(1024)
}
