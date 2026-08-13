//! Optional HTTP benchmark hardware-counter probes.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::{run_command_with_timeout, HardwareCounterEvidence};

fn probe_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "collect HTTP benchmark hardware counters",
        rendered,
    )
}

pub(crate) fn run_hardware_counter_probe(
    pid: u32,
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
) -> Result<HardwareCounterEvidence, BoundaryError> {
    let duration_seconds = env::var("TERLAN_BENCH_HTTP_PERF_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if duration_seconds == 0 {
        return Ok(HardwareCounterEvidence {
            status: "disabled".to_string(),
            duration_seconds,
            counters: BTreeMap::new(),
            syscall_counter_status: "disabled".to_string(),
            diagnostic: "set TERLAN_BENCH_HTTP_PERF_SECONDS to enable".to_string(),
        });
    }
    let perf = match Command::new("perf")
        .args([
            "stat",
            "-x,",
            "-e",
            "cycles,instructions,cache-misses,context-switches",
            "-p",
            &pid.to_string(),
            "--",
            "sleep",
            &duration_seconds.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Ok(HardwareCounterEvidence {
                status: "unavailable".to_string(),
                duration_seconds,
                counters: BTreeMap::new(),
                syscall_counter_status: "unavailable-without-intrusive-ptrace".to_string(),
                diagnostic: error.to_string(),
            });
        }
    };
    thread::sleep(Duration::from_millis(100));
    run_wrk_for_duration(port, concurrency, payload_bytes, duration_seconds)?;
    let output = perf
        .wait_with_output()
        .map_err(|error| probe_error(format!("cannot collect perf counters: {error}")))?;
    let diagnostic = String::from_utf8_lossy(&output.stderr).to_string();
    let counters = diagnostic
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(',');
            let value = fields.next()?.trim().replace(',', "");
            let event = fields.nth(1)?.trim();
            value
                .parse::<f64>()
                .ok()
                .map(|value| (event.to_string(), value))
        })
        .collect::<BTreeMap<_, _>>();
    Ok(HardwareCounterEvidence {
        status: if output.status.success() && !counters.is_empty() {
            "measured".to_string()
        } else {
            "unavailable".to_string()
        },
        duration_seconds,
        counters,
        syscall_counter_status: "unavailable-without-intrusive-ptrace".to_string(),
        diagnostic,
    })
}

fn run_wrk_for_duration(
    port: u16,
    concurrency: usize,
    payload_bytes: usize,
    duration_seconds: u64,
) -> Result<(), BoundaryError> {
    let script = write_wrk_script(payload_bytes)?;
    let output = run_command_with_timeout(
        Command::new("wrk")
            .arg(format!("-t{}", concurrency.clamp(1, 8)))
            .arg(format!("-c{}", concurrency.max(1)))
            .arg(format!("-d{duration_seconds}s"))
            .arg("-s")
            .arg(&script)
            .arg(format!("http://127.0.0.1:{port}/api/bench")),
        "wrk perf probe",
        Duration::from_secs(duration_seconds.saturating_add(30)),
    );
    let _ = fs::remove_file(&script);
    let output = output.map_err(probe_error)?;
    if !output.status.success() {
        return Err(probe_error(format!(
            "wrk perf probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn write_wrk_script(payload_bytes: usize) -> Result<std::path::PathBuf, BoundaryError> {
    let script = env::temp_dir().join(format!(
        "terlan-http-wrk-{}-{}.lua",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let body = "x".repeat(payload_bytes);
    fs::write(
        &script,
        format!(
            "wrk.method = \"POST\"\nwrk.body = {:?}\nwrk.headers[\"Content-Type\"] = \"text/plain\"\n",
            body
        ),
    )
    .map_err(|error| probe_error(format!("cannot write wrk script: {error}")))?;
    Ok(script)
}
