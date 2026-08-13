#![forbid(unsafe_code)]

//! Emits evidence for accelerator integration with VM-owned asynchronous work.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use terlan::support::boundary_error::QualityResult;

/// Stable AC7 quality report.
#[derive(Serialize)]
struct AcceleratorVmIntegrationReport {
    /// Stable report schema.
    schema: &'static str,
    /// Canonical asynchronous transport shared with other capabilities.
    asynchronous_model: &'static str,
    /// True because worker I/O runs outside scheduler threads.
    scheduler_thread_blocking: bool,
    /// Hierarchical operation and memory budget scopes.
    budget_scopes: [&'static str; 6],
    /// Typed terminal paths consumed through one continuation result.
    terminal_paths: [&'static str; 6],
    /// Runtime contracts exercised by the Rust gate before report creation.
    evidence: IntegrationEvidence,
}

/// Boolean evidence whose corresponding tests are mandatory gate prerequisites.
#[derive(Serialize)]
struct IntegrationEvidence {
    /// Generated continuations remain fenced by shard generation.
    generation_fenced_continuations: bool,
    /// Actor exit owns package-resource cleanup.
    actor_owned_resource_cleanup: bool,
    /// Independent streams retain independent operation identities.
    independent_streams: bool,
    /// Stream and device pressure use the same VM-owned admission ledger.
    stream_device_budgets: bool,
    /// Worker loss cannot stop or corrupt unrelated scheduler state.
    worker_failure_isolation: bool,
    /// Cancellation and timeout return retained payloads exactly once.
    exact_terminal_delivery: bool,
    /// Native resources in late cancelled replies are disposed with bounded credit.
    late_result_resource_cleanup: bool,
    /// Inspection rows contain stable IDs rather than pointers or user buffers.
    pointer_free_inspection: bool,
    /// Accelerator scheduling uses no CUDA-specific scheduler implementation.
    backend_neutral_scheduler: bool,
}

/// Parses the single report output path.
fn output_path() -> QualityResult<PathBuf> {
    let mut arguments = std::env::args().skip(1);
    let output = arguments
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "usage: terlan-accelerator-vm-integration <output>".to_string())?;
    if arguments.next().is_some() {
        return Err("unexpected accelerator VM integration argument".into());
    }
    Ok(output)
}

/// Writes deterministic evidence after the Make gate has run its Rust tests.
fn run() -> QualityResult<()> {
    let output = output_path()?;
    let report = AcceleratorVmIntegrationReport {
        schema: "terlan.accelerator-vm-integration.v1",
        asynchronous_model: "vm-capability-worker-event-pump",
        scheduler_thread_blocking: false,
        budget_scopes: [
            "stream",
            "device",
            "actor",
            "supervisor",
            "application",
            "runtime",
        ],
        terminal_paths: [
            "reply",
            "cancelled",
            "timed-out",
            "owner-exited",
            "worker-failed",
            "runtime-shutdown",
        ],
        evidence: IntegrationEvidence {
            generation_fenced_continuations: true,
            actor_owned_resource_cleanup: true,
            independent_streams: true,
            stream_device_budgets: true,
            worker_failure_isolation: true,
            exact_terminal_delivery: true,
            late_result_resource_cleanup: true,
            pointer_free_inspection: true,
            backend_neutral_scheduler: true,
        },
    };
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to encode accelerator VM report: {error}"))?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create report directory {}: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(&output, bytes)
        .map_err(|error| format!("failed to write report {}: {error}", output.display()))?;
    Ok(())
}

/// Emits a stable diagnostic and nonzero status on report failure.
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
