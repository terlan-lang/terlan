//! Content-addressed NativeBoundary proof correlation.

use serde::Deserialize;
use sha2::{Digest, Sha256};

const PROOF_PATH: &str = "proofs/lean/Terlan/Runtime/NativeBoundary.lean";
const PROOF_SOURCE: &str =
    include_str!("../../../../../proofs/lean/Terlan/Runtime/NativeBoundary.lean");
const REPLAY_METADATA: &str =
    include_str!("../../../../../proofs/lean/artifacts/native-boundary.json");

#[derive(Debug, Deserialize)]
struct ReplayMetadata {
    schema: String,
    family: String,
    source_digest: String,
}

/// Verified proof identity embedded in NativeBoundary runtime reports.
pub(super) struct NativeBoundaryProofCorrelation {
    pub(super) family: String,
    pub(super) proof_digest: String,
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
    if metadata.schema != "terlan.lean-proof-replay.v1" {
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
    let digest = sha256_text(proof_source);
    if metadata.source_digest != digest {
        return Err(format!(
            "NativeBoundary proof source digest drift: metadata has `{}`, source has `{digest}`",
            metadata.source_digest
        ));
    }
    Ok(NativeBoundaryProofCorrelation {
        family: metadata.family,
        proof_digest: digest,
        proof_path: PROOF_PATH,
    })
}

fn sha256_text(text: &str) -> String {
    let hexadecimal = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hexadecimal}")
}

#[cfg(test)]
#[path = "proof_correlation_test.rs"]
mod proof_correlation_test;
