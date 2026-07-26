//! Shared loopback HTTP client used by every framework comparison lane.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProcessMemorySnapshot {
    pub(crate) phase: String,
    pub(crate) rss_bytes: Option<u64>,
    pub(crate) virtual_bytes: Option<u64>,
    pub(crate) data_bytes: Option<u64>,
    pub(crate) stack_bytes: Option<u64>,
    pub(crate) thread_count: Option<u64>,
    pub(crate) proportional_set_bytes: Option<u64>,
    pub(crate) anonymous_bytes: Option<u64>,
    pub(crate) shared_clean_bytes: Option<u64>,
    pub(crate) shared_dirty_bytes: Option<u64>,
    pub(crate) private_clean_bytes: Option<u64>,
    pub(crate) private_dirty_bytes: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MemoryAttributionEvidence {
    pub(crate) idle_rss_bytes: Option<u64>,
    pub(crate) idle_pss_bytes: Option<u64>,
    pub(crate) warmed_rss_bytes: Option<u64>,
    pub(crate) peak_rss_bytes: Option<u64>,
    pub(crate) retained_rss_delta_bytes: Option<i64>,
    pub(crate) retained_pss_delta_bytes: Option<i64>,
    pub(crate) peak_rss_bytes_per_completed_request: Option<f64>,
    pub(crate) thread_count_peak: Option<u64>,
    pub(crate) attribution: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProcessEfficiencySnapshot {
    pub(crate) user_ticks: u64,
    pub(crate) system_ticks: u64,
    pub(crate) voluntary_context_switches: u64,
    pub(crate) involuntary_context_switches: u64,
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProcessEfficiencyEvidence {
    pub(crate) before: ProcessEfficiencySnapshot,
    pub(crate) after: ProcessEfficiencySnapshot,
    pub(crate) completed_requests: usize,
    pub(crate) cpu_ticks_per_request: f64,
    pub(crate) context_switches_per_request: f64,
    pub(crate) read_bytes_per_request: f64,
    pub(crate) write_bytes_per_request: f64,
}

pub(crate) struct HttpClientMeasurement {
    pub(crate) durations: Vec<Duration>,
    pub(crate) wall: Duration,
}

pub(crate) fn process_efficiency_snapshot(pid: u32) -> Option<ProcessEfficiencySnapshot> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let io = fs::read_to_string(format!("/proc/{pid}/io")).ok()?;
    let (_, stat_fields) = stat.rsplit_once(") ")?;
    let fields = stat_fields.split_whitespace().collect::<Vec<_>>();
    Some(ProcessEfficiencySnapshot {
        user_ticks: fields.get(11)?.parse().ok()?,
        system_ticks: fields.get(12)?.parse().ok()?,
        voluntary_context_switches: proc_scalar(&status, "voluntary_ctxt_switches:")?,
        involuntary_context_switches: proc_scalar(&status, "nonvoluntary_ctxt_switches:")?,
        read_bytes: proc_scalar(&io, "read_bytes:")?,
        write_bytes: proc_scalar(&io, "write_bytes:")?,
    })
}

pub(crate) fn efficiency_evidence(
    before: ProcessEfficiencySnapshot,
    after: ProcessEfficiencySnapshot,
    completed_requests: usize,
) -> ProcessEfficiencyEvidence {
    let requests = completed_requests.max(1) as f64;
    let cpu_ticks = after
        .user_ticks
        .saturating_add(after.system_ticks)
        .saturating_sub(before.user_ticks.saturating_add(before.system_ticks));
    let context_switches = after
        .voluntary_context_switches
        .saturating_add(after.involuntary_context_switches)
        .saturating_sub(
            before
                .voluntary_context_switches
                .saturating_add(before.involuntary_context_switches),
        );
    ProcessEfficiencyEvidence {
        before: before.clone(),
        after: after.clone(),
        completed_requests,
        cpu_ticks_per_request: cpu_ticks as f64 / requests,
        context_switches_per_request: context_switches as f64 / requests,
        read_bytes_per_request: after.read_bytes.saturating_sub(before.read_bytes) as f64
            / requests,
        write_bytes_per_request: after.write_bytes.saturating_sub(before.write_bytes) as f64
            / requests,
    }
}

/// Reads resident-set bytes for a child process through procfs or `ps`.
pub(crate) fn resident_bytes(pid: u32) -> Option<u64> {
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

/// Captures attributed process memory and thread state from Linux procfs.
pub(crate) fn process_memory_snapshot(pid: u32, phase: &str) -> Option<ProcessMemorySnapshot> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let rollup = fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok();
    let status_kib = |name: &str| proc_kib(&status, name);
    let rollup_kib = |name: &str| rollup.as_deref().and_then(|value| proc_kib(value, name));
    Some(ProcessMemorySnapshot {
        phase: phase.to_string(),
        rss_bytes: status_kib("VmRSS:").and_then(kib_to_bytes),
        virtual_bytes: status_kib("VmSize:").and_then(kib_to_bytes),
        data_bytes: status_kib("VmData:").and_then(kib_to_bytes),
        stack_bytes: status_kib("VmStk:").and_then(kib_to_bytes),
        thread_count: proc_scalar(&status, "Threads:"),
        proportional_set_bytes: rollup_kib("Pss:").and_then(kib_to_bytes),
        anonymous_bytes: rollup_kib("Anonymous:").and_then(kib_to_bytes),
        shared_clean_bytes: rollup_kib("Shared_Clean:").and_then(kib_to_bytes),
        shared_dirty_bytes: rollup_kib("Shared_Dirty:").and_then(kib_to_bytes),
        private_clean_bytes: rollup_kib("Private_Clean:").and_then(kib_to_bytes),
        private_dirty_bytes: rollup_kib("Private_Dirty:").and_then(kib_to_bytes),
    })
}

pub(crate) fn memory_attribution(
    snapshots: &[ProcessMemorySnapshot],
    completed_requests: usize,
) -> MemoryAttributionEvidence {
    let idle = snapshots
        .iter()
        .find(|snapshot| snapshot.phase == "idle_after_readiness");
    let warmed = snapshots
        .iter()
        .find(|snapshot| snapshot.phase == "after_warmup");
    let final_snapshot = snapshots.last();
    let peak_rss = snapshots
        .iter()
        .filter_map(|snapshot| snapshot.rss_bytes)
        .max();
    MemoryAttributionEvidence {
        idle_rss_bytes: idle.and_then(|snapshot| snapshot.rss_bytes),
        idle_pss_bytes: idle.and_then(|snapshot| snapshot.proportional_set_bytes),
        warmed_rss_bytes: warmed.and_then(|snapshot| snapshot.rss_bytes),
        peak_rss_bytes: peak_rss,
        retained_rss_delta_bytes: signed_delta(
            idle.and_then(|snapshot| snapshot.rss_bytes),
            final_snapshot.and_then(|snapshot| snapshot.rss_bytes),
        ),
        retained_pss_delta_bytes: signed_delta(
            idle.and_then(|snapshot| snapshot.proportional_set_bytes),
            final_snapshot.and_then(|snapshot| snapshot.proportional_set_bytes),
        ),
        peak_rss_bytes_per_completed_request: peak_rss
            .map(|value| value as f64 / completed_requests.max(1) as f64),
        thread_count_peak: snapshots
            .iter()
            .filter_map(|snapshot| snapshot.thread_count)
            .max(),
        attribution: "Linux /proc status and smaps_rollup; idle, warmed, peak, and retained"
            .to_string(),
    }
}

fn signed_delta(before: Option<u64>, after: Option<u64>) -> Option<i64> {
    before.zip(after).map(|(before, after)| {
        i128::from(after)
            .saturating_sub(i128::from(before))
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    })
}

fn proc_kib(contents: &str, name: &str) -> Option<u64> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(name))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn proc_scalar(contents: &str, name: &str) -> Option<u64> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix(name))?
        .trim()
        .parse()
        .ok()
}

fn kib_to_bytes(kib: u64) -> Option<u64> {
    kib.checked_mul(1024)
}

pub(crate) fn measure(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
    generation: &'static str,
) -> Result<HttpClientMeasurement, String> {
    let mut workers = Vec::with_capacity(concurrency);
    let barrier = Arc::new(Barrier::new(concurrency.saturating_add(1)));
    for _ in 0..concurrency {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let mut durations = Vec::with_capacity(requests_per_worker);
            let payload = "x".repeat(payload_bytes);
            barrier.wait();
            for _ in 0..requests_per_worker {
                let request_started = Instant::now();
                request_with_payload(port, &payload, generation)?;
                durations.push(request_started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    barrier.wait();
    let started = Instant::now();
    let mut durations = Vec::with_capacity(concurrency * requests_per_worker);
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP benchmark worker panicked".to_string())??,
        );
    }
    Ok(HttpClientMeasurement {
        durations,
        wall: started.elapsed(),
    })
}

/// Measures close-after-response traffic for at least one fixed duration.
pub(crate) fn measure_for_duration(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
    generation: &'static str,
) -> Result<HttpClientMeasurement, String> {
    measure_duration_inner(
        port,
        concurrency,
        duration,
        payload_bytes,
        generation,
        false,
    )
}

/// Measures persistent HTTP/1.1 traffic with one connection per client worker.
pub(crate) fn measure_keep_alive(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
    generation: &'static str,
) -> Result<HttpClientMeasurement, String> {
    let mut workers = Vec::with_capacity(concurrency);
    let barrier = Arc::new(Barrier::new(concurrency.saturating_add(1)));
    for _ in 0..concurrency {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let payload = "x".repeat(payload_bytes);
            let mut stream = configured_stream(port)?;
            let mut response = Vec::with_capacity(payload_bytes.saturating_add(512));
            let mut durations = Vec::with_capacity(requests_per_worker);
            barrier.wait();
            for _ in 0..requests_per_worker {
                let request_started = Instant::now();
                write_request(&mut stream, &payload, false)?;
                read_response(&mut stream, &mut response, generation, &payload)?;
                durations.push(request_started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    barrier.wait();
    let started = Instant::now();
    let mut durations = Vec::with_capacity(concurrency * requests_per_worker);
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP keep-alive benchmark worker panicked".to_string())??,
        );
    }
    Ok(HttpClientMeasurement {
        durations,
        wall: started.elapsed(),
    })
}

/// Measures persistent HTTP/1.1 traffic for at least one fixed duration.
pub(crate) fn measure_keep_alive_for_duration(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
    generation: &'static str,
) -> Result<HttpClientMeasurement, String> {
    measure_duration_inner(port, concurrency, duration, payload_bytes, generation, true)
}

/// Measures a request shape that stresses header parsing or slow readers.
pub(crate) fn measure_shaped(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
    generation: &'static str,
    extra_header_count: usize,
    response_read_delay: Duration,
) -> Result<HttpClientMeasurement, String> {
    let barrier = Arc::new(Barrier::new(concurrency.saturating_add(1)));
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let payload = "x".repeat(payload_bytes);
            let extra_headers = (0..extra_header_count)
                .map(|index| format!("X-Terlan-Benchmark-{index}: value-{index}\r\n"))
                .collect::<String>();
            let mut durations = Vec::with_capacity(requests_per_worker);
            barrier.wait();
            for _ in 0..requests_per_worker {
                let started = Instant::now();
                let mut stream = configured_stream(port)?;
                write_shaped_request(&mut stream, &payload, true, &extra_headers)?;
                if !response_read_delay.is_zero() {
                    thread::sleep(response_read_delay);
                }
                let mut bytes = Vec::with_capacity(payload.len().saturating_add(512));
                stream
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("HTTP shaped response read failed: {error}"))?;
                validate_response(&bytes, generation, &payload)?;
                durations.push(started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    barrier.wait();
    let started = Instant::now();
    let mut durations = Vec::with_capacity(concurrency.saturating_mul(requests_per_worker));
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP shaped benchmark worker panicked".to_string())??,
        );
    }
    Ok(HttpClientMeasurement {
        durations,
        wall: started.elapsed(),
    })
}

/// Measures a shaped request stream for a stable wall-clock duration.
pub(crate) fn measure_shaped_for_duration(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
    generation: &'static str,
    extra_header_count: usize,
    response_read_delay: Duration,
) -> Result<HttpClientMeasurement, String> {
    let barrier = Arc::new(Barrier::new(concurrency.saturating_add(1)));
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let payload = "x".repeat(payload_bytes);
            let extra_headers = (0..extra_header_count)
                .map(|index| format!("X-Terlan-Benchmark-{index}: value-{index}\r\n"))
                .collect::<String>();
            let mut durations = Vec::new();
            barrier.wait();
            let deadline = Instant::now() + duration;
            while Instant::now() < deadline {
                let started = Instant::now();
                let mut stream = configured_stream(port)?;
                write_shaped_request(&mut stream, &payload, true, &extra_headers)?;
                if !response_read_delay.is_zero() {
                    thread::sleep(response_read_delay);
                }
                let mut bytes = Vec::with_capacity(payload.len().saturating_add(512));
                stream
                    .read_to_end(&mut bytes)
                    .map_err(|error| format!("HTTP shaped response read failed: {error}"))?;
                validate_response(&bytes, generation, &payload)?;
                durations.push(started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    barrier.wait();
    let started = Instant::now();
    let mut durations = Vec::new();
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP shaped duration worker panicked".to_string())??,
        );
    }
    Ok(HttpClientMeasurement {
        durations,
        wall: started.elapsed(),
    })
}

fn measure_duration_inner(
    port: u16,
    concurrency: usize,
    duration: Duration,
    payload_bytes: usize,
    generation: &'static str,
    keep_alive: bool,
) -> Result<HttpClientMeasurement, String> {
    let barrier = Arc::new(Barrier::new(concurrency.saturating_add(1)));
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let barrier = Arc::clone(&barrier);
        workers.push(thread::spawn(move || {
            let payload = "x".repeat(payload_bytes);
            let mut durations = Vec::new();
            let mut stream = keep_alive.then(|| configured_stream(port)).transpose()?;
            let mut response = Vec::with_capacity(payload_bytes.saturating_add(512));
            barrier.wait();
            let deadline = Instant::now() + duration;
            while Instant::now() < deadline {
                let request_started = Instant::now();
                if let Some(stream) = stream.as_mut() {
                    write_request(stream, &payload, false)?;
                    read_response(stream, &mut response, generation, &payload)?;
                } else {
                    request_with_payload(port, &payload, generation)?;
                }
                durations.push(request_started.elapsed());
            }
            Ok::<_, String>(durations)
        }));
    }
    barrier.wait();
    let started = Instant::now();
    let mut durations = Vec::new();
    for worker in workers {
        durations.extend(
            worker
                .join()
                .map_err(|_| "HTTP duration benchmark worker panicked".to_string())??,
        );
    }
    Ok(HttpClientMeasurement {
        durations,
        wall: started.elapsed(),
    })
}

pub(crate) fn request(port: u16, payload_bytes: usize, generation: &str) -> Result<(), String> {
    let payload = "x".repeat(payload_bytes);
    request_with_payload(port, &payload, generation)
}

fn request_with_payload(port: u16, payload: &str, generation: &str) -> Result<(), String> {
    let mut stream = configured_stream(port)?;
    write_request(&mut stream, payload, true)?;
    let mut bytes = Vec::with_capacity(payload.len().saturating_add(512));
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| format!("HTTP benchmark response read failed: {error}"))?;
    validate_response(&bytes, generation, payload)
}

fn configured_stream(port: u16) -> Result<TcpStream, String> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .map_err(|error| format!("HTTP benchmark connect failed: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP benchmark read timeout setup failed: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("HTTP benchmark write timeout setup failed: {error}"))?;
    stream
        .set_nodelay(true)
        .map_err(|error| format!("HTTP benchmark TCP_NODELAY setup failed: {error}"))?;
    Ok(stream)
}

fn write_request(stream: &mut TcpStream, payload: &str, close: bool) -> Result<(), String> {
    write_shaped_request(stream, payload, close, "")
}

fn write_shaped_request(
    stream: &mut TcpStream,
    payload: &str,
    close: bool,
    extra_headers: &str,
) -> Result<(), String> {
    write!(
        stream,
        "POST /api/bench HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: {}\r\n{}\r\n{}",
        payload.len(),
        if close { "close" } else { "keep-alive" },
        extra_headers,
        payload
    )
    .map_err(|error| format!("HTTP benchmark request write failed: {error}"))
}

fn read_response(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
    generation: &str,
    payload: &str,
) -> Result<(), String> {
    bytes.clear();
    let header_end = loop {
        if let Some(index) = find_header_end(bytes) {
            break index;
        }
        read_more(stream, bytes)?;
    };
    let head = std::str::from_utf8(&bytes[..header_end])
        .map_err(|error| format!("HTTP benchmark response head was not UTF-8: {error}"))?;
    let content_length = content_length(head)?;
    let response_end = header_end
        .checked_add(4)
        .and_then(|body| body.checked_add(content_length))
        .ok_or_else(|| "HTTP benchmark response length overflow".to_string())?;
    while bytes.len() < response_end {
        read_more(stream, bytes)?;
    }
    if bytes.len() != response_end {
        return Err("HTTP benchmark received pipelined bytes unexpectedly".to_string());
    }
    validate_response(bytes, generation, payload)
}

fn read_more(stream: &mut TcpStream, bytes: &mut Vec<u8>) -> Result<(), String> {
    let mut chunk = [0_u8; 8192];
    let count = stream
        .read(&mut chunk)
        .map_err(|error| format!("HTTP benchmark response read failed: {error}"))?;
    if count == 0 {
        return Err("HTTP benchmark connection closed before its response completed".to_string());
    }
    bytes.extend_from_slice(&chunk[..count]);
    Ok(())
}

fn validate_response(bytes: &[u8], generation: &str, payload: &str) -> Result<(), String> {
    let response = String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("HTTP benchmark response was not UTF-8: {error}"))?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "HTTP benchmark response lacked a header terminator".to_string())?;
    if !head.lines().next().unwrap_or_default().contains(" 200 ") {
        return Err(format!("HTTP benchmark returned non-200 response `{head}`"));
    }
    require_response_headers(head)?;
    if body != format!("{generation}:{payload}") {
        return Err(format!("unexpected benchmark response body `{body}`"));
    }
    Ok(())
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(head: &str) -> Result<usize, String> {
    head.lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| "HTTP keep-alive benchmark requires Content-Length".to_string())
}

fn require_response_headers(head: &str) -> Result<(), String> {
    for expected in [
        "content-type: text/plain; charset=utf-8",
        "cache-control: no-cache",
        "x-content-type-options: nosniff",
    ] {
        if !head.lines().any(|line| line.eq_ignore_ascii_case(expected)) {
            return Err(format!(
                "HTTP benchmark response lacked required header `{expected}`"
            ));
        }
    }
    Ok(())
}
