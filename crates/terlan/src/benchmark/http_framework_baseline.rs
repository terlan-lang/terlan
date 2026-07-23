//! Framework-neutral HTTP client benchmark for the Axum/Tokio control lane.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Clone, Serialize)]
struct Workload {
    warmup_requests: usize,
    measurement_rounds: usize,
    readiness_reactors: usize,
    sequential_requests: usize,
    concurrency: usize,
    requests_per_worker: usize,
    longevity_requests: usize,
    payload_bytes: usize,
}

impl Workload {
    fn from_env() -> Self {
        Self {
            warmup_requests: positive_env("TERLAN_BENCH_HTTP_AOT_WARMUP", 250),
            measurement_rounds: positive_env("TERLAN_BENCH_HTTP_AOT_ROUNDS", 5),
            readiness_reactors: positive_env(
                "TERLAN_BENCH_HTTP_AOT_REACTORS",
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(1),
            ),
            sequential_requests: positive_env("TERLAN_BENCH_HTTP_AOT_ITERATIONS", 500),
            concurrency: positive_env("TERLAN_BENCH_HTTP_AOT_CONCURRENCY", 8),
            requests_per_worker: positive_env("TERLAN_BENCH_HTTP_AOT_REQUESTS_PER_WORKER", 100),
            longevity_requests: positive_env("TERLAN_BENCH_HTTP_AOT_LONGEVITY", 1_000),
            payload_bytes: positive_env("TERLAN_BENCH_HTTP_AOT_PAYLOAD_BYTES", 512),
        }
    }
}

#[derive(Clone, Serialize)]
struct Timing {
    sample_count: usize,
    total_wall_ns: u128,
    throughput_requests_per_second: u128,
    min_ns: u128,
    mean_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

#[derive(Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    implementation: &'static str,
    workload: Workload,
    sequential_rounds: Vec<Timing>,
    pressure_rounds: Vec<Timing>,
    longevity_rounds: Vec<Timing>,
    sequential: Timing,
    pressure: Timing,
    longevity: Timing,
    resident_memory_before_bytes: Option<u64>,
    resident_memory_peak_bytes: Option<u64>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error[http-framework-benchmark]: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let binary = env::var_os("TERLAN_BENCH_HTTP_AXUM_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/release/terlan-axum-baseline"));
    if !binary.is_file() {
        return Err(format!("Axum server `{}` does not exist", binary.display()));
    }
    let output = env::var_os("TERLAN_BENCH_HTTP_AXUM_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/quality/http-axum-performance.json"));
    let workload = Workload::from_env();
    let port = reserve_port()?;
    let mut server = ServerGuard::spawn(&binary, port)?;
    wait_ready(port, workload.payload_bytes)?;

    measure(port, 1, workload.warmup_requests, workload.payload_bytes)?;
    measure(
        port,
        workload.concurrency,
        workload
            .warmup_requests
            .div_ceil(workload.concurrency)
            .max(1),
        workload.payload_bytes,
    )?;

    let memory_before = resident_bytes(server.id());
    let (sequential, sequential_rounds) = rounds(
        port,
        1,
        workload.sequential_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
    )?;
    let (pressure, pressure_rounds) = rounds(
        port,
        workload.concurrency,
        workload.requests_per_worker,
        workload.payload_bytes,
        workload.measurement_rounds,
    )?;
    let memory_pressure = resident_bytes(server.id());
    let (longevity, longevity_rounds) = rounds(
        port,
        1,
        workload.longevity_requests,
        workload.payload_bytes,
        workload.measurement_rounds,
    )?;
    let memory_longevity = resident_bytes(server.id());
    server.stop();

    let report = Report {
        schema: "terlan-http-framework-performance-v1",
        status: "completed",
        implementation: "axum-0.8.9+tokio-1.52.3",
        workload,
        sequential_rounds,
        pressure_rounds,
        longevity_rounds,
        sequential,
        pressure,
        longevity,
        resident_memory_before_bytes: memory_before,
        resident_memory_peak_bytes: [memory_before, memory_pressure, memory_longevity]
            .into_iter()
            .flatten()
            .max(),
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(
        &output,
        serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    println!("[http-axum-performance] wrote {}", output.display());
    Ok(())
}

fn rounds(
    port: u16,
    concurrency: usize,
    requests: usize,
    payload: usize,
    count: usize,
) -> Result<(Timing, Vec<Timing>), String> {
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        samples.push(measure(port, concurrency, requests, payload)?);
    }
    let mut ranking = (0..samples.len()).collect::<Vec<_>>();
    ranking.sort_by_key(|index| samples[*index].throughput_requests_per_second);
    let selected = samples[ranking[ranking.len() / 2]].clone();
    Ok((selected, samples))
}

fn measure(
    port: u16,
    concurrency: usize,
    requests_per_worker: usize,
    payload_bytes: usize,
) -> Result<Timing, String> {
    let started = Instant::now();
    let mut workers = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        workers.push(thread::spawn(move || {
            let mut durations = Vec::with_capacity(requests_per_worker);
            for _ in 0..requests_per_worker {
                let request_started = Instant::now();
                request(port, payload_bytes)?;
                durations.push(request_started.elapsed().as_nanos());
            }
            Ok::<_, String>(durations)
        }));
    }
    let mut durations = Vec::with_capacity(concurrency * requests_per_worker);
    for worker in workers {
        durations.extend(worker.join().map_err(|_| "client worker panicked")??);
    }
    durations.sort_unstable();
    let wall_ns = started.elapsed().as_nanos().max(1);
    let total = durations.iter().sum::<u128>();
    Ok(Timing {
        sample_count: durations.len(),
        total_wall_ns: wall_ns,
        throughput_requests_per_second: durations.len() as u128 * 1_000_000_000 / wall_ns,
        min_ns: durations[0],
        mean_ns: total / durations.len() as u128,
        p50_ns: percentile(&durations, 50),
        p95_ns: percentile(&durations, 95),
        p99_ns: percentile(&durations, 99),
        max_ns: durations[durations.len() - 1],
    })
}

fn request(port: u16, payload_bytes: usize) -> Result<(), String> {
    let payload = "x".repeat(payload_bytes);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| error.to_string())?;
    write!(
        stream,
        "POST /api/bench HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    )
    .map_err(|error| error.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| error.to_string())?;
    let expected = format!("generation-one:{payload}");
    let response = String::from_utf8(response).map_err(|error| error.to_string())?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| "response header terminator missing".to_string())?;
    if !head.starts_with("HTTP/1.1 200") || body != expected {
        return Err("Axum response did not match the benchmark contract".to_string());
    }
    Ok(())
}

fn wait_ready(port: u16, payload: usize) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if request(port, payload).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err("Axum server did not become ready".to_string())
}

fn reserve_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| error.to_string())
}

fn percentile(values: &[u128], percentile: usize) -> u128 {
    values[(values.len() * percentile).div_ceil(100).saturating_sub(1)]
}

fn positive_env(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn resident_bytes(pid: u32) -> Option<u64> {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes * 1024)
}

struct ServerGuard(Child);

impl ServerGuard {
    fn spawn(binary: &Path, port: u16) -> Result<Self, String> {
        Command::new(binary)
            .arg(port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(Self)
            .map_err(|error| error.to_string())
    }

    fn id(&self) -> u32 {
        self.0.id()
    }

    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.stop();
    }
}
