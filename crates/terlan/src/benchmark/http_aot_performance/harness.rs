//! Loopback server and request orchestration for the HTTP AOT benchmark.

use std::env;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::HttpTiming;

impl HttpTiming {
    /// Summarizes non-empty request durations against one wall-clock interval.
    pub(super) fn from_durations(durations: &[Duration], wall: Duration) -> Result<Self, String> {
        if durations.is_empty() {
            return Err("HTTP benchmark timing requires at least one sample".to_string());
        }
        let mut values = durations.iter().map(Duration::as_nanos).collect::<Vec<_>>();
        values.sort_unstable();
        let total = values.iter().sum::<u128>();
        let wall_ns = wall.as_nanos().max(1);
        Ok(Self {
            sample_count: values.len(),
            total_wall_ns: wall_ns,
            throughput_requests_per_second: values.len() as u128 * 1_000_000_000 / wall_ns,
            min_ns: values[0],
            mean_ns: total / values.len() as u128,
            p50_ns: percentile(&values, 50),
            p95_ns: percentile(&values, 95),
            p99_ns: percentile(&values, 99),
            max_ns: values[values.len() - 1],
        })
    }
}

/// Measures individual request latency and aggregate wall-clock throughput.
pub(super) fn measure_requests(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let measurement = super::http_client::measure(
        port,
        concurrency,
        requests_per_worker,
        payload_bytes,
        "generation-one",
    )?;
    HttpTiming::from_durations(&measurement.durations, measurement.wall)
}

/// Measures close-after-response traffic for a stable wall-clock duration.
pub(super) fn measure_requests_for_duration(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let measurement = super::http_client::measure_for_duration(
        port,
        concurrency,
        duration,
        payload_bytes,
        "generation-one",
    )?;
    HttpTiming::from_durations(&measurement.durations, measurement.wall)
}

/// Measures persistent traffic for a stable wall-clock duration.
pub(super) fn measure_keep_alive_for_duration(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let measurement = super::http_client::measure_keep_alive_for_duration(
        port,
        concurrency,
        duration,
        payload_bytes,
        "generation-one",
    )?;
    HttpTiming::from_durations(&measurement.durations, measurement.wall)
}

/// Runs independent rounds and selects the median-throughput round intact.
pub(super) fn measure_request_rounds(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
    rounds: usize,
    duration_ms: u64,
) -> Result<(HttpTiming, Vec<HttpTiming>), String> {
    let mut timings = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        timings.push(if duration_ms == 0 {
            measure_requests(port, concurrency, requests_per_worker, payload_bytes)?
        } else {
            measure_requests_for_duration(
                port,
                concurrency,
                Duration::from_millis(duration_ms),
                payload_bytes,
            )?
        });
    }
    Ok((median_throughput_round(&timings)?.clone(), timings))
}

/// Selects one coherent observed round instead of synthesizing percentiles.
pub(super) fn median_throughput_round(rounds: &[HttpTiming]) -> Result<&HttpTiming, String> {
    if rounds.is_empty() {
        return Err("HTTP benchmark requires at least one measurement round".to_string());
    }
    let mut ordered = rounds.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|timing| timing.throughput_requests_per_second);
    Ok(ordered[ordered.len() / 2])
}

fn percentile(sorted: &[u128], requested: usize) -> u128 {
    let index = ((sorted.len() - 1) * requested).div_ceil(100);
    sorted[index]
}

/// Measures persistent HTTP/1.1 requests without connection setup per sample.
pub(super) fn measure_keep_alive_requests(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<HttpTiming, String> {
    let measurement = super::http_client::measure_keep_alive(
        port,
        concurrency,
        requests_per_worker,
        payload_bytes,
        "generation-one",
    )?;
    HttpTiming::from_durations(&measurement.durations, measurement.wall)
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
        match super::http_client::request(port, payload_bytes, generation) {
            Ok(()) => return Ok(()),
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
        "[package]\nname = \"http_aot_performance\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
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
  "handlers": [
    {
      "method": "POST",
      "route": "/api/bench",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {"path": "src/app/Api.terl", "line": 9, "column": 5}
    },
    {
      "method": "POST",
      "route": "/api/json",
      "module": "app.Api",
      "function": "json",
      "arity": 1,
      "source": {"path": "src/app/Api.terl", "line": 12, "column": 5}
    },
    {
      "method": "POST",
      "route": "/api/metadata",
      "module": "app.Api",
      "function": "metadata",
      "arity": 1,
      "source": {"path": "src/app/Api.terl", "line": 19, "column": 5}
    },
    {
      "method": "GET",
      "route": "/api/static",
      "module": "app.Api",
      "function": "static_response",
      "arity": 1,
      "source": {"path": "src/app/Api.terl", "line": 22, "column": 5}
    },
    {
      "method": "GET",
      "route": "/api/add/{left:Int}/{right:Int}",
      "module": "app.Api",
      "function": "add",
      "arity": 3,
      "source": {"path": "src/app/Api.terl", "line": 25, "column": 5}
    },
    {
      "method": "POST",
      "route": "/api/items",
      "module": "app.Api",
      "function": "create_item",
      "arity": 1,
      "source": {"path": "src/app/Api.terl", "line": 31, "column": 5}
    },
    {
      "method": "GET",
      "route": "/api/items/:id",
      "module": "app.Api",
      "function": "read_item",
      "arity": 2,
      "source": {"path": "src/app/Api.terl", "line": 34, "column": 5}
    },
    {
      "method": "PUT",
      "route": "/api/items/:id",
      "module": "app.Api",
      "function": "update_item",
      "arity": 2,
      "source": {"path": "src/app/Api.terl", "line": 37, "column": 5}
    },
    {
      "method": "DELETE",
      "route": "/api/items/:id",
      "module": "app.Api",
      "function": "delete_item",
      "arity": 2,
      "source": {"path": "src/app/Api.terl", "line": 40, "column": 5}
    }
  ],
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
            "module app.Api.\n\nimport std.core.Int.\nimport std.core.Option.\nimport std.core.Result.{{Err, Ok}}.\nimport std.http.Response.\nimport type std.http.Request.{{Request}}.\nimport type std.http.Response.{{Response}}.\n\npub handle(request: Request): Response ->\n    Response.text(\"{generation}:\" + request.body_text()).\n\npub json(request: Request): Response ->\n    case request.body_json() {{\n        Ok(value) -> Response.json(value);\n        Err(_error) -> Response.text(\"invalid-json\").with_status(400)\n    }}.\n\npub metadata(request: Request): Response ->\n    Response.text(request.method() + \":\" + request.query_string() + \":\" + Option.with_default(request.header(\"accept\"), \"missing\") + \":\" + Option.with_default(request.header(\"cookie\"), \"missing\") + \":\" + request.body_text()).\n\npub static_response(_request: Request): Response ->\n    Response.text(\"static-benchmark-response\").\n\npub add(_request: Request, left: Int, right: Int): Response ->\n    Response.text(Int.to_string(left + right)).\n\npub create_item(request: Request): Response ->\n    Response.text(request.body_text()).with_status(201).\n\npub read_item(_request: Request, id: String): Response ->\n    Response.text(\"item-\" + id).\n\npub update_item(request: Request, id: String): Response ->\n    Response.text(id + \":\" + request.body_text()).\n\npub delete_item(_request: Request, _id: String): Response ->\n    Response.text(\"\").with_status(204).\n"
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
    let affinity = env::var("TERLAN_BENCH_HTTP_CPU_LIST")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let mut command = if let Some(affinity) = affinity {
        let mut command = Command::new("taskset");
        command.args(["--cpu-list", affinity.as_str()]);
        command.arg(compiler);
        command
    } else {
        Command::new(compiler)
    };
    command
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
        .stderr(Stdio::inherit())
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
    super::http_client::resident_bytes(pid)
}

/// Captures attributed process memory and thread state from Linux procfs.
pub(super) fn process_memory_snapshot(
    pid: u32,
    phase: &str,
) -> Option<super::HttpProcessMemorySnapshot> {
    super::http_client::process_memory_snapshot(pid, phase)
}
