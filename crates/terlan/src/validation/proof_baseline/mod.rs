/// Static CoreIR contract expectations for a gate-backed LP8 compiler fixture.
///
/// Inputs:
/// - `module_name`: phase-contract fixture module name used to locate the
///   source fixture.
/// - `required_snippets`: CoreIR contract fragments that must appear in the
///   fixture's lowered contract text.
///
/// Output:
/// - Immutable record consumed by formal CLI tests and future proof-export
///   preflight checks.
///
/// Transformation:
/// - Stores expected proof evidence without reading files or executing compiler
///   phases.
pub(crate) struct ContractBaseline {
    pub(crate) module_name: &'static str,
    pub(crate) required_snippets: &'static [&'static str],
}

/// Static phase-manifest counter expectations for a gate-backed LP8 fixture.
///
/// Inputs:
/// - `module_name`: phase-contract fixture module name used to locate the
///   source fixture.
/// - `counts`: expected numeric `core_proof_coverage` counters emitted by
///   `terlc check --emit-phase-manifest`.
///
/// Output:
/// - Immutable record consumed by formal CLI tests and future proof-export
///   preflight checks.
///
/// Transformation:
/// - Stores expected manifest proof counters without serializing or decoding
///   JSON.
pub(crate) struct ManifestBaseline {
    pub(crate) module_name: &'static str,
    pub(crate) counts: &'static [ManifestCount],
}

/// Expected numeric `core_proof_coverage` field for one manifest baseline.
///
/// Inputs:
/// - `field`: JSON field name under `core_proof_coverage`.
/// - `expected`: expected unsigned integer value for that field.
///
/// Output:
/// - Immutable field/value pair for manifest validation tests.
///
/// Transformation:
/// - Names one expected counter without reading the manifest.
pub(crate) struct ManifestCount {
    pub(crate) field: &'static str,
    pub(crate) expected: u64,
}

mod contracts;
mod manifests;

pub(crate) use contracts::{contract_baselines, next_lean_model_candidate_baselines};
pub(crate) use manifests::{
    manifest_baselines, next_lean_model_candidate_manifest_baselines,
    RESOLVED_CONSTRUCTOR_COUNTER_FIELDS, UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS,
};

/// Validates CoreIR contract text against one gate-backed baseline.
///
/// Inputs:
/// - `baseline`: static fixture contract baseline.
/// - `contract_text`: actual CoreIR contract text emitted by compiler lowering.
///
/// Output:
/// - `Ok(())` when every required snippet is present.
/// - `Err(String)` naming the missing snippet and fixture when validation
///   fails.
///
/// Transformation:
/// - Scans the actual contract text for each static required snippet without
///   mutating inputs or reading additional files.
pub(crate) fn validate_contract_baseline(
    baseline: &ContractBaseline,
    contract_text: &str,
) -> Result<(), String> {
    for expected in baseline.required_snippets {
        if !contract_text.contains(expected) {
            return Err(format!(
                "CoreIR contract for {} did not contain {expected:?}",
                baseline.module_name
            ));
        }
    }
    Ok(())
}

/// Validates manifest proof counters against one gate-backed baseline.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when every required counter is present and equal.
/// - `Err(String)` naming the missing or mismatched counter when validation
///   fails.
///
/// Transformation:
/// - Pulls actual counter values through `count_for` and compares them to the
///   static baseline without owning any JSON representation.
pub(crate) fn validate_manifest_baseline_counts(
    baseline: &ManifestBaseline,
    mut count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    for count in baseline.counts {
        let actual = count_for(count.field).ok_or_else(|| {
            format!(
                "manifest count {}.{} is missing",
                baseline.module_name, count.field
            )
        })?;
        if actual != count.expected {
            return Err(format!(
                "unexpected manifest count for {}.{}: expected {}, got {}",
                baseline.module_name, count.field, count.expected, actual
            ));
        }
    }
    Ok(())
}

/// Validates a phase manifest artifact against one gate-backed baseline and readiness.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `expected_readiness`: required manifest `core_proof_coverage.readiness`
///   value.
/// - `core_ir_hash`: actual manifest `core_ir_hash` value.
/// - `readiness`: actual manifest `core_proof_coverage.readiness` value.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when the manifest has a nonzero CoreIR hash, reports the
///   expected readiness, and matches all baseline counters.
/// - `Err(String)` naming the failed artifact-level or counter-level
///   requirement.
///
/// Transformation:
/// - Checks artifact identity/readiness first, then delegates numeric counter
///   validation to `validate_manifest_baseline_counts`.
pub(crate) fn validate_manifest_baseline_artifact_with_readiness(
    baseline: &ManifestBaseline,
    expected_readiness: &str,
    core_ir_hash: Option<u64>,
    readiness: Option<&str>,
    count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    match core_ir_hash {
        Some(hash) if hash != 0 => {}
        Some(_) => {
            return Err(format!(
                "manifest for {} has zero core_ir_hash",
                baseline.module_name
            ));
        }
        None => {
            return Err(format!(
                "manifest for {} is missing core_ir_hash",
                baseline.module_name
            ));
        }
    }

    match readiness {
        Some(actual) if actual == expected_readiness => {}
        Some(actual) => {
            return Err(format!(
                "manifest for {} has readiness {actual:?}, expected {expected_readiness:?}",
                baseline.module_name,
            ));
        }
        None => {
            return Err(format!(
                "manifest for {} is missing core proof readiness",
                baseline.module_name
            ));
        }
    }

    validate_manifest_baseline_counts(baseline, count_for)
}

/// Validates a Lean-covered phase manifest artifact against one baseline.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `core_ir_hash`: actual manifest `core_ir_hash` value.
/// - `readiness`: actual manifest `core_proof_coverage.readiness` value.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when the manifest has nonzero CoreIR hash, `lean-covered`
///   readiness, and matching counters.
/// - `Err(String)` naming the failed artifact-level or counter-level
///   requirement.
///
/// Transformation:
/// - Delegates to `validate_manifest_baseline_artifact_with_readiness` with the
///   Lean-ready readiness label.
pub(crate) fn validate_manifest_baseline_artifact(
    baseline: &ManifestBaseline,
    core_ir_hash: Option<u64>,
    readiness: Option<&str>,
    count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    validate_manifest_baseline_artifact_with_readiness(
        baseline,
        "lean-covered",
        core_ir_hash,
        readiness,
        count_for,
    )
}

#[cfg(test)]
#[path = "proof_baseline_test.rs"]
mod proof_baseline_test;
