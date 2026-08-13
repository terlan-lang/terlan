use super::*;

/// Rejects source execution until the standalone frontend owns an AOT compile
/// path. Runtime CoreIR interpretation is intentionally unavailable.
pub(super) fn run_source_file(
    source: &Path,
    entry: &str,
    result_mode: RunResultMode,
    output: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let _ = (source, entry, result_mode, output);
    Err(aot_cutover_error("source execution"))
}

/// Selects how the standalone VM exposes the value returned by an entrypoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RunResultMode {
    /// Execute an application entrypoint without exposing its return value.
    Discard,
    /// Require the entrypoint to return `true`, as used by compiled tests.
    Test,
    /// Propagate a script's final non-`Unit` value to the process caller.
    Script,
}

pub(super) fn benchmark_http_handler(iterations: usize) -> Result<String, String> {
    let _ = iterations;
    Err(aot_cutover_error("HTTP handler benchmark"))
}

pub(super) fn benchmark_http_stack(iterations: usize) -> Result<String, String> {
    let _ = iterations;
    Err(aot_cutover_error("HTTP stack benchmark"))
}

pub(super) fn benchmark_http_vm_stream(
    iterations: usize,
    payload_bytes: usize,
    requests_per_connection: usize,
    request_mix: BenchmarkHttpRequestMix,
) -> Result<String, String> {
    let _ = (
        iterations,
        payload_bytes,
        requests_per_connection,
        request_mix,
    );
    Err(aot_cutover_error("HTTP VM-stream benchmark"))
}

pub(super) struct HttpSocketBenchmarkOptions {
    pub(super) iterations: usize,
    pub(super) concurrency: usize,
    pub(super) queue_capacity: usize,
    pub(super) warmup_requests: usize,
    pub(super) handler_delay_ms: u64,
    pub(super) requests_per_connection: usize,
    pub(super) payload_bytes: usize,
    pub(super) request_mix: BenchmarkHttpRequestMix,
}

pub(super) fn benchmark_http_socket(options: HttpSocketBenchmarkOptions) -> Result<String, String> {
    let HttpSocketBenchmarkOptions {
        iterations,
        concurrency,
        queue_capacity,
        warmup_requests,
        handler_delay_ms,
        requests_per_connection,
        payload_bytes,
        request_mix,
    } = options;
    let _ = (
        iterations,
        concurrency,
        queue_capacity,
        warmup_requests,
        handler_delay_ms,
        requests_per_connection,
        payload_bytes,
        request_mix,
    );
    Err(aot_cutover_error("HTTP socket benchmark"))
}

pub(super) fn aot_cutover_error(surface: &str) -> String {
    format!(
        "error[vm.aot_required]: {surface} has no managed AOT implementation; runtime CoreIR interpretation has been removed"
    )
}

pub(super) fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(super) fn evaluate_test_result(value: ReplValue) -> Result<(), String> {
    match value {
        ReplValue::Bool(true) => Ok(()),
        ReplValue::Bool(false) => Err("terlan-vm test-eval failed: returned false".to_string()),
        other => Err(format!(
            "terlan-vm test-eval expects Bool return, found {}",
            other.render()
        )),
    }
}

#[cfg(test)]
#[path = "execution_test.rs"]
mod tests;

/// Converts a compiled script result into the caller-visible text contract.
pub(super) fn evaluate_script_result(value: ReplValue) -> Option<String> {
    match value {
        ReplValue::Unit => None,
        other => Some(other.render()),
    }
}

pub(super) fn print_usage() {
    println!("terlan-vm run <file.tvm> [--entry <function>] [--test|--test-eval|--script-eval]");
    println!("terlan-vm load <file.tvm>");
    println!("terlan-vm package-image-metadata <file.tvm> --entry <function> [--package-path <relative.tvm>]");
    println!("terlan-vm validate-package <archive-or-install-root>");
    println!("terlan-vm support-bundle <file.tvm>");
    println!("source execution and HTTP benchmarks require managed AOT support");
    println!("terlan-vm inspect processes|supervisors|resources|process <pid>");
    println!(
        "terlan-vm benchmark-in-memory-framing [--iterations <count>] [--payload-bytes <count>] [--workload roundtrip|truncated|malformed-length|invalid-utf8]"
    );
    println!("terlan-vm version");
}
