use super::*;

impl BenchmarkReport {
    /// Builds a skipped benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark configuration.
    /// - `reason`: typed skip reason.
    ///
    /// Output:
    /// - Serializable skipped report.
    ///
    /// Transformation:
    /// - Preserves environment metadata even when no database is configured.
    pub(super) fn skipped(options: &BenchmarkOptions, reason: impl Into<String>) -> Self {
        Self::base(options, BenchmarkStatus::Skipped, Vec::new(), Vec::new())
            .with_skip_reason(reason)
    }

    /// Builds a failed benchmark report.
    ///
    /// Inputs:
    /// - `options`: benchmark configuration.
    /// - `reason`: failure reason.
    ///
    /// Output:
    /// - Serializable failed report.
    ///
    /// Transformation:
    /// - Captures benchmark failure in the same JSON shape as completed runs.
    pub(super) fn failed(options: &BenchmarkOptions, reason: impl Into<String>) -> Self {
        Self::base(options, BenchmarkStatus::Failed, Vec::new(), Vec::new())
            .with_error_reason(reason)
    }

    /// Builds the shared report header.
    ///
    /// Inputs:
    /// - Benchmark options.
    /// - Report status.
    /// - Measurements and assertions.
    ///
    /// Output:
    /// - Report with common metadata populated.
    ///
    /// Transformation:
    /// - Redacts Postgres URL credentials and records toolchain metadata.
    pub(super) fn base(
        options: &BenchmarkOptions,
        status: BenchmarkStatus,
        measurements: Vec<Measurement>,
        assertions: Vec<AssertionResult>,
    ) -> Self {
        Self {
            benchmark: POSTGRES_COMMAND,
            status,
            timestamp_unix_seconds: unix_timestamp_seconds(),
            terlan_version: env!("CARGO_PKG_VERSION"),
            rustc_version: rustc_version(),
            adapter_stack: AdapterStack::current(),
            postgres_url_source: options.postgres_url_source,
            postgres_url_redacted: options.postgres_url.as_deref().map(redact_postgres_url),
            iterations: options.iterations,
            concurrency: options.concurrency,
            measurements,
            assertions,
            skip_reason: None,
            error_reason: None,
        }
    }

    /// Adds a skip reason to this report.
    pub(super) fn with_skip_reason(mut self, reason: impl Into<String>) -> Self {
        self.skip_reason = Some(reason.into());
        self
    }

    /// Adds an error reason to this report.
    pub(super) fn with_error_reason(mut self, reason: impl Into<String>) -> Self {
        self.error_reason = Some(reason.into());
        self
    }
}

/// HTTP adapter stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
pub(super) struct HttpAdapterStack {
    pub(super) response_boundary: &'static str,
    pub(super) request_boundary: &'static str,
    pub(super) serialization: &'static str,
    pub(super) socket_runtime: &'static str,
}

impl HttpAdapterStack {
    /// Returns the current HTTP adapter stack names.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Maintained crate names and boundary labels.
    ///
    /// Transformation:
    /// - Records that this first baseline measures native response/request
    ///   conversion without binding Hyper sockets.
    pub(super) fn current() -> Self {
        Self {
            response_boundary: "http crate response conversion",
            request_boundary: "Terlan native HTTP request snapshot",
            serialization: "serde_json through std.data.Json adapter",
            socket_runtime: "no socket; handler/response boundary only",
        }
    }
}

/// Benchmark execution status.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchmarkStatus {
    Completed,
    Skipped,
    Failed,
}

/// Adapter stack captured by the benchmark.
#[derive(Debug, Clone, Serialize)]
pub(super) struct AdapterStack {
    pub(super) pool: &'static str,
    pub(super) postgres_client: &'static str,
    pub(super) async_runtime: &'static str,
    pub(super) runtime_lifecycle: &'static str,
    pub(super) boundary: &'static str,
}

impl AdapterStack {
    /// Returns the current adapter stack names.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - Maintained crate names and boundary label.
    ///
    /// Transformation:
    /// - Keeps the baseline explicit about which old path is measured.
    pub(super) fn current() -> Self {
        Self {
            pool: "terlan-vm",
            postgres_client: "generated-libpq-c-abi",
            async_runtime: "terlan-vm-reactor",
            runtime_lifecycle: "VM-owned actor and resource lifecycle",
            boundary: "VM NativeBoundary libpq adapter",
        }
    }
}

/// One benchmark measurement summary.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Measurement {
    pub(super) name: &'static str,
    pub(super) unit: &'static str,
    pub(super) iterations: usize,
    pub(super) total_ns: u128,
    pub(super) min_ns: u128,
    pub(super) mean_ns: u128,
    pub(super) p50_ns: u128,
    pub(super) p95_ns: u128,
    pub(super) p99_ns: u128,
    pub(super) max_ns: u128,
    pub(super) total_us: u128,
    pub(super) min_us: u128,
    pub(super) mean_us: u128,
    pub(super) p50_us: u128,
    pub(super) p95_us: u128,
    pub(super) p99_us: u128,
    pub(super) max_us: u128,
}

/// Correctness assertion captured next to timing data.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct AssertionResult {
    pub(crate) name: &'static str,
    pub(crate) passed: bool,
    pub(crate) detail: String,
}

/// Runs the Postgres baseline benchmark.
///
/// Inputs:
/// - `options`: URL, output, iteration, and concurrency configuration.
///
/// Output:
/// - Completed, skipped, or failed benchmark report.
///
/// Transformation:
/// - Connects through the current adapter, prepares a small benchmark table,
///   runs sequential and concurrent operation measurements, and validates
///   every measured operation.
pub(super) fn run_postgres_baseline(options: &BenchmarkOptions) -> Result<BenchmarkReport, String> {
    let Some(url) = options.postgres_url.as_deref() else {
        return Ok(BenchmarkReport::skipped(
            options,
            "postgres_url_unconfigured",
        ));
    };
    let config = Config::new(url)
        .with_pool_limits(1, options.concurrency.max(2))
        .with_timeouts(5_000, 5_000);
    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    let (pool, connect_measurement) = measure_connect(&config)?;
    measurements.push(connect_measurement);

    prepare_benchmark_table(&pool)?;
    let query = "SELECT value FROM terlan_bench_native_boundary WHERE id = 1";
    let insert = "INSERT INTO terlan_bench_native_boundary(value) VALUES ($1)";

    measurements.push(measure_repeated(
        "query_one_select_int",
        options.iterations,
        || {
            let row = postgres::query_one(&pool, query, &[])
                .map_err(format_postgres_error)?
                .ok_or_else(|| "query_one returned no row".to_string())?;
            let value = postgres::int(&row, "value").map_err(format_postgres_error)?;
            if value == 1 {
                Ok(())
            } else {
                Err(format!("query_one returned unexpected value {value}"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "query_one_select_int",
        passed: true,
        detail: "all measured query_one calls returned value 1".to_string(),
    });

    measurements.push(measure_repeated(
        "execute_insert_param",
        options.iterations,
        || {
            let affected =
                postgres::execute(&pool, insert, &[json::int(2)]).map_err(format_postgres_error)?;
            if affected == 1 {
                Ok(())
            } else {
                Err(format!("execute affected {affected} rows"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "execute_insert_param",
        passed: true,
        detail: "all measured execute calls affected one row".to_string(),
    });

    measurements.push(measure_repeated(
        "transaction_empty_commit",
        options.iterations,
        || {
            let value =
                postgres::transaction(&pool, |_connection| Ok(1)).map_err(format_postgres_error)?;
            if value == 1 {
                Ok(())
            } else {
                Err(format!("transaction returned unexpected value {value}"))
            }
        },
    )?);
    assertions.push(AssertionResult {
        name: "transaction_empty_commit",
        passed: true,
        detail: "all measured transactions committed".to_string(),
    });

    measurements.push(measure_concurrent_query_one(
        &pool,
        options.concurrency,
        options.iterations,
    )?);
    assertions.push(AssertionResult {
        name: "concurrent_query_one_select_int",
        passed: true,
        detail: format!(
            "{} workers each completed {} query_one calls",
            options.concurrency, options.iterations
        ),
    });

    Ok(BenchmarkReport::base(
        options,
        BenchmarkStatus::Completed,
        measurements,
        assertions,
    ))
}

/// Runs the VM performance baseline benchmark.
///
/// Inputs:
/// - `options`: output, iteration, and optional local binary configuration.
///
/// Output:
/// - Completed VM baseline report or a failure reason.
///
/// Transformation:
/// - Resolves local `terlan-vm` and `terlc` binaries, creates deterministic
///   temporary source programs, measures supported VM paths, and records
///   explicit skipped rows for VM-owned tracks that are not executable yet.
pub(super) fn run_vm_performance_baseline(
    options: &VmBenchmarkOptions,
) -> Result<VmBenchmarkReport, String> {
    let vm_binary = resolve_benchmark_binary("terlan-vm", options.vm_binary.as_deref())?;
    let compiler_binary = resolve_benchmark_binary("terlc", options.compiler_binary.as_deref())?;
    let workspace = create_vm_benchmark_workspace()?;
    let single_source = write_vm_benchmark_source(
        &workspace,
        "Single.terl",
        "module bench.Single.\n\npub main(): Bool ->\n    1 + 2 == 3.\n",
    )?;
    let project_source = write_vm_benchmark_source(
        &workspace,
        "Project.terl",
        &synthetic_helper_source("bench.Project", 20),
    )?;
    let collection_source = write_vm_benchmark_source(
        &workspace,
        "Collections.terl",
        "module bench.Collections.\n\nimport std.collections.List.\n\npub main(): Bool ->\n    let values = List.new();\n    values.push(1);\n    values.push(2);\n    values.length() == 2.\n",
    )?;
    let http_response_source = write_vm_benchmark_source(
        &workspace,
        "HttpResponse.terl",
        "module bench.HttpResponse.\n\nimport std.http.Response.\n\npub main(): Bool ->\n    let text = Response.text(\"ok\");\n    let json = Response.json_text(\"{\\\"ok\\\":true}\");\n    true.\n",
    )?;
    let http_router_source = write_vm_benchmark_source(
        &workspace,
        "HttpRouter.terl",
        "module bench.HttpRouter.\n\nimport std.http.Response.\nimport std.http.Router.\nimport type std.http.Request.Request.\nimport type std.http.Response.Response.\nimport type std.http.Router.Router.\n\npub home(_request: Request): Response ->\n    Response.text(\"ok\").\n\npub users(router: Router): Router ->\n    Router.get(router, \"/:id\", home).\n\npub main(): Bool ->\n    let router = Router.group(Router.get(Router.new(), \"/\", home), \"/users\", users);\n    true.\n",
    )?;
    let agent_source = write_vm_benchmark_source(
        &workspace,
        "AgentBench.terl",
        "module bench.AgentBench.\n\nimport std.vm.Agent.\nimport std.core.Result.{Ok, Err}.\n\npub inc(value: Int): Int ->\n    value + 1.\n\npub update_agent(agent: Agent[Int]): Bool ->\n    agent.update(inc);\n    agent.get() == 43.\n\npub main(): Bool ->\n    case Agent.start(42) {\n        Ok(agent) -> update_agent(agent);\n        Err(_) -> false\n    }.\n",
    )?;
    let native_bridge_source = write_vm_benchmark_source(
        &workspace,
        "NativeBridgeBench.terl",
        "module bench.NativeBridgeBench.\n\nimport std.vm.NativeBridge.\nimport std.vm.NativeBridge.{NativeTransfer}.\nimport std.core.Result.\nimport std.core.Result.{Ok, Err}.\n\npub call_bridge(bridge: NativeBridge[String]): Bool ->\n    Result.with_default(bridge.call(\"ping\"), \"failed\") == \"ping\".\n\npub close_bridge(bridge: NativeBridge[String]): Bool ->\n    let called = call_bridge(bridge);\n    bridge.dispose();\n    bridge.stop();\n    called.\n\npub main(): Bool ->\n    case NativeBridge.start(\"resource\") {\n        Ok(bridge) -> close_bridge(bridge);\n        Err(reason) -> false\n    }.\n",
    )?;
    let task_source = write_vm_benchmark_source(
        &workspace,
        "TaskBench.terl",
        "module bench.TaskBench.\n\nimport std.vm.Task.\nimport std.core.Result.\nimport std.core.Result.{Ok, Err}.\n\npub work(): Int ->\n    41 + 1.\n\npub read_task(task: Task[Int]): Bool ->\n    Result.with_default(task.result(), 0) == 42.\n\npub close_task(task: Task[Int]): Bool ->\n    let read = read_task(task);\n    task.cancel();\n    read.\n\npub main(): Bool ->\n    case Task.start(work) {\n        Ok(task) -> close_task(task);\n        Err(reason) -> false\n    }.\n",
    )?;
    let timeout_source = write_vm_benchmark_source(
        &workspace,
        "TimeoutBench.terl",
        "module bench.TimeoutBench.\n\nimport std.vm.Timeout.\n\npub main(): Bool ->\n    Timeout.milliseconds(10) != Timeout.forever().\n",
    )?;
    let timer_wakeup_source = write_vm_benchmark_source(
        &workspace,
        "TimerWakeup.terl",
        "module bench.TimerWakeup.\n\nimport std.vm.Timeout.\n\npub main(): Bool ->\n    let finite = Timeout.milliseconds(10);\n        let forever = Timeout.forever();\n    Timeout.wakes_after(finite, 9) == false and Timeout.wakes_after(finite, 10) and Timeout.wakes_after(forever, 100) == false.\n",
    )?;
    let bytes_source = write_vm_benchmark_source(
        &workspace,
        "BytesBench.terl",
        "module bench.BytesBench.\n\nimport std.vm.Bytes.\n\npub main(): Bool ->\n    let left = Bytes.from_list([1, 2]);\n        let right = Bytes.from_list([3]);\n        let joined = Bytes.concat(left, right);\n    joined.length() == 3 and joined.to_list() == [1, 2, 3].\n",
    )?;
    let port_source = write_vm_benchmark_source(
        &workspace,
        "PortBench.terl",
        "module bench.PortBench.\n\nimport std.vm.Port.\n\npub main(): Bool ->\n    let env = Port.env(\"TERLAN\", \"1\");\n        let command = Port.command(\"echo\", [\"hello\"], [env]);\n    true.\n",
    )?;
    let large_app_source = write_vm_benchmark_source(
        &workspace,
        "LargeAppSized.terl",
        &synthetic_helper_source("bench.LargeAppSized", 80),
    )?;

    let mut measurements = Vec::new();
    let mut assertions = Vec::new();

    measurements.push(measure_repeated(
        "vm_binary_version_startup",
        options.iterations,
        || {
            let output = run_required_command(&vm_binary, &["--version"])?;
            require_stdout_contains("vm_binary_version_startup", &output, "terlan-vm")
        },
    )?);
    assertions.push(assertion(
        "vm_binary_version_startup",
        "terlan-vm --version completed and identified the VM binary",
    ));

    measurements.push(measure_repeated(
        "vm_artifact_build_single_file",
        options.iterations,
        || measure_vm_artifact_build_single_file(&compiler_binary, &single_source),
    )?);
    assertions.push(assertion(
        "vm_artifact_build_single_file",
        "terlc build --target terlan-vm emitted a native single-file .tvm application image",
    ));

    measurements.push(measure_repeated(
        "vm_artifact_load_single_file",
        options.iterations,
        || measure_vm_artifact_load_single_file(&vm_binary, &compiler_binary, &single_source),
    )?);
    assertions.push(assertion(
        "vm_artifact_load_single_file",
        "terlan-vm load admitted a compiler-emitted native .tvm application image",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_single_file",
        options.iterations,
        || run_vm_source_test(&vm_binary, &single_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_single_file",
        "single-file Terlan source compiled and returned Bool true through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_project_sized_synthetic",
        options.iterations,
        || run_vm_source_test(&vm_binary, &project_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_project_sized_synthetic",
        "synthetic project-sized source compiled and returned Bool true",
    ));

    measurements.push(measure_repeated(
        "vm_source_collection_operations",
        options.iterations,
        || run_vm_source_test(&vm_binary, &collection_source),
    )?);
    assertions.push(assertion(
        "vm_source_collection_operations",
        "source-level std.collections.List construction, push, and length executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_http_response_operations",
        options.iterations,
        || run_vm_source_test(&vm_binary, &http_response_source),
    )?);
    assertions.push(assertion(
        "vm_source_http_response_operations",
        "source-level std.http.Response text and JSON response construction executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_http_router_handler_operations",
        options.iterations,
        || run_vm_source_test(&vm_binary, &http_router_source),
    )?);
    assertions.push(assertion(
        "vm_source_http_router_handler_operations",
        "source-level std.http.Router route registration with local handler references executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_agent_update_get",
        options.iterations,
        || run_vm_source_test(&vm_binary, &agent_source),
    )?);
    assertions.push(assertion(
        "vm_source_agent_update_get",
        "source-level std.vm.Agent start result matching, receiver update, and receiver get executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_native_bridge_lifecycle",
        options.iterations,
        || run_vm_source_test(&vm_binary, &native_bridge_source),
    )?);
    assertions.push(assertion(
        "vm_source_native_bridge_lifecycle",
        "source-level std.vm.NativeBridge start, call, dispose, and stop executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_task_lifecycle",
        options.iterations,
        || run_vm_source_test(&vm_binary, &task_source),
    )?);
    assertions.push(assertion(
        "vm_source_task_lifecycle",
        "source-level std.vm.Task start, result, and cancel executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_timeout_constructors",
        options.iterations,
        || run_vm_source_test(&vm_binary, &timeout_source),
    )?);
    assertions.push(assertion(
        "vm_source_timeout_constructors",
        "source-level std.vm.Timeout finite and forever constructors executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_timer_wakeup",
        options.iterations,
        || run_vm_source_test(&vm_binary, &timer_wakeup_source),
    )?);
    assertions.push(assertion(
        "vm_source_timer_wakeup",
        "source-level std.vm.Timeout deterministic wakeup checks executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_bytes_operations",
        options.iterations,
        || run_vm_source_test(&vm_binary, &bytes_source),
    )?);
    assertions.push(assertion(
        "vm_source_bytes_operations",
        "source-level std.vm.Bytes construction, concat, length, and list conversion executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_source_port_command_descriptors",
        options.iterations,
        || run_vm_source_test(&vm_binary, &port_source),
    )?);
    assertions.push(assertion(
        "vm_source_port_command_descriptors",
        "source-level std.vm.Port env and command descriptor construction executed through VM test-eval",
    ));

    measurements.push(measure_repeated(
        "vm_compile_run_large_app_sized_synthetic",
        options.iterations,
        || run_vm_source_test(&vm_binary, &large_app_source),
    )?);
    assertions.push(assertion(
        "vm_compile_run_large_app_sized_synthetic",
        "synthetic large-app-sized source compiled and returned Bool true",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_process_spawn_mailbox_exit",
        options.iterations,
        measure_vm_process_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_process_spawn_mailbox_exit",
        "VM process primitive spawned parent/child processes, delivered ordered messages, preserved skipped mailbox state, and cleaned up on exit",
    ));

    measurements.push(measure_repeated(
        "vm_inspect_processes_startup",
        options.iterations,
        measure_vm_process_inspection_startup,
    )?);
    assertions.push(assertion(
        "vm_inspect_processes_startup",
        "VM process inspection exposed process identity, source metadata, state, mailbox depth, reductions, and owned resources",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_scheduler_yield_block_exit",
        options.iterations,
        measure_vm_scheduler_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_scheduler_yield_block_exit",
        "VM scheduler primitive ran a process slice, requeued yield, handled block/wake, and exited with cleanup",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_resource_register_transfer_cleanup",
        options.iterations,
        measure_vm_resource_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_resource_register_transfer_cleanup",
        "VM resource primitive registered typed handles, enforced ownership, transferred, released, and cleaned up on exit",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_cancellation_resource_cleanup",
        options.iterations,
        measure_vm_cancellation_resource_cleanup_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_cancellation_resource_cleanup",
        "VM scheduler cancellation surfaced owned resource handles and resource cleanup made the old handle stale",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_table_insert_lookup_delete",
        options.iterations,
        measure_vm_table_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_table_insert_lookup_delete",
        "VM table primitive created a table, inserted, looked up, deleted, and exposed empty snapshot state",
    ));

    for size in MAP_BENCHMARK_SIZES {
        measurements.push(measure_vm_map_workload(*size, options.iterations)?);
        measurements.push(measure_otp_map_workload(*size, options.iterations)?);
        assertions.push(assertion(
            map_assertion_name(*size),
            format!(
                "Terlan VM flat map and OTP maps both completed insert, lookup, and persistent update workloads for {size} entries"
            ),
        ));
    }
    assert_vm_wins_large_map_reference_lane(&measurements)?;
    assertions.push(assertion(
        "terlan_vm_map_insert_lookup_update_size_5000_beats_otp",
        "Terlan VM completed the 5,000-key map insert/lookup/private-update workload faster than the OTP reference lane",
    ));

    measurements.push(measure_vm_collision_heavy_map_workload(options.iterations)?);
    assertions.push(assertion(
        "terlan_vm_collision_heavy_map_workload",
        "Terlan VM completed a forced hash-collision map workload with stable lookup and update behavior",
    ));

    measurements.push(measure_vm_shared_persistent_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    measurements.push(measure_otp_shared_persistent_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    assertions.push(assertion(
        "map_shared_persistent_update_workload_matches_otp_size_5000",
        "Terlan VM and OTP both completed shared persistent update workloads while preserving the original map",
    ));

    measurements.push(measure_vm_iterator_equality_rendering_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    measurements.push(measure_otp_iterator_equality_rendering_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    assertions.push(assertion(
        "map_iterator_equality_rendering_workload_matches_otp_size_5000",
        "Terlan VM and OTP both completed iterator/equality/rendering workloads for large maps",
    ));

    measurements.push(measure_vm_mixed_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    measurements.push(measure_otp_mixed_map_workload(
        MAP_STRESS_SIZE,
        options.iterations,
    )?);
    assertions.push(assertion(
        "map_mixed_insert_remove_update_workload_matches_otp_size_5000",
        "Terlan VM and OTP both completed mixed insert/remove/update workloads for large maps",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_actor_send_receive",
        options.iterations,
        measure_vm_actor_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_actor_send_receive",
        "VM actor primitive spawned processes, sent one message, and received it through the mailbox",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_selective_receive",
        options.iterations,
        measure_vm_selective_receive_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_selective_receive",
        "VM actor primitive preserved skipped messages while receiving the selected mailbox payload",
    ));

    measurements.push(measure_repeated(
        "vm_runtime_timer_wakeup",
        options.iterations,
        measure_vm_timer_runtime_primitives,
    )?);
    assertions.push(assertion(
        "vm_runtime_timer_wakeup",
        "VM timer primitive blocked a process, fired the receive timeout, and woke the owner",
    ));

    Ok(VmBenchmarkReport::completed(
        options,
        VmRuntimeStack::resolved(&vm_binary, &compiler_binary),
        measurements,
        assertions,
        vm_performance_skipped_tracks(),
    ))
}

/// Runs the VM-backed HTTP runtime benchmark.
///
/// Inputs:
/// - `options`: output, iteration, and optional compiler binary settings.
///
/// Output:
/// - Completed VM HTTP benchmark report or a failure reason.
///
/// Transformation:
/// - Creates an isolated browser package with a source-backed dynamic handler,
///   starts `terlc serve`, and measures real HTTP requests through the VM
///   handler runtime. The old native host-runtime baseline is not touched.
pub(super) fn run_vm_http_runtime_baseline(
    options: &VmHttpBenchmarkOptions,
) -> Result<VmHttpBenchmarkReport, String> {
    let compiler_binary = resolve_benchmark_binary("terlc", options.compiler_binary.as_deref())?;
    let workspace = create_vm_http_benchmark_workspace()?;
    let web_root = write_vm_http_benchmark_package(&workspace)?;
    let port = reserve_localhost_port()?;
    let port_text = port.to_string();
    let web_root_text = web_root.to_string_lossy().to_string();
    let child = Command::new(&compiler_binary)
        .args([
            "serve",
            &web_root_text,
            "--host",
            "127.0.0.1",
            "--port",
            &port_text,
            "--poll-ms",
            "60000",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            format!(
                "failed to start VM HTTP benchmark server `{}`: {error}",
                compiler_binary.display()
            )
        })?;
    let _server = ChildProcessGuard::new(child);
    wait_for_vm_http_server(port)?;

    let measurements = vec![measure_repeated(
        "vm_http_dynamic_handler_round_trip",
        options.iterations,
        || vm_http_round_trip(port),
    )?];
    let _ = fs::remove_dir_all(&workspace);
    Ok(VmHttpBenchmarkReport::completed(
        options,
        VmHttpRuntimeStack::resolved(&compiler_binary),
        measurements,
        vec![assertion(
            "vm_http_dynamic_handler_round_trip",
            "all measured loopback HTTP requests reached a VM-backed Terlan handler",
        )],
    ))
}

/// Creates a unique temporary workspace for VM HTTP benchmark sources.
pub(super) fn create_vm_http_benchmark_workspace() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!(
        "terlan-vm-http-runtime-baseline-{}-{}",
        std::process::id(),
        unix_timestamp_nanos()
    ));
    fs::create_dir_all(&path)
        .map_err(|error| format!("failed to create VM HTTP benchmark workspace: {error}"))?;
    Ok(path)
}
