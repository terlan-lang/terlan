use super::*;

pub(super) fn assert_vm_wins_large_map_reference_lane(
    measurements: &[Measurement],
) -> Result<(), String> {
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

pub(super) fn measurement_mean_ns(
    measurements: &[Measurement],
    name: &str,
) -> Result<u128, String> {
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
pub(super) fn map_assertion_name(size: usize) -> &'static str {
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
pub(super) fn measure_vm_actor_runtime_primitives() -> Result<(), String> {
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
pub(super) fn measure_vm_selective_receive_primitives() -> Result<(), String> {
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
pub(super) fn measure_vm_timer_runtime_primitives() -> Result<(), String> {
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
pub(super) struct BenchmarkCommandOutput {
    pub(super) stdout: String,
    pub(super) stderr: String,
}

/// Runs a benchmark command and requires a successful exit status.
pub(super) fn run_required_command(
    program: &Path,
    args: &[&str],
) -> Result<BenchmarkCommandOutput, String> {
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
pub(super) fn require_stdout_contains(
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
pub(super) fn format_command_failure(command: &str, output: &std::process::Output) -> String {
    format!(
        "`{command}` failed with status {}; stdout=`{}` stderr=`{}`",
        output.status,
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

/// Builds a passed assertion record.
pub(super) fn assertion(name: &'static str, detail: impl Into<String>) -> AssertionResult {
    AssertionResult {
        name,
        passed: true,
        detail: detail.into(),
    }
}

/// Returns required VM performance tracks that are not executable yet.
pub(super) fn vm_performance_skipped_tracks() -> Vec<SkippedTrack> {
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
pub(super) fn measure_connect(config: &Config) -> Result<(Pool, Measurement), String> {
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
pub(super) fn prepare_benchmark_table(pool: &Pool) -> Result<(), String> {
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
pub(super) fn measure_repeated(
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
pub(super) fn measure_concurrent_query_one(
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
pub(super) fn percentile(sorted_values: &[u128], percentile_value: usize) -> u128 {
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
pub(super) fn format_postgres_error(error: PostgresError) -> String {
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
pub(super) fn format_http_error(error: http::HttpError) -> String {
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
pub(super) fn format_json_error(error: json::JsonError) -> String {
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
pub(super) fn unix_timestamp_nanos() -> u128 {
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
pub(super) fn redact_postgres_url(url: &str) -> String {
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
#[path = "database_test.rs"]
mod tests;
