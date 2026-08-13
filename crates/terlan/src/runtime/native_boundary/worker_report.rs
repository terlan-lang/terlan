//! NativeBoundary worker lifecycle report serialization.

use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
};

use serde_json::json;

use crate::terlan_native_boundary::{
    metadata::postgres_worker_manifest,
    proof_correlation::native_boundary_proof_correlation,
    runtime_events::{NativeBoundaryDispatchEvent, NativeBoundaryResourceEvent},
    worker::NativeBoundaryWorkerEvent,
};

const PROOF_FEATURE_CLASS: &str = "native-boundary";
const PROOF_GAP_CATEGORY: &str = "native-boundary contracts";
const PROOF_GAP_SOURCE: &str = "docs/compiler/proof_track/lean_proof_gaps.tsv";
const PROOF_OWNER: &str = "runtime";
const PROOF_STATUS: &str = "current";

/// Bounded worker state and correlated runtime events written to one report.
pub(super) struct NativeBoundaryWorkerReport<'a> {
    pub(super) credit_limit: u64,
    pub(super) reserved_credits: u64,
    pub(super) available_credits: u64,
    pub(super) last_started_request_id: Option<u64>,
    pub(super) event_history_limit: usize,
    pub(super) events: &'a VecDeque<NativeBoundaryWorkerEvent>,
    pub(super) resource_events: &'a [&'a NativeBoundaryResourceEvent],
    pub(super) dispatch_events: &'a [&'a NativeBoundaryDispatchEvent],
}

/// Writes one bounded worker lifecycle snapshot as deterministic JSON.
pub(super) fn write_worker_report(
    path: &Path,
    report: NativeBoundaryWorkerReport<'_>,
) -> Result<(), String> {
    let NativeBoundaryWorkerReport {
        credit_limit,
        reserved_credits,
        available_credits,
        last_started_request_id,
        event_history_limit,
        events,
        resource_events,
        dispatch_events,
    } = report;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create NativeBoundary report directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let mut worker_class_usage = BTreeMap::<&str, u64>::new();
    for event in dispatch_events {
        let worker_class = event.worker_class.as_deref().unwrap_or("unclassified");
        *worker_class_usage.entry(worker_class).or_default() += 1;
    }
    let correlated_dispatches = dispatch_events
        .iter()
        .filter(|event| event.worker_class.is_some())
        .count();
    let proof = native_boundary_proof_correlation()?;
    let report = json!({
        "schema": "terlan-vm-native-boundary-report-v1",
        "creditLimit": credit_limit,
        "reservedCredits": reserved_credits,
        "availableCredits": available_credits,
        "lastStartedRequestId": last_started_request_id,
        "eventHistoryLimit": event_history_limit,
        "events": events,
        "resourceEvents": resource_events,
        "dispatchEvents": dispatch_events,
        "workerClassUsage": worker_class_usage,
        "proofManifestCorrelation": {
            "featureClass": PROOF_FEATURE_CLASS,
            "status": PROOF_STATUS,
            "proofFamily": proof.family,
            "proofPath": proof.proof_path,
            "proofDigest": proof.proof_digest,
            "gapCategory": PROOF_GAP_CATEGORY,
            "gapSource": PROOF_GAP_SOURCE,
            "bridgeStatus": "runtime-sources-fingerprinted; full Aeneas/Rust refinement pending",
            "owner": PROOF_OWNER,
            "plannedGate": "lean-proof-native-boundary-check",
            "runtimeManifest": postgres_worker_manifest().adapter,
            "runtimeManifestExports": postgres_worker_manifest().exports.len(),
            "correlatedDispatches": correlated_dispatches,
            "unmanifestedDispatches": dispatch_events.len() - correlated_dispatches,
        },
    });
    let json = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("failed to serialize NativeBoundary report: {error}"))?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|error| format!("failed to write NativeBoundary report: {error}"))
}
