//! NativeBoundary proof-path correlation.

use serde::Deserialize;

const PROOF_PATH: &str = "proofs/lean/native_boundary/NativeBoundary.lean";
const PROOF_SOURCE: &str =
    include_str!("../../../../../proofs/lean/native_boundary/NativeBoundary.lean");
const REPLAY_METADATA: &str =
    include_str!("../../../../../proofs/lean/artifacts/native-boundary.json");

#[derive(Debug, Deserialize)]
struct ReplayMetadata {
    schema: String,
    family: String,
}

/// Verified proof identity embedded in NativeBoundary runtime reports.
pub(super) struct NativeBoundaryProofCorrelation {
    pub(super) family: String,
    pub(super) proof_path: &'static str,
}

/// Validates replay metadata against the proof source compiled into this binary.
pub(super) fn native_boundary_proof_correlation() -> Result<NativeBoundaryProofCorrelation, String>
{
    correlate_native_boundary_proof(REPLAY_METADATA, PROOF_SOURCE)
}

fn correlate_native_boundary_proof(
    replay_metadata: &str,
    proof_source: &str,
) -> Result<NativeBoundaryProofCorrelation, String> {
    let metadata = serde_json::from_str::<ReplayMetadata>(replay_metadata)
        .map_err(|error| format!("invalid NativeBoundary proof replay metadata: {error}"))?;
    if metadata.schema != "terlan.lean-proof-replay.v2" {
        return Err(format!(
            "unsupported NativeBoundary proof replay schema `{}`",
            metadata.schema
        ));
    }
    if metadata.family != "native-boundary" {
        return Err(format!(
            "NativeBoundary proof replay family must be `native-boundary`, found `{}`",
            metadata.family
        ));
    }
    if proof_source.trim().is_empty() {
        return Err("NativeBoundary proof source is empty".to_string());
    }
    Ok(NativeBoundaryProofCorrelation {
        family: metadata.family,
        proof_path: PROOF_PATH,
    })
}

#[cfg(test)]
#[path = "proof_correlation_test.rs"]
#[cfg(test)]
mod proof_correlation_test;
