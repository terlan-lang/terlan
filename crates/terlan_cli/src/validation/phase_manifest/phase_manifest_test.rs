use super::*;

/// Builds a valid phase-manifest JSON fixture with caller-selected debug
/// trace overrides.
///
/// Inputs:
/// - `debug_module`: module identity recorded in the nested debug trace.
/// - `debug_source_path`: source path recorded in the nested debug trace.
/// - `debug_core_ir_hash`: CoreIR hash recorded in the nested debug trace.
/// - `debug_core_ir_available`: CoreIR availability flag recorded in the
///   nested debug trace.
/// - `artifact_kind`: generated artifact kind recorded in the debug trace.
/// - `artifact_name`: optional generated artifact name recorded in the debug
///   trace.
///
/// Output:
/// - Serialized phase-manifest JSON string.
///
/// Transformation:
/// - Embeds the current syntax contract identity and a minimal successful
///   declaration-only CoreIR coverage payload, then varies only the debug
///   fields under test.
fn phase_manifest_json_with_debug_trace(
    debug_module: &str,
    debug_source_path: &str,
    debug_core_ir_hash: u64,
    debug_core_ir_available: bool,
    artifact_kind: &str,
    artifact_name: Option<&str>,
) -> String {
    serde_json::json!({
        "schema": PHASE_MANIFEST_SCHEMA,
        "module": "sample",
        "source_path": "sample.terl",
        "debug_trace": {
            "module": debug_module,
            "source_path": debug_source_path,
            "core_ir_hash": debug_core_ir_hash,
            "core_ir_available": debug_core_ir_available,
            "generated_artifact_kind": artifact_kind,
            "generated_artifact_name": artifact_name,
        },
        "syntax_contract": current_syntax_contract_identity()
            .expect("current syntax contract identity for manifest fixture"),
        "source_hash": 1_u64,
        "interface_hash": 2_u64,
        "interface_doc_hash": 3_u64,
        "core_ir_hash": 4_u64,
        "core_proof_coverage": {
            "readiness": "no-expressions",
            "lean_covered": 0,
            "partial": 0,
            "proof_model_required": 0,
            "runtime_boundary": 0,
            "artifact_only": 0,
            "pattern_lean_covered": 0,
            "pattern_partial": 0,
            "pattern_proof_model_required": 0,
            "pattern_runtime_boundary": 0,
            "pattern_artifact_only": 0,
            "typed_core_expr": 0,
            "summary_only_expr": 0,
            "typed_core_pattern": 0,
            "summary_only_pattern": 0,
            "typed_core_type": 1,
            "summary_only_type": 0,
            "checked_preservation_expr": 0,
            "checked_preservation_pattern": 0,
            "checked_preservation_expr_structural": 0,
            "checked_preservation_pattern_structural": 0,
            "checked_preservation_expr_no_runtime_bindings": 0,
            "checked_preservation_pattern_no_runtime_bindings": 0,
            "checked_preservation_expr_runtime_bindings_required": 0,
            "checked_preservation_pattern_runtime_bindings_required": 0,
            "resolved_constructor_call_identity": 0,
            "resolved_constructor_chain_identity": 0,
            "resolved_constructor_pattern_identity": 0,
            "unresolved_constructor_call_candidate": 0,
            "unresolved_constructor_chain_candidate": 0,
            "unresolved_constructor_pattern_candidate": 0,
        },
        "dependencies": [],
        "phases": [
            {
                "name": "parse",
                "status": "ok",
                "diagnostics": [],
            },
            {
                "name": "core",
                "status": "ok",
                "diagnostics": [],
            },
        ],
    })
    .to_string()
}

/// Verifies phase-manifest validation accepts a coherent debug trace.
///
/// Inputs:
/// - None; constructs an in-memory manifest with matching top-level and
///   nested debug identity.
///
/// Output:
/// - Test passes when validation returns a snapshot exposing the debug
///   trace.
///
/// Transformation:
/// - Parses the manifest JSON through the same validation path used for
///   emitted phase manifests.
#[test]
fn phase_manifest_validation_accepts_debug_trace_identity() {
    let manifest =
        phase_manifest_json_with_debug_trace("sample", "sample.terl", 4, true, "none", None);

    let snapshot = validate_phase_manifest_contents(&manifest).expect("valid debug trace manifest");

    assert_eq!(snapshot.debug_trace.module, "sample");
    assert_eq!(snapshot.debug_trace.source_path, "sample.terl");
    assert_eq!(snapshot.debug_trace.core_ir_hash, 4);
    assert!(snapshot.debug_trace.core_ir_available);
    assert_eq!(snapshot.debug_trace.generated_artifact_kind, "none");
    assert_eq!(snapshot.debug_trace.generated_artifact_name, None);
}

/// Verifies phase-manifest validation rejects stale debug module identity.
///
/// Inputs:
/// - None; constructs an in-memory manifest whose debug trace names a
///   different module than the top-level manifest.
///
/// Output:
/// - Test passes when validation rejects the mismatch.
///
/// Transformation:
/// - Parses malformed manifest JSON through the production validator to
///   protect source-to-artifact identity consistency.
#[test]
fn phase_manifest_validation_rejects_debug_trace_module_mismatch() {
    let manifest =
        phase_manifest_json_with_debug_trace("other", "sample.terl", 4, true, "none", None);

    let error = match validate_phase_manifest_contents(&manifest) {
        Ok(_) => panic!("debug trace module mismatch should fail"),
        Err(error) => error,
    };

    assert!(
        error.contains("debug trace module"),
        "unexpected error: {error}"
    );
}

/// Verifies phase-manifest validation rejects stale debug CoreIR identity.
///
/// Inputs:
/// - None; constructs an in-memory manifest whose nested CoreIR hash differs
///   from the top-level CoreIR hash.
///
/// Output:
/// - Test passes when validation rejects the mismatch.
///
/// Transformation:
/// - Exercises the debug-trace CoreIR identity guard independently from the
///   proof-coverage hash guard.
#[test]
fn phase_manifest_validation_rejects_debug_trace_core_hash_mismatch() {
    let manifest =
        phase_manifest_json_with_debug_trace("sample", "sample.terl", 5, true, "none", None);

    let error = match validate_phase_manifest_contents(&manifest) {
        Ok(_) => panic!("debug trace CoreIR hash mismatch should fail"),
        Err(error) => error,
    };

    assert!(
        error.contains("debug trace CoreIR hash"),
        "unexpected error: {error}"
    );
}

/// Verifies matched Lean-covered and typed-payload counts are accepted.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when `validate_typed_payload_consistency` returns `Ok`.
///
/// Transformation:
/// - Sets matching expression and pattern coverage/payload counts without
///   serializing a full phase manifest.
#[test]
fn phase_manifest_core_proof_coverage_accepts_typed_payload_match() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 2,
        pattern_lean_covered: 1,
        checked_preservation_expr: 2,
        checked_preservation_pattern: 1,
        checked_preservation_expr_structural: 2,
        checked_preservation_pattern_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 2,
        checked_preservation_pattern_runtime_bindings_required: 1,
        typed_core_expr: 2,
        typed_core_type: 1,
        typed_core_pattern: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    assert!(coverage.validate_typed_payload_consistency().is_ok());
}

/// Verifies typed Core payloads may exceed Lean-covered counts.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture with typed
///   partial/proof-model-required payloads in addition to Lean-covered
///   payloads.
///
/// Output:
/// - Test passes when `validate_typed_payload_consistency` returns `Ok`.
///
/// Transformation:
/// - Sets typed payload and checked-preservation counts above Lean-covered
///   counts to match production CoreIR forms that are typed but not yet
///   Lean-modeled.
#[test]
fn phase_manifest_core_proof_coverage_accepts_typed_payload_superset() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "proof-model-required".to_string(),
        lean_covered: 2,
        proof_model_required: 1,
        pattern_lean_covered: 1,
        checked_preservation_expr: 3,
        checked_preservation_pattern: 1,
        checked_preservation_expr_structural: 3,
        checked_preservation_pattern_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 3,
        checked_preservation_pattern_runtime_bindings_required: 1,
        typed_core_expr: 3,
        typed_core_pattern: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    assert!(coverage.validate_typed_payload_consistency().is_ok());
}

/// Verifies readiness must match the combined expression/pattern buckets.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture whose readiness
///   claims `lean-covered` while pattern coverage still has proof-model
///   debt.
///
/// Output:
/// - Test passes when readiness validation rejects the stale coverage
///   label.
///
/// Transformation:
/// - Keeps typed payload and preservation counts internally consistent so
///   the failure isolates readiness-vs-bucket coherence.
#[test]
fn phase_manifest_core_proof_coverage_rejects_readiness_bucket_mismatch() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 1,
        pattern_proof_model_required: 1,
        typed_core_expr: 1,
        typed_core_pattern: 1,
        typed_core_type: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("readiness bucket mismatch should fail");
    assert!(
        error.contains("proof readiness lean-covered does not match coverage buckets"),
        "unexpected error: {error}"
    );
}

/// Verifies readiness bucket precedence matches CoreIR metadata
/// construction.
///
/// Inputs:
/// - None; constructs in-memory proof coverage fixtures for each
///   precedence boundary.
///
/// Output:
/// - Test passes when expected readiness follows runtime-boundary,
///   partial, proof-model-required, artifact-only, lean-covered, and
///   no-expressions order.
///
/// Transformation:
/// - Exercises readiness computation directly without payload or
///   preservation validation noise.
#[test]
fn phase_manifest_core_proof_coverage_readiness_precedence_matches_core_metadata() {
    let cases = [
        (
            PhaseManifestCoreProofCoverage {
                runtime_boundary: 1,
                partial: 1,
                proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
                ..PhaseManifestCoreProofCoverage::default()
            },
            "runtime-boundary",
        ),
        (
            PhaseManifestCoreProofCoverage {
                pattern_partial: 1,
                proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
                ..PhaseManifestCoreProofCoverage::default()
            },
            "partial",
        ),
        (
            PhaseManifestCoreProofCoverage {
                pattern_proof_model_required: 1,
                artifact_only: 1,
                lean_covered: 1,
                ..PhaseManifestCoreProofCoverage::default()
            },
            "proof-model-required",
        ),
        (
            PhaseManifestCoreProofCoverage {
                pattern_artifact_only: 1,
                lean_covered: 1,
                ..PhaseManifestCoreProofCoverage::default()
            },
            "artifact-only",
        ),
        (
            PhaseManifestCoreProofCoverage {
                pattern_lean_covered: 1,
                ..PhaseManifestCoreProofCoverage::default()
            },
            "lean-covered",
        ),
        (PhaseManifestCoreProofCoverage::default(), "no-expressions"),
    ];

    for (coverage, expected) in cases {
        assert_eq!(coverage.expected_readiness(), expected);
    }
}

/// Verifies summary-only CoreType positions affect manifest readiness.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture with lean-covered
///   expression coverage and one summary-only CoreType position.
///
/// Output:
/// - Test passes when expected readiness reports proof-model-required.
///
/// Transformation:
/// - Exercises phase-manifest readiness derivation without full CoreIR
///   artifacts.
#[test]
fn phase_manifest_core_proof_coverage_readiness_includes_summary_only_type_debt() {
    let coverage = PhaseManifestCoreProofCoverage {
        lean_covered: 1,
        summary_only_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    assert_eq!(coverage.expected_readiness(), "proof-model-required");
}

/// Verifies `no-expressions` readiness remains valid for declaration-only
/// CoreIR payloads.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture with no
///   expression or pattern summaries but with CoreType payload metrics.
///
/// Output:
/// - Test passes when validation accepts the declaration-only CoreIR
///   readiness state.
///
/// Transformation:
/// - Separates real CoreIR-without-expressions readiness from skipped
///   `none` placeholders.
#[test]
fn phase_manifest_core_proof_coverage_accepts_no_expressions_with_type_payloads() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "no-expressions".to_string(),
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    assert!(coverage.validate_typed_payload_consistency().is_ok());
}

/// Verifies `no-expressions` readiness still requires type payload metrics.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture with
///   `no-expressions` readiness and no CoreType payload counters.
///
/// Output:
/// - Test passes when validation rejects the empty CoreIR proof payload.
///
/// Transformation:
/// - Exercises declaration-only readiness validation independently from
///   skipped `none` placeholders.
#[test]
fn phase_manifest_core_proof_coverage_rejects_no_expressions_without_type_payloads() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "no-expressions".to_string(),
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("no-expressions without type payloads should fail");
    assert!(
        error.contains("CoreType signature payload counts"),
        "unexpected error: {error}"
    );
}

/// Verifies `none` readiness is only valid for empty coverage placeholders.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture whose readiness
///   is `none` while a typed CoreExpr counter is present.
///
/// Output:
/// - Test passes when coverage validation rejects the non-empty placeholder.
///
/// Transformation:
/// - Exercises skipped/error-path manifest consistency independently from
///   the outer CoreIR hash validation.
#[test]
fn phase_manifest_core_proof_coverage_rejects_none_readiness_with_counters() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "none".to_string(),
        typed_core_expr: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("none readiness with counters should fail");
    assert!(
        error.contains("readiness none requires zero coverage counters"),
        "unexpected error: {error}"
    );
}

/// Verifies expression Lean coverage cannot exceed typed CoreExpr payloads.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when expression mismatch validation returns an error.
///
/// Transformation:
/// - Sets mismatched expression coverage/payload counts while leaving
///   pattern counts consistent.
#[test]
fn phase_manifest_core_proof_coverage_rejects_expr_payload_mismatch() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 2,
        typed_core_expr: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("expression payload mismatch should fail");
    assert!(
        error.contains("typed CoreExpr count"),
        "unexpected error: {error}"
    );
}

/// Verifies pattern Lean coverage cannot exceed typed CorePattern payloads.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when pattern mismatch validation returns an error.
///
/// Transformation:
/// - Sets mismatched pattern coverage/payload counts while leaving
///   expression counts consistent.
#[test]
fn phase_manifest_core_proof_coverage_rejects_pattern_payload_mismatch() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        pattern_lean_covered: 2,
        typed_core_pattern: 1,
        checked_preservation_pattern: 1,
        checked_preservation_pattern_structural: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("pattern payload mismatch should fail");
    assert!(
        error.contains("typed CorePattern count"),
        "unexpected error: {error}"
    );
}

/// Verifies expression Lean coverage requires checked-preservation evidence.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when missing expression checked-preservation evidence
///   returns an error.
///
/// Transformation:
/// - Keeps typed expression payloads available while setting
///   checked-preservation evidence below Lean-covered expression count.
#[test]
fn phase_manifest_core_proof_coverage_rejects_expr_checked_evidence_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "proof-model-required".to_string(),
        lean_covered: 2,
        proof_model_required: 1,
        typed_core_expr: 3,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("expression checked-preservation gap should fail");
    assert!(
        error.contains("lean-covered expressions"),
        "unexpected error: {error}"
    );
}

/// Verifies pattern Lean coverage requires checked-preservation evidence.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when missing pattern checked-preservation evidence returns
///   an error.
///
/// Transformation:
/// - Keeps typed pattern payloads available while setting
///   checked-preservation evidence below Lean-covered pattern count.
#[test]
fn phase_manifest_core_proof_coverage_rejects_pattern_checked_evidence_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "proof-model-required".to_string(),
        pattern_lean_covered: 2,
        pattern_proof_model_required: 1,
        typed_core_pattern: 3,
        checked_preservation_pattern: 1,
        checked_preservation_pattern_structural: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("pattern checked-preservation gap should fail");
    assert!(
        error.contains("lean-covered patterns"),
        "unexpected error: {error}"
    );
}

/// Verifies CoreIR manifests must include signature type payload metrics.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when non-default readiness without type payload metrics
///   returns an error.
///
/// Transformation:
/// - Sets a CoreIR readiness state while leaving type payload counts empty
///   to exercise the manifest consistency guard.
#[test]
fn phase_manifest_core_proof_coverage_rejects_missing_type_payload_metrics() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 1,
        typed_core_expr: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("missing type payload metrics should fail");
    assert!(
        error.contains("CoreType signature payload counts"),
        "unexpected error: {error}"
    );
}

/// Verifies expression checked-preservation counts name their evidence kind.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when preservation evidence without matching structural
///   evidence-kind accounting returns an error.
///
/// Transformation:
/// - Sets a valid expression preservation count while leaving the
///   structural expression preservation count below it.
#[test]
fn phase_manifest_core_proof_coverage_rejects_expr_structural_evidence_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 0,
        typed_core_expr: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("expression structural checked-preservation gap should fail");
    assert!(
        error.contains("structural checked-preservation expression count"),
        "unexpected error: {error}"
    );
}

/// Verifies pattern checked-preservation counts name their evidence kind.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when pattern preservation evidence without matching
///   structural evidence-kind accounting returns an error.
///
/// Transformation:
/// - Sets a valid pattern preservation count while leaving the structural
///   pattern preservation count below it.
#[test]
fn phase_manifest_core_proof_coverage_rejects_pattern_structural_evidence_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        pattern_lean_covered: 1,
        checked_preservation_pattern: 1,
        checked_preservation_pattern_structural: 0,
        typed_core_pattern: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("pattern structural checked-preservation gap should fail");
    assert!(
        error.contains("structural checked-preservation pattern count"),
        "unexpected error: {error}"
    );
}

/// Verifies expression freshness counts partition preservation evidence.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when expression preservation evidence without matching
///   freshness accounting returns an error.
///
/// Transformation:
/// - Keeps expression preservation and structural counts consistent while
///   leaving the freshness buckets below the expression preservation total.
#[test]
fn phase_manifest_core_proof_coverage_rejects_expr_freshness_partition_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        lean_covered: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 0,
        checked_preservation_expr_runtime_bindings_required: 0,
        typed_core_expr: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("expression freshness partition gap should fail");
    assert!(
        error.contains("expression freshness counts"),
        "unexpected error: {error}"
    );
}

/// Verifies pattern freshness counts partition preservation evidence.
///
/// Inputs:
/// - None; constructs an in-memory proof coverage fixture.
///
/// Output:
/// - Test passes when pattern preservation evidence without matching
///   freshness accounting returns an error.
///
/// Transformation:
/// - Keeps pattern preservation and structural counts consistent while
///   leaving the freshness buckets below the pattern preservation total.
#[test]
fn phase_manifest_core_proof_coverage_rejects_pattern_freshness_partition_gap() {
    let coverage = PhaseManifestCoreProofCoverage {
        readiness: "lean-covered".to_string(),
        pattern_lean_covered: 1,
        checked_preservation_pattern: 1,
        checked_preservation_pattern_structural: 1,
        checked_preservation_pattern_no_runtime_bindings: 0,
        checked_preservation_pattern_runtime_bindings_required: 0,
        typed_core_pattern: 1,
        typed_core_type: 1,
        ..PhaseManifestCoreProofCoverage::default()
    };

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("pattern freshness partition gap should fail");
    assert!(
        error.contains("pattern freshness counts"),
        "unexpected error: {error}"
    );
}

/// Builds an otherwise consistent coverage fixture with unresolved debt.
///
/// Inputs:
/// - `call_candidates`: unresolved constructor-call candidate count.
/// - `chain_candidates`: unresolved constructor-chain candidate count.
/// - `pattern_candidates`: unresolved constructor-pattern candidate count.
///
/// Output:
/// - `PhaseManifestCoreProofCoverage` ready for validation.
///
/// Transformation:
/// - Keeps typed-payload, preservation, freshness, and type-payload counts
///   internally consistent while injecting caller-selected constructor
///   resolution debt.
fn unresolved_constructor_candidate_coverage(
    call_candidates: usize,
    chain_candidates: usize,
    pattern_candidates: usize,
) -> PhaseManifestCoreProofCoverage {
    PhaseManifestCoreProofCoverage {
        readiness: "proof-model-required".to_string(),
        lean_covered: 1,
        proof_model_required: 1,
        checked_preservation_expr: 1,
        checked_preservation_expr_structural: 1,
        checked_preservation_expr_no_runtime_bindings: 1,
        typed_core_expr: 1,
        typed_core_type: 1,
        unresolved_constructor_call_candidate: call_candidates,
        unresolved_constructor_chain_candidate: chain_candidates,
        unresolved_constructor_pattern_candidate: pattern_candidates,
        ..PhaseManifestCoreProofCoverage::default()
    }
}

/// Verifies unresolved constructor-call candidates fail formal manifest
/// validation.
///
/// Inputs:
/// - None; constructs an otherwise consistent in-memory proof coverage
///   fixture with one unresolved constructor-call candidate.
///
/// Output:
/// - Test passes when constructor-resolution validation returns an error.
///
/// Transformation:
/// - Keeps typed-payload, preservation, freshness, and type-payload counts
///   internally consistent while adding unresolved semantic constructor
///   debt.
#[test]
fn phase_manifest_core_proof_coverage_rejects_unresolved_constructor_call_candidate() {
    let coverage = unresolved_constructor_candidate_coverage(1, 0, 0);

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("unresolved constructor candidates should fail");
    assert_eq!(
        error, PHASE_MANIFEST_UNRESOLVED_CONSTRUCTOR_ERROR,
        "unexpected error"
    );
}

/// Verifies unresolved constructor-chain candidates fail formal manifest
/// validation.
///
/// Inputs:
/// - None; constructs an otherwise consistent in-memory proof coverage
///   fixture with one unresolved constructor-chain candidate.
///
/// Output:
/// - Test passes when constructor-resolution validation returns an error.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the chain
///   counter from other proof-coverage consistency rules.
#[test]
fn phase_manifest_core_proof_coverage_rejects_unresolved_constructor_chain_candidate() {
    let coverage = unresolved_constructor_candidate_coverage(0, 1, 0);

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("unresolved constructor candidates should fail");
    assert_eq!(
        error, PHASE_MANIFEST_UNRESOLVED_CONSTRUCTOR_ERROR,
        "unexpected error"
    );
}

/// Verifies unresolved constructor-pattern candidates fail formal manifest
/// validation.
///
/// Inputs:
/// - None; constructs an otherwise consistent in-memory proof coverage
///   fixture with one unresolved constructor-pattern candidate.
///
/// Output:
/// - Test passes when constructor-resolution validation returns an error.
///
/// Transformation:
/// - Uses the unresolved-constructor fixture helper to isolate the pattern
///   counter from other proof-coverage consistency rules.
#[test]
fn phase_manifest_core_proof_coverage_rejects_unresolved_constructor_pattern_candidate() {
    let coverage = unresolved_constructor_candidate_coverage(0, 0, 1);

    let error = coverage
        .validate_typed_payload_consistency()
        .expect_err("unresolved constructor candidates should fail");
    assert_eq!(
        error, PHASE_MANIFEST_UNRESOLVED_CONSTRUCTOR_ERROR,
        "unexpected error"
    );
}
