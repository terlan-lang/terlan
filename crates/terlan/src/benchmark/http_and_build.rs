use super::*;

/// Writes a source-backed browser package fixture for VM HTTP benchmarking.
pub(super) fn write_vm_http_benchmark_package(workspace: &Path) -> Result<PathBuf, String> {
    let web_root = workspace.join("_build/web");
    let source_dir = workspace.join("src/app");
    fs::create_dir_all(web_root.join("assets/js/modules"))
        .map_err(|error| format!("failed to create VM HTTP web package: {error}"))?;
    fs::create_dir_all(&source_dir)
        .map_err(|error| format!("failed to create VM HTTP source directory: {error}"))?;
    fs::write(
        workspace.join("terlan.toml"),
        "[package]\nname = \"vm_http_bench\"\nversion = \"0.0.7\"\nnamespace = \"app\"\n",
    )
    .map_err(|error| format!("failed to write VM HTTP terlan.toml: {error}"))?;
    fs::write(web_root.join("index.html"), "<!doctype html>\n")
        .map_err(|error| format!("failed to write VM HTTP index: {error}"))?;
    fs::write(
        web_root.join("assets/js/modules/app.js"),
        "export const value = 1;\n",
    )
    .map_err(|error| format!("failed to write VM HTTP JS asset: {error}"))?;
    fs::write(
        source_dir.join("Api.terl"),
        "module app.Api.\n\nimport std.http.Response.\nimport std.core.Option.\nimport type std.http.Request.{Request}.\nimport type std.http.Response.{Response}.\n\npub handle(request: Request): Response ->\n    Response.text(request.method() + \":\" + Option.with_default(request.query(\"page\"), \"missing\") + \":\" + Option.with_default(request.header(\"accept\"), \"missing\") + \":\" + Option.with_default(request.cookie(\"session\"), \"missing\") + \":\" + request.body_text()).with_status(203).\n",
    )
    .map_err(|error| format!("failed to write VM HTTP handler source: {error}"))?;
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
      "route": "/api/users",
      "module": "app.Api",
      "function": "handle",
      "arity": 1,
      "source": {
        "path": "src/app/Api.terl",
        "line": 7,
        "column": 5
      }
    }
  ],
  "assets": [
    {
      "module": "app",
      "kind": "javascript-module",
      "source_relative_path": "modules/app.js",
      "web_relative_path": "assets/js/modules/app.js",
      "fingerprint": 1
    }
  ]
}
"#,
    )
    .map_err(|error| format!("failed to write VM HTTP package manifest: {error}"))?;
    Ok(web_root)
}

/// Reserves a currently free localhost TCP port.
pub(super) fn reserve_localhost_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to reserve VM HTTP benchmark port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to read VM HTTP benchmark port: {error}"))?
        .port();
    drop(listener);
    Ok(port)
}

/// Waits until the spawned VM HTTP server can answer benchmark requests.
pub(super) fn wait_for_vm_http_server(port: u16) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_error = "server did not answer".to_string();
    while Instant::now() < deadline {
        match vm_http_round_trip(port) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!(
        "VM HTTP benchmark server did not become ready: {last_error}"
    ))
}

/// Executes one real HTTP request through the VM-backed handler server.
pub(super) fn vm_http_round_trip(port: u16) -> Result<(), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("VM HTTP benchmark connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| format!("failed to set VM HTTP benchmark read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|error| format!("failed to set VM HTTP benchmark write timeout: {error}"))?;
    stream
        .write_all(
            b"POST /api/users?page=2 HTTP/1.1\r\n\
Host: 127.0.0.1\r\n\
Accept: application/json\r\n\
Cookie: session=abc\r\n\
Content-Length: 7\r\n\
Connection: close\r\n\
\r\n\
payload",
        )
        .map_err(|error| format!("VM HTTP benchmark request write failed: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("VM HTTP benchmark response read failed: {error}"))?;
    let response = String::from_utf8_lossy(&response);
    let Some((head, body)) = response.split_once("\r\n\r\n") else {
        return Err(format!("malformed VM HTTP response `{response}`"));
    };
    let status_line = head.lines().next().unwrap_or_default();
    if !status_line.contains(" 203 ") {
        return Err(format!(
            "unexpected VM HTTP response status `{status_line}`: {body}"
        ));
    }
    if body == "POST:2:application/json:abc:payload" {
        Ok(())
    } else {
        Err(format!("unexpected VM HTTP response body `{body}`"))
    }
}

/// Kills a spawned benchmark server when the benchmark exits.
pub(super) struct ChildProcessGuard {
    pub(super) child: Child,
}

impl ChildProcessGuard {
    /// Stores a child process for drop-time cleanup.
    pub(super) fn new(child: Child) -> Self {
        Self { child }
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs the HTTP baseline benchmark.
///
/// Inputs:
/// - `options`: output and iteration configuration.
///
/// Output:
/// - Completed HTTP benchmark report.
///
/// Transformation:
/// - Measures static response conversion, JSON response conversion, and
///   handler-style request dispatch through the current native HTTP APIs.
pub(super) fn run_http_baseline(
    options: &HttpBenchmarkOptions,
) -> Result<HttpBenchmarkReport, String> {
    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    measurements.push(measure_repeated(
        "http_static_response_to_http",
        options.iterations,
        measure_static_http_response,
    )?);
    assertions.push(AssertionResult {
        name: "http_static_response_to_http",
        passed: true,
        detail: "all static response conversions returned 200 and stable body".to_string(),
    });

    measurements.push(measure_repeated(
        "http_json_response_serialize_to_http",
        options.iterations,
        measure_json_http_response,
    )?);
    assertions.push(AssertionResult {
        name: "http_json_response_serialize_to_http",
        passed: true,
        detail: "all JSON response conversions serialized expected payload".to_string(),
    });

    let request = http::Request::from_parts_with_raw_query_metadata(
        "GET",
        "/api/counter/42",
        "",
        http::RequestMetadata {
            params: vec![("id".to_string(), "42".to_string())],
            query_string: "format=json".to_string(),
            query: vec![("format".to_string(), "json".to_string())],
            headers: vec![("accept".to_string(), "application/json".to_string())],
            cookies: Vec::new(),
        },
    );
    measurements.push(measure_repeated(
        "http_handler_dispatch_to_response",
        options.iterations,
        || {
            let response = benchmark_http_handler(&request)?;
            let converted = response.to_http_response().map_err(format_http_error)?;
            if converted.status() != ::http::StatusCode::OK {
                return Err(format!("unexpected handler status {}", converted.status()));
            }
            if !converted.body().contains("\"id\":\"42\"") {
                return Err("unexpected handler response body".to_string());
            }
            Ok(())
        },
    )?);
    assertions.push(AssertionResult {
        name: "http_handler_dispatch_to_response",
        passed: true,
        detail: "all handler dispatch calls read request metadata and returned JSON".to_string(),
    });

    for concurrency in HTTP_CONCURRENCY_LEVELS {
        measurements.push(measure_concurrent_http_track(
            "http_static_response_to_http",
            *concurrency,
            options.concurrent_iterations,
            measure_static_http_response,
        )?);
        measurements.push(measure_concurrent_http_track(
            "http_json_response_serialize_to_http",
            *concurrency,
            options.concurrent_iterations,
            measure_json_http_response,
        )?);
        let handler_request = request.clone();
        measurements.push(measure_concurrent_http_track(
            "http_handler_dispatch_to_response",
            *concurrency,
            options.concurrent_iterations,
            move || {
                let response = benchmark_http_handler(&handler_request)?;
                let converted = response.to_http_response().map_err(format_http_error)?;
                if converted.status() != ::http::StatusCode::OK {
                    return Err(format!("unexpected handler status {}", converted.status()));
                }
                if !converted.body().contains("\"id\":\"42\"") {
                    return Err("unexpected handler response body".to_string());
                }
                Ok(())
            },
        )?);
    }
    assertions.push(AssertionResult {
        name: "http_concurrent_tracks",
        passed: true,
        detail: "static, JSON, and handler tracks completed at 100 and 1000 simulated users"
            .to_string(),
    });

    Ok(HttpBenchmarkReport::completed(
        options,
        measurements,
        assertions,
    ))
}

/// Measures one static HTTP response conversion.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when conversion preserves status and body.
///
/// Transformation:
/// - Exercises the native HTTP response constructor plus Rust `http` crate
///   conversion path.
pub(super) fn measure_static_http_response() -> Result<(), String> {
    let response = http::text("Hello from Terlan", 200);
    let converted = response.to_http_response().map_err(format_http_error)?;
    if converted.status() != ::http::StatusCode::OK {
        return Err(format!("unexpected static status {}", converted.status()));
    }
    if converted.body() != "Hello from Terlan" {
        return Err("unexpected static response body".to_string());
    }
    Ok(())
}

/// Measures one JSON HTTP response conversion.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when serialization and conversion preserve expected JSON.
///
/// Transformation:
/// - Exercises `std.data.Json` adapter construction, HTTP JSON response
///   construction, and Rust `http` crate conversion.
pub(super) fn measure_json_http_response() -> Result<(), String> {
    let mut payload = json::object();
    json::put(&mut payload, "status", json::string("ok")).map_err(format_json_error)?;
    json::put(&mut payload, "count", json::int(3)).map_err(format_json_error)?;
    let response = http::json(&payload, 200);
    let converted = response.to_http_response().map_err(format_http_error)?;
    if converted.status() != ::http::StatusCode::OK {
        return Err(format!("unexpected JSON status {}", converted.status()));
    }
    if !converted.body().contains("\"status\":\"ok\"") {
        return Err("unexpected JSON response body".to_string());
    }
    Ok(())
}

/// Measures a concurrent HTTP benchmark track.
///
/// Inputs:
/// - `track`: base track name.
/// - `concurrency`: number of simulated users.
/// - `iterations`: operations per user.
/// - `operation`: operation executed by every simulated user.
///
/// Output:
/// - Worker wall-time measurement summary.
///
/// Transformation:
/// - Spawns one thread per simulated user and records each worker's total
///   duration for the requested track.
pub(super) fn measure_concurrent_http_track<F>(
    track: &'static str,
    concurrency: usize,
    iterations: usize,
    operation: F,
) -> Result<Measurement, String>
where
    F: Fn() -> Result<(), String> + Send + Sync + 'static,
{
    let operation = Arc::new(operation);
    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let worker_operation = Arc::clone(&operation);
        handles.push(thread::spawn(move || {
            let start = Instant::now();
            for _ in 0..iterations {
                worker_operation()?;
            }
            Ok::<Duration, String>(start.elapsed())
        }));
    }
    let mut durations = Vec::with_capacity(concurrency);
    for handle in handles {
        let duration = handle
            .join()
            .map_err(|_| "HTTP benchmark worker panicked".to_string())??;
        durations.push(duration);
    }
    Ok(Measurement::from_durations(
        concurrent_measurement_name(track, concurrency),
        &durations,
    ))
}

/// Returns the measurement name for a concurrent HTTP track.
///
/// Inputs:
/// - `track`: base track name.
/// - `concurrency`: simulated user count.
///
/// Output:
/// - Stable measurement name.
///
/// Transformation:
/// - Uses the two required concurrency levels as static names so reports do
///   not allocate leaked strings.
pub(super) fn concurrent_measurement_name(track: &'static str, concurrency: usize) -> &'static str {
    match (track, concurrency) {
        ("http_static_response_to_http", 100) => {
            "http_static_response_to_http_100_users_worker_wall_time"
        }
        ("http_static_response_to_http", 1_000) => {
            "http_static_response_to_http_1000_users_worker_wall_time"
        }
        ("http_json_response_serialize_to_http", 100) => {
            "http_json_response_serialize_to_http_100_users_worker_wall_time"
        }
        ("http_json_response_serialize_to_http", 1_000) => {
            "http_json_response_serialize_to_http_1000_users_worker_wall_time"
        }
        ("http_handler_dispatch_to_response", 100) => {
            "http_handler_dispatch_to_response_100_users_worker_wall_time"
        }
        ("http_handler_dispatch_to_response", 1_000) => {
            "http_handler_dispatch_to_response_1000_users_worker_wall_time"
        }
        _ => "http_unknown_concurrent_track_worker_wall_time",
    }
}

/// Sample handler used by the HTTP baseline.
///
/// Inputs:
/// - `request`: native HTTP request snapshot.
///
/// Output:
/// - JSON response carrying route and query metadata.
///
/// Transformation:
/// - Mimics the work a Terlan handler currently performs through typed request
///   accessors and native response constructors without invoking VM.
pub(super) fn benchmark_http_handler(request: &http::Request) -> Result<http::Response, String> {
    let id = http::param(request, "id").ok_or_else(|| "missing id param".to_string())?;
    let format =
        http::query(request, "format").ok_or_else(|| "missing format query".to_string())?;
    let mut payload = json::object();
    json::put(&mut payload, "id", json::string(&id)).map_err(format_json_error)?;
    json::put(&mut payload, "format", json::string(&format)).map_err(format_json_error)?;
    json::put(&mut payload, "method", json::string(&http::method(request)))
        .map_err(format_json_error)?;
    Ok(http::json(&payload, 200))
}

/// Resolves a benchmark binary path.
///
/// Inputs:
/// - `binary`: Cargo binary name.
/// - `explicit`: optional caller-supplied path.
///
/// Output:
/// - Path to an executable local binary.
///
/// Transformation:
/// - Prefers explicit configuration, otherwise builds the requested binary
///   through Cargo before returning the sibling path.
pub(super) fn resolve_benchmark_binary(
    binary: &str,
    explicit: Option<&Path>,
) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return require_existing_binary(path);
    }
    let current = env::current_exe()
        .map_err(|error| format!("failed to resolve current benchmark binary: {error}"))?;
    let parent = current.parent().ok_or_else(|| {
        format!(
            "failed to resolve parent directory for benchmark binary `{}`",
            current.display()
        )
    })?;
    let sibling = parent.join(platform_binary_name(binary));
    build_cargo_binary(binary)?;
    require_existing_binary(&sibling)
}

/// Returns the platform-specific binary filename.
pub(super) fn platform_binary_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    }
}

/// Ensures a configured binary path exists.
pub(super) fn require_existing_binary(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        Ok(path.to_path_buf())
    } else {
        Err(format!(
            "benchmark binary `{}` does not exist",
            path.display()
        ))
    }
}

/// Builds one local Cargo binary needed by the benchmark.
pub(super) fn build_cargo_binary(binary: &str) -> Result<(), String> {
    let output = Command::new("cargo")
        .args(["build", "-p", "terlan", "--bin", binary, "--quiet"])
        .output()
        .map_err(|error| format!("failed to start cargo build for `{binary}`: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format_command_failure("cargo build", &output))
    }
}

/// Creates a unique temporary workspace for VM benchmark sources.
pub(super) fn create_vm_benchmark_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-vm-performance-baseline-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create VM benchmark workspace: {error}"))?;
    Ok(path)
}

/// Writes one Terlan benchmark source into the workspace.
pub(super) fn write_vm_benchmark_source(
    workspace: &Path,
    name: &str,
    source: &str,
) -> Result<PathBuf, String> {
    let path = workspace.join(name);
    fs::write(&path, source).map_err(|error| {
        format!(
            "failed to write VM benchmark source `{}`: {error}",
            path.display()
        )
    })?;
    Ok(path)
}

/// Builds a synthetic helper-heavy Terlan source file.
///
/// Inputs:
/// - `module`: source module name.
/// - `helper_count`: number of local helper functions.
///
/// Output:
/// - Terlan source that returns `Bool true` when all helper calls execute.
///
/// Transformation:
/// - Generates pure local calls to approximate larger source compile/run
///   workloads without depending on an external app checkout.
pub(super) fn synthetic_helper_source(module: &str, helper_count: usize) -> String {
    let mut source = format!("module {module}.\n\n");
    for index in 0..helper_count {
        source.push_str(&format!(
            "helper{index}(value: Int): Int ->\n    value + {index}.\n\n"
        ));
    }
    let block_size = 10;
    let block_count = helper_count.div_ceil(block_size);
    for block in 0..block_count {
        let start = block * block_size;
        let end = ((block + 1) * block_size).min(helper_count);
        let mut expression = if block == 0 {
            "1".to_string()
        } else {
            format!("block{}()", block - 1)
        };
        for index in start..end {
            expression = format!("helper{index}({expression})");
        }
        source.push_str(&format!("block{block}(): Int ->\n    {expression}.\n\n"));
    }
    let expected = 1 + helper_count.saturating_sub(1) * helper_count / 2;
    let final_call = if block_count == 0 {
        "1".to_string()
    } else {
        format!("block{}()", block_count - 1)
    };
    source.push_str(&format!(
        "pub main(): Bool ->\n    {final_call} == {expected}.\n"
    ));
    source
}

/// Runs a VM source file through `--test-eval`.
pub(super) fn run_vm_source_test(vm_binary: &Path, source: &Path) -> Result<(), String> {
    let source = source.to_string_lossy().to_string();
    run_required_command(
        vm_binary,
        &["run", &source, "--entry", "main", "--test-eval"],
    )?;
    Ok(())
}

/// Measures single-file VM artifact build output.
///
/// Inputs:
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build.
///
/// Output:
/// - Success when one non-empty native `.tvm` application image is emitted.
///
/// Transformation:
/// - Runs `terlc build --target terlan-vm` in an isolated output directory and
///   validates the produced artifact envelope without loading it.
pub(super) fn measure_vm_artifact_build_single_file(
    compiler_binary: &Path,
    source: &Path,
) -> Result<(), String> {
    build_single_file_vm_artifact(compiler_binary, source).map(|_| ())
}

/// Measures single-file VM artifact loading.
///
/// Inputs:
/// - `vm_binary`: resolved local `terlan-vm` binary.
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build before loading.
///
/// Output:
/// - Success when `terlan-vm load` admits the compiler-emitted native image.
///
/// Transformation:
/// - Builds a fresh artifact, validates its envelope, and then routes it
///   through the standalone VM loader command.
pub(super) fn measure_vm_artifact_load_single_file(
    vm_binary: &Path,
    compiler_binary: &Path,
    source: &Path,
) -> Result<(), String> {
    let artifact = build_single_file_vm_artifact(compiler_binary, source)?;
    let artifact_arg = artifact.to_string_lossy().to_string();
    let output = run_required_command(vm_binary, &["load", &artifact_arg])?;
    require_stdout_contains(
        "vm_artifact_load_single_file",
        &output,
        "loaded native TVM image",
    )
}

/// Builds and validates one single-file VM artifact.
///
/// Inputs:
/// - `compiler_binary`: resolved local `terlc` binary.
/// - `source`: Terlan source file to build.
///
/// Output:
/// - Path to the emitted native `.tvm` application image.
///
/// Transformation:
/// - Runs `terlc build --target terlan-vm` in an isolated output directory and
///   validates the native image's expected path and non-empty publication.
pub(super) fn build_single_file_vm_artifact(
    compiler_binary: &Path,
    source: &Path,
) -> Result<PathBuf, String> {
    let out_dir = env::temp_dir().join(format!(
        "terlan-vm-artifact-build-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&out_dir)
        .map_err(|error| format!("failed to create VM artifact build output: {error}"))?;
    let out_dir_arg = out_dir.to_string_lossy().to_string();
    let source_arg = source.to_string_lossy().to_string();
    run_required_command(
        compiler_binary,
        &[
            "--out-dir",
            &out_dir_arg,
            "build",
            &source_arg,
            "--target",
            "terlan-vm",
        ],
    )?;

    let vm_dir = out_dir.join("vm");
    let artifacts = fs::read_dir(&vm_dir)
        .map_err(|error| format!("failed to read VM artifact directory: {error}"))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("failed to read VM artifact entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let artifacts = artifacts
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".tvm"))
        })
        .collect::<Vec<_>>();
    let [artifact_path] = artifacts.as_slice() else {
        return Err(format!(
            "expected one VM artifact under `{}`, found {}",
            vm_dir.display(),
            artifacts.len()
        ));
    };
    let metadata = fs::metadata(artifact_path).map_err(|error| {
        format!(
            "failed to inspect native TVM image `{}`: {error}",
            artifact_path.display()
        )
    })?;
    if metadata.len() == 0 {
        return Err(format!(
            "native TVM image `{}` is empty",
            artifact_path.display()
        ));
    }
    Ok(artifact_path.to_path_buf())
}

/// Measures VM-owned process-table primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when process spawn, ordered message delivery, selective receive,
///   mailbox accounting, and exit cleanup behave correctly.
///
/// Transformation:
/// - Exercises local process identity and mailbox semantics without routing
///   through actor convenience APIs or source-level process syntax.
pub(super) fn measure_vm_process_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let parent = processes.spawn_root(VmProcessSource::new("bench.Process", "parent", 0));
    let child = processes.spawn_child(parent, VmProcessSource::new("bench.Process", "child", 0))?;
    let first_id = processes.send(parent, child, VmPrimitiveValue::Atom("first".to_string()))?;
    let second_id = processes.send(parent, child, VmPrimitiveValue::Atom("second".to_string()))?;
    if second_id <= first_id {
        return Err(format!(
            "message ids were not monotonic: first={first_id}, second={second_id}"
        ));
    }

    processes.with_process_control_mutator(child, |child_process| -> Result<(), String> {
        let selected = child_process
            .selective_receive(|message| {
                message.payload == VmPrimitiveValue::Atom("second".to_string())
            })
            .ok_or_else(|| "selective receive did not find second message".to_string())?;
        if selected.id != second_id {
            return Err(format!(
                "selective receive returned message {}, expected {second_id}",
                selected.id
            ));
        }
        if child_process.mailbox_len() != 1 {
            return Err(format!(
                "skipped mailbox length was {}, expected 1",
                child_process.mailbox_len()
            ));
        }
        let remaining = child_process
            .receive_next()
            .ok_or_else(|| "remaining message was not preserved".to_string())?;
        if remaining.id != first_id {
            return Err(format!(
                "remaining message id was {}, expected {first_id}",
                remaining.id
            ));
        }
        child_process.add_resource_handle("native:process-benchmark");
        Ok(())
    })??;
    let cleanup = processes.exit_process(child, process::VmExitReason::Normal)?;
    if cleanup == ["native:process-benchmark".to_string()] {
        Ok(())
    } else {
        Err(format!("unexpected process cleanup handles: {cleanup:?}"))
    }
}

/// Measures VM-owned process inspection directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when immutable process inspection exposes source identity,
///   process relation, state, mailbox depth, reductions, and resources.
///
/// Transformation:
/// - Exercises the committed runtime inspection data without depending on a
///   CLI inspection command or OTP process metadata.
pub(super) fn measure_vm_process_inspection_startup() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let parent = processes.spawn_root(VmProcessSource::new("bench.Inspect", "parent", 0));
    let child = processes.spawn_child(parent, VmProcessSource::new("bench.Inspect", "child", 1))?;
    processes.send(parent, child, VmPrimitiveValue::String("ready".to_string()))?;

    processes.with_process_control_mutator(child, |child_process| {
        child_process.charge_reductions(11);
        child_process.add_resource_handle("native:inspect-benchmark");
        child_process.block();
    })?;

    let inspected = processes
        .get(child)
        .ok_or_else(|| "child process missing for inspection".to_string())?;
    if inspected.pid != child {
        return Err(format!("inspected pid was {:?}", inspected.pid));
    }
    if inspected.parent != Some(parent) {
        return Err(format!("inspected parent was {:?}", inspected.parent));
    }
    if inspected.source.module != "bench.Inspect"
        || inspected.source.function != "child"
        || inspected.source.arity != 1
    {
        return Err(format!(
            "unexpected source metadata: {:?}",
            inspected.source
        ));
    }
    if inspected.state != process::VmProcessState::Blocked {
        return Err(format!("unexpected inspected state: {:?}", inspected.state));
    }
    if inspected.mailbox_len() != 1 {
        return Err(format!(
            "inspected mailbox depth was {}, expected 1",
            inspected.mailbox_len()
        ));
    }
    if inspected.reductions != 11 {
        return Err(format!(
            "inspected reductions were {}, expected 11",
            inspected.reductions
        ));
    }
    if inspected.resource_handles != ["native:inspect-benchmark".to_string()] {
        return Err(format!(
            "unexpected inspected resources: {:?}",
            inspected.resource_handles
        ));
    }
    Ok(())
}
