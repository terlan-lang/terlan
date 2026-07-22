
fn assert_vm_wins_large_map_reference_lane(measurements: &[Measurement]) -> Result<(), String> {
    let vm = measurement_mean_ns(
        measurements,
        vm_map_measurement_name(LARGE_MAP_REFERENCE_SIZE),
    )?;
    let otp = measurement_mean_ns(
        measurements,
        otp_map_measurement_name(LARGE_MAP_REFERENCE_SIZE),
    )?;
    if vm < otp {
        return Ok(());
    }
    Err(format!(
        "Terlan VM map benchmark must beat OTP for {LARGE_MAP_REFERENCE_SIZE} entries; Terlan VM mean_ns={vm}, OTP mean_ns={otp}"
    ))
}

fn measurement_mean_ns(measurements: &[Measurement], name: &str) -> Result<u128, String> {
    measurements
        .iter()
        .find(|measurement| measurement.name == name)
        .map(|measurement| measurement.mean_ns)
        .ok_or_else(|| format!("missing benchmark measurement `{name}`"))
}

/// Returns the correctness assertion name for one map comparison size.
///
/// Inputs:
/// - `size`: map cardinality.
///
/// Output:
/// - Stable assertion name paired with both VM and OTP map rows.
///
/// Transformation:
/// - Makes the benchmark report show that the two map lanes ran equivalent
///   insert, lookup, and update semantics.
fn map_assertion_name(size: usize) -> &'static str {
    match size {
        16 => "map_workload_matches_otp_size_16",
        32 => "map_workload_matches_otp_size_32",
        33 => "map_workload_matches_otp_size_33",
        128 => "map_workload_matches_otp_size_128",
        5_000 => "map_workload_matches_otp_size_5000",
        _ => "map_workload_matches_otp_size_custom",
    }
}

/// Measures VM-owned actor send/receive primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when one actor sends and another receives a stable payload.
///
/// Transformation:
/// - Exercises local actor/process semantics without pretending distributed
///   or source-level actor syntax is already available.
fn measure_vm_actor_runtime_primitives() -> Result<(), String> {
    let mut actors = VmActorRuntime::default();
    let sender = actors.spawn_root(VmProcessSource::new("bench.Actor", "sender", 0));
    let recipient = actors.spawn_root(VmProcessSource::new("bench.Actor", "recipient", 0));
    actors.send(
        sender,
        recipient,
        VmPrimitiveValue::Atom("ping".to_string()),
    )?;
    match actors.receive_next_or_block(recipient)? {
        VmActorReceive::Message(message)
            if message.sender == sender
                && message.payload == VmPrimitiveValue::Atom("ping".to_string()) =>
        {
            Ok(())
        }
        other => Err(format!("unexpected actor receive result: {other:?}")),
    }
}

/// Measures VM-owned selective receive primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when a selected mailbox message is received while earlier
///   nonmatching messages remain queued.
///
/// Transformation:
/// - Exercises the selective receive cursor behavior at the actor facade
///   without requiring source-level receive syntax to exist yet.
fn measure_vm_selective_receive_primitives() -> Result<(), String> {
    let mut actors = VmActorRuntime::default();
    let sender = actors.spawn_root(VmProcessSource::new("bench.Actor", "sender", 0));
    let recipient = actors.spawn_root(VmProcessSource::new("bench.Actor", "recipient", 0));
    actors.send(
        sender,
        recipient,
        VmPrimitiveValue::Atom("skip".to_string()),
    )?;
    actors.send(
        sender,
        recipient,
        VmPrimitiveValue::Atom("take".to_string()),
    )?;

    match actors.selective_receive_or_block(recipient, |message| {
        message.payload == VmPrimitiveValue::Atom("take".to_string())
    })? {
        VmActorReceive::Message(message)
            if message.sender == sender
                && message.payload == VmPrimitiveValue::Atom("take".to_string()) =>
        {
            match actors.receive_next_or_block(recipient)? {
                VmActorReceive::Message(remaining)
                    if remaining.payload == VmPrimitiveValue::Atom("skip".to_string()) =>
                {
                    Ok(())
                }
                other => Err(format!(
                    "selective receive did not preserve skipped message: {other:?}"
                )),
            }
        }
        other => Err(format!("unexpected selective receive result: {other:?}")),
    }
}

/// Measures VM-owned receive-timeout wakeup primitives directly.
///
/// Inputs:
/// - No external input.
///
/// Output:
/// - Success when a receive-timeout timer fires and wakes its owner process.
///
/// Transformation:
/// - Exercises timer/scheduler integration without relying on host async or OS
///   runtime timers.
fn measure_vm_timer_runtime_primitives() -> Result<(), String> {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("bench.Timer", "main", 0));
    let mut scheduler = VmScheduler::default();
    let mut timers = VmTimerTable::default();
    let timer = timers.start_receive_timeout(&mut processes, &mut scheduler, owner, 10, 5)?;
    let events = timers.advance_clock(&mut processes, &mut scheduler, 15);
    let fired = events.iter().any(|event| {
        matches!(
            event,
            timer::VmTimerEvent::Fired {
                timer_id,
                owner: event_owner,
                kind: VmTimerKind::ReceiveTimeout
            } if *timer_id == timer && *event_owner == owner
        )
    });
    if !fired {
        return Err(format!("receive-timeout timer did not fire: {events:?}"));
    }
    let snapshot_len = timers.snapshots().len();
    if snapshot_len == 0 {
        Ok(())
    } else {
        Err(format!("fired timer remained in snapshot: {snapshot_len}"))
    }
}

/// Captured command output for benchmark assertions.
struct BenchmarkCommandOutput {
    stdout: String,
    stderr: String,
}

/// Runs a benchmark command and requires a successful exit status.
fn run_required_command(program: &Path, args: &[&str]) -> Result<BenchmarkCommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("failed to start `{}`: {error}", program.display()))?;
    if !output.status.success() {
        return Err(format_command_failure(
            &program.display().to_string(),
            &output,
        ));
    }
    Ok(BenchmarkCommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// Requires command stdout to contain an expected string.
fn require_stdout_contains(
    track: &str,
    output: &BenchmarkCommandOutput,
    expected: &str,
) -> Result<(), String> {
    if output.stdout.contains(expected) {
        Ok(())
    } else {
        Err(format!(
            "{track} stdout did not contain `{expected}`; stdout=`{}` stderr=`{}`",
            output.stdout.trim(),
            output.stderr.trim()
        ))
    }
}

/// Formats an unsuccessful command result.
fn format_command_failure(command: &str, output: &std::process::Output) -> String {
    format!(
        "`{command}` failed with status {}; stdout=`{}` stderr=`{}`",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Builds a passed assertion record.
fn assertion(name: &'static str, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name,
        passed: true,
        detail: detail.into(),
    }
}

/// Returns required VM performance tracks that are not executable yet.
fn vm_performance_skipped_tracks() -> Vec<SkippedTrack> {
    REQUIRED_VM_SKIPPED_TRACKS.to_vec()
}

/// Measures adapter connection setup.
///
/// Inputs:
/// - `config`: Postgres adapter configuration.
///
/// Output:
/// - Connected pool and one measurement for connect time.
///
/// Transformation:
/// - Times the current adapter's `connect` call including runtime creation,
///   deadpool construction, and minimum connection warmup.
fn measure_connect(config: &Config) -> Result<(Pool, Measurement), String> {
    let start = Instant::now();
    let pool = postgres::connect(config).map_err(format_postgres_error)?;
    let duration = start.elapsed();
    Ok((
        pool,
        Measurement::from_durations("connect_pool_warmup", &[duration]),
    ))
}

/// Creates and seeds the benchmark table.
///
/// Inputs:
/// - `pool`: connected Postgres pool.
///
/// Output:
/// - Success when the table is ready.
///
/// Transformation:
/// - Uses the same adapter execute path that the benchmark later measures.
fn prepare_benchmark_table(pool: &Pool) -> Result<(), String> {
    postgres::execute(
        pool,
        "DROP TABLE IF EXISTS terlan_bench_native_boundary",
        &[],
    )
    .map_err(format_postgres_error)?;
    postgres::execute(
        pool,
        "CREATE TABLE terlan_bench_native_boundary(id BIGSERIAL PRIMARY KEY, value BIGINT NOT NULL)",
        &[],
    )
    .map_err(format_postgres_error)?;
    postgres::execute(
        pool,
        "INSERT INTO terlan_bench_native_boundary(value) VALUES (1)",
        &[],
    )
    .map_err(format_postgres_error)?;
    Ok(())
}

/// Measures a repeated synchronous adapter operation.
///
/// Inputs:
/// - `name`: measurement name.
/// - `iterations`: operation count.
/// - `operation`: fallible operation to run.
///
/// Output:
/// - Timing summary for successful operation runs.
///
/// Transformation:
/// - Records wall-clock duration for each call and stops on the first
///   correctness or adapter error.
fn measure_repeated(
    name: &'static str,
    iterations: usize,
    mut operation: impl FnMut() -> Result<(), String>,
) -> Result<Measurement, String> {
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        operation()?;
        durations.push(start.elapsed());
    }
    Ok(Measurement::from_durations(name, &durations))
}

/// Measures concurrent query_one calls through the current adapter.
///
/// Inputs:
/// - `pool`: connected Postgres pool.
/// - `concurrency`: number of worker threads.
/// - `iterations`: calls per worker.
///
/// Output:
/// - Timing summary for worker wall-clock durations.
///
/// Transformation:
/// - Clones the adapter pool handle into OS threads. Each call still uses the
///   current synchronous Rust adapter path, including per-call runtime
///   creation inside the adapter.
fn measure_concurrent_query_one(
    pool: &Pool,
    concurrency: usize,
    iterations: usize,
) -> Result<Measurement, String> {
    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let worker_pool = pool.clone();
        handles.push(thread::spawn(move || {
            let start = Instant::now();
            for _ in 0..iterations {
                let row = postgres::query_one(
                    &worker_pool,
                    "SELECT value FROM terlan_bench_native_boundary WHERE id = 1",
                    &[],
                )
                .map_err(format_postgres_error)?
                .ok_or_else(|| "query_one returned no row".to_string())?;
                let value = postgres::int(&row, "value").map_err(format_postgres_error)?;
                if value != 1 {
                    return Err(format!("query_one returned unexpected value {value}"));
                }
            }
            Ok(start.elapsed())
        }));
    }
    let mut durations = Vec::with_capacity(concurrency);
    for handle in handles {
        let duration = handle
            .join()
            .map_err(|_| "concurrent query worker panicked".to_string())??;
        durations.push(duration);
    }
    Ok(Measurement::from_durations(
        "concurrent_query_one_select_int_worker_wall_time",
        &durations,
    ))
}

impl Measurement {
    /// Builds a timing summary from raw durations.
    ///
    /// Inputs:
    /// - `name`: measurement name.
    /// - `durations`: non-empty operation durations.
    ///
    /// Output:
    /// - Nanosecond and microsecond summaries with min, mean, p50, p95, and
    ///   max.
    ///
    /// Transformation:
    /// - Sorts copied duration values and uses nearest-rank percentiles. The
    ///   nanosecond fields keep very small HTTP adapter operations visible;
    ///   the microsecond fields stay useful for slower database and worker
    ///   wall-time paths.
    pub(crate) fn from_durations(name: &'static str, durations: &[Duration]) -> Self {
        let mut values_ns = durations.iter().map(Duration::as_nanos).collect::<Vec<_>>();
        values_ns.sort_unstable();
        let total_ns = values_ns.iter().sum::<u128>();
        let mut values_us = durations
            .iter()
            .map(Duration::as_micros)
            .collect::<Vec<_>>();
        values_us.sort_unstable();
        let total_us = values_us.iter().sum::<u128>();
        let iterations = values_ns.len();
        Self {
            name,
            unit: "microseconds",
            iterations,
            total_ns,
            min_ns: values_ns[0],
            mean_ns: total_ns / iterations as u128,
            p50_ns: percentile(&values_ns, 50),
            p95_ns: percentile(&values_ns, 95),
            p99_ns: percentile(&values_ns, 99),
            max_ns: values_ns[iterations - 1],
            total_us,
            min_us: values_us[0],
            mean_us: total_us / iterations as u128,
            p50_us: percentile(&values_us, 50),
            p95_us: percentile(&values_us, 95),
            p99_us: percentile(&values_us, 99),
            max_us: values_us[iterations - 1],
        }
    }
}

/// Returns a nearest-rank percentile from sorted microsecond values.
///
/// Inputs:
/// - `sorted_values`: sorted non-empty duration values.
/// - `percentile_value`: percentile from 0 to 100.
///
/// Output:
/// - Value at the selected percentile rank.
///
/// Transformation:
/// - Uses integer arithmetic to avoid floating-point rounding in reports.
fn percentile(sorted_values: &[u128], percentile_value: usize) -> u128 {
    let max_index = sorted_values.len() - 1;
    let index = (max_index * percentile_value).div_ceil(100);
    sorted_values[index]
}

/// Formats a stable Postgres adapter error.
///
/// Inputs:
/// - `error`: Postgres adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves the portable error code while keeping driver text available for
///   benchmark failure diagnostics.
fn format_postgres_error(error: PostgresError) -> String {
    format!("error[{}]: {}", error.code(), error.message())
}

/// Formats a stable HTTP adapter error.
///
/// Inputs:
/// - `error`: HTTP adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves the portable error code for benchmark failure diagnostics.
fn format_http_error(error: http::HttpError) -> String {
    format!(
        "error[{}]: {} (status {})",
        error.code(),
        error.message(),
        error.status()
    )
}

/// Formats a stable JSON adapter error.
///
/// Inputs:
/// - `error`: JSON adapter error.
///
/// Output:
/// - Human-readable error with stable code.
///
/// Transformation:
/// - Preserves JSON adapter diagnostics for HTTP benchmark failures.
fn format_json_error(error: json::JsonError) -> String {
    format!(
        "error[{}]: {} (offset {})",
        error.code(),
        error.message(),
        error.offset()
    )
}

/// Writes a benchmark report as pretty JSON.
///
/// Inputs:
/// - `path`: output path.
/// - `report`: report to serialize.
///
/// Output:
/// - Success when the report is written.
///
/// Transformation:
/// - Creates parent directories and serializes stable JSON for checked
///   baseline storage.
pub(crate) fn write_report<T: Serialize>(path: &Path, report: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("{}: failed to create directory: {error}", parent.display())
        })?;
    }
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("failed to serialize benchmark report: {error}"))?;
    fs::write(path, format!("{json}\n")).map_err(|error| {
        format!(
            "{}: failed to write benchmark report: {error}",
            path.display()
        )
    })
}

/// Returns a Unix timestamp in seconds.
///
/// Inputs:
/// - Current system clock.
///
/// Output:
/// - Seconds since Unix epoch, or zero if the system clock is before epoch.
///
/// Transformation:
/// - Keeps report timestamps machine-readable without adding date/time
///   dependencies.
pub(crate) fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Returns a Unix timestamp in nanoseconds.
///
/// Inputs:
/// - Current system clock.
///
/// Output:
/// - Nanoseconds since Unix epoch, or zero if the system clock is before epoch.
///
/// Transformation:
/// - Provides a cheap unique suffix for temporary benchmark workspaces.
fn unix_timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// Reads the installed Rust compiler version.
///
/// Inputs:
/// - Local `rustc` executable on PATH.
///
/// Output:
/// - Version string when available.
///
/// Transformation:
/// - Shells out once for report metadata and tolerates missing toolchain text.
pub(crate) fn rustc_version() -> Option<String> {
    let output = Command::new("rustc").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Redacts credentials from a Postgres URL.
///
/// Inputs:
/// - `url`: original Postgres URL.
///
/// Output:
/// - Redacted URL string, or a stable placeholder when parsing fails.
///
/// Transformation:
/// - Uses the `url` crate rather than string slicing so credentials are not
///   accidentally leaked in benchmark reports.
fn redact_postgres_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return "<invalid postgres url>".to_string();
    };
    let _ = parsed.set_username(if parsed.username().is_empty() {
        ""
    } else {
        "redacted"
    });
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("redacted"));
    }
    parsed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies synthetic VM sources scale helper count and expected result.
    ///
    /// Inputs:
    /// - Module name and helper count.
    ///
    /// Output:
    /// - Test passes when generated source contains the requested helpers and
    ///   stable assertion expression.
    ///
    /// Transformation:
    /// - Protects the project/large-app-sized benchmark source generator from
    ///   silently shrinking to a tiny single-expression workload.
    #[test]
    fn synthetic_helper_source_contains_requested_workload() {
        let source = synthetic_helper_source("bench.Sample", 5);

        assert!(source.contains("module bench.Sample."));
        assert!(source.contains("helper0(value: Int): Int"));
        assert!(source.contains("helper4(value: Int): Int"));
        assert!(source.contains("block0(): Int"));
        assert!(source.contains("helper4(helper3(helper2(helper1(helper0(1)))))"));
        assert!(source.contains("pub main(): Bool ->\n    block0() == 11."));
    }

    /// Verifies the VM performance baseline records all deliberate pending lanes.
    ///
    /// Inputs:
    /// - Static skipped-track list.
    ///
    /// Output:
    /// - Test passes when the skipped tracks match the exact required list.
    ///
    /// Transformation:
    /// - Keeps skipped VM tracks explicit. The current required list is empty
    ///   because previously pending lanes have been promoted to measurements.
    #[test]
    fn vm_performance_skipped_tracks_match_required_policy() {
        let tracks = vm_performance_skipped_tracks();
        let names = tracks.iter().map(|track| track.name).collect::<Vec<_>>();
        let expected_names = REQUIRED_VM_SKIPPED_TRACKS
            .iter()
            .map(|track| track.name)
            .collect::<Vec<_>>();

        assert_eq!(names, expected_names);
        assert_eq!(names.len(), REQUIRED_VM_SKIPPED_TRACKS.len());
        for (index, name) in names.iter().enumerate() {
            assert!(
                !names[..index].contains(name),
                "duplicate skipped performance track `{name}`"
            );
        }
        for track in &tracks {
            assert!(
                track.reason.starts_with("vm_") || track.reason.starts_with("terlan_vm_"),
                "skipped track `{}` has untyped reason `{}`",
                track.name,
                track.reason
            );
            assert!(
                !track.detail.trim().is_empty(),
                "skipped track `{}` has empty detail",
                track.name
            );
        }
    }

    /// Verifies direct VM primitive benchmark tracks execute correctness
    /// assertions independently from source-level VM execution.
    ///
    /// Inputs:
    /// - VM table, actor, and timer primitive benchmark operations.
    ///
    /// Output:
    /// - Test passes when all primitive operations complete successfully.
    ///
    /// Transformation:
    /// - Prevents the performance baseline from reporting primitive timings
    ///   before the underlying runtime operations are actually exercised.
    #[test]
    fn vm_runtime_primitive_benchmark_operations_are_executable() {
        measure_vm_process_runtime_primitives().expect("process primitive benchmark");
        measure_vm_scheduler_runtime_primitives().expect("scheduler primitive benchmark");
        measure_vm_resource_runtime_primitives().expect("resource primitive benchmark");
        measure_vm_cancellation_resource_cleanup_primitives()
            .expect("cancellation resource cleanup primitive benchmark");
        measure_vm_table_runtime_primitives().expect("table primitive benchmark");
        run_vm_map_workload(16).expect("VM map benchmark size 16");
        run_vm_map_workload(32).expect("VM map benchmark size 32");
        run_vm_map_workload(33).expect("VM map benchmark size 33");
        run_vm_map_workload(128).expect("VM map benchmark size 128");
        measure_vm_actor_runtime_primitives().expect("actor primitive benchmark");
        measure_vm_selective_receive_primitives().expect("selective receive primitive benchmark");
        measure_vm_timer_runtime_primitives().expect("timer primitive benchmark");
    }

    /// Verifies map benchmark names cover the OTP threshold pressure points.
    ///
    /// Inputs:
    /// - Static map benchmark sizes.
    ///
    /// Output:
    /// - Test passes when row names remain split by Terlan VM and OTP.
    ///
    /// Transformation:
    /// - Protects the benchmark from losing the 32-to-33-entry comparison that
    ///   motivates the VM adaptive-map work.
    #[test]
    fn map_benchmark_tracks_cover_otp_threshold_sizes() {
        assert_eq!(MAP_BENCHMARK_SIZES, &[16, 32, 33, 127, 128, 129, 5_000]);
        assert_eq!(
            MAP_BENCHMARK_SIZES
                .iter()
                .map(|size| vm_map_measurement_name(*size))
                .collect::<Vec<_>>(),
            vec![
                "terlan_vm_map_insert_lookup_update_size_16",
                "terlan_vm_map_insert_lookup_update_size_32",
                "terlan_vm_map_insert_lookup_update_size_33",
                "terlan_vm_map_insert_lookup_update_size_127",
                "terlan_vm_map_insert_lookup_update_size_128",
                "terlan_vm_map_insert_lookup_update_size_129",
                "terlan_vm_map_insert_lookup_update_size_5000",
            ]
        );
        assert_eq!(
            MAP_BENCHMARK_SIZES
                .iter()
                .map(|size| otp_map_measurement_name(*size))
                .collect::<Vec<_>>(),
            vec![
                "otp_map_insert_lookup_update_size_16",
                "otp_map_insert_lookup_update_size_32",
                "otp_map_insert_lookup_update_size_33",
                "otp_map_insert_lookup_update_size_127",
                "otp_map_insert_lookup_update_size_128",
                "otp_map_insert_lookup_update_size_129",
                "otp_map_insert_lookup_update_size_5000",
            ]
        );
    }

    #[test]
    fn large_map_reference_lane_requires_vm_to_beat_otp() {
        let winning = vec![
            test_measurement(vm_map_measurement_name(5_000), 10),
            test_measurement(otp_map_measurement_name(5_000), 20),
        ];
        assert!(assert_vm_wins_large_map_reference_lane(&winning).is_ok());

        let losing = vec![
            test_measurement(vm_map_measurement_name(5_000), 20),
            test_measurement(otp_map_measurement_name(5_000), 10),
        ];
        let error = assert_vm_wins_large_map_reference_lane(&losing)
            .expect_err("OTP win should fail the large-map reference lane");
        assert!(error.contains("must beat OTP"));
    }

    fn test_measurement(name: &'static str, mean_ns: u128) -> Measurement {
        Measurement {
            name,
            unit: "nanoseconds",
            iterations: 1,
            total_ns: mean_ns,
            min_ns: mean_ns,
            mean_ns,
            p50_ns: mean_ns,
            p95_ns: mean_ns,
            p99_ns: mean_ns,
            max_ns: mean_ns,
            total_us: mean_ns / 1_000,
            min_us: mean_ns / 1_000,
            mean_us: mean_ns / 1_000,
            p50_us: mean_ns / 1_000,
            p95_us: mean_ns / 1_000,
            p99_us: mean_ns / 1_000,
            max_us: mean_ns / 1_000,
        }
    }

    /// Verifies generated OTP map benchmark source uses native maps.
    ///
    /// Inputs:
    /// - One threshold-sized benchmark eval expression.
    ///
    /// Output:
    /// - Test passes when the expression keeps internal timing and correctness
    ///   assertions inside OTP.
    ///
    /// Transformation:
    /// - Avoids shelling out to Erlang during unit tests while still locking
    ///   down the source emitted by the benchmark gate.
    #[test]
    fn otp_map_benchmark_eval_uses_native_map_assertions() {
        let source = otp_map_benchmark_eval(33, 3);

        assert!(source.contains("Size = 33"));
        assert!(source.contains("Iterations = 3"));
        assert!(source.contains("maps:put"));
        assert!(source.contains("maps:get"));
        assert!(source.contains("maps:update"));
        assert!(source.contains("erlang:monotonic_time(nanosecond)"));
        assert!(source.contains("{true, true} -> End - Start"));
    }

    /// Verifies OTP duration parsing accepts only the expected list shape.
    ///
    /// Inputs:
    /// - Stable Erlang list text and malformed text.
    ///
    /// Output:
    /// - Test passes when valid output becomes durations and invalid output is
    ///   rejected.
    ///
    /// Transformation:
    /// - Keeps the OTP comparison parser deterministic for benchmark gates.
    #[test]
    fn otp_map_duration_list_parser_is_strict() {
        let durations =
            parse_erlang_duration_list("[1, 2000, 3000000]").expect("duration list should parse");

        assert_eq!(
            durations,
            vec![
                Duration::from_nanos(1),
                Duration::from_nanos(2_000),
                Duration::from_nanos(3_000_000),
            ]
        );
        assert!(parse_erlang_duration_list("{1,2,3}").is_err());
    }
}
