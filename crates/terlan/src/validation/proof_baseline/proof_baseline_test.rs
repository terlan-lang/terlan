use super::{
    contract_baselines, manifest_baselines, next_lean_model_candidate_baselines,
    next_lean_model_candidate_manifest_baselines, validate_contract_baseline,
    validate_manifest_baseline_artifact, validate_manifest_baseline_counts,
    RESOLVED_CONSTRUCTOR_COUNTER_FIELDS, UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS,
};

/// Verifies contract and manifest baselines protect the same LP8 fixtures.
///
/// Inputs:
/// - Static `contract_baselines` and `manifest_baselines` tables.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Extracts module names from both static tables and compares them to the
///   expected gate-backed LP8 fixture sequence.
#[test]
fn proof_baseline_tables_cover_same_fixture_set() {
    let contract_names = contract_baselines()
        .iter()
        .map(|baseline| baseline.module_name)
        .collect::<Vec<_>>();
    let manifest_names = manifest_baselines()
        .iter()
        .map(|baseline| baseline.module_name)
        .collect::<Vec<_>>();
    let expected = vec![
        "phase_basic",
        "phase_binary_eq",
        "phase_binary_lt",
        "phase_binary_lte",
        "phase_binary_gt",
        "phase_binary_gte",
        "phase_binary_mul",
        "phase_binary_sub",
        "phase_core_lean",
        "phase_int_literal",
        "phase_atom_literal",
        "phase_binary_literal",
        "phase_tuple_literal",
        "phase_list_literal",
        "phase_named_call",
        "phase_unary_operator",
        "phase_core_lambda",
        "phase_constructor_resolution",
        "phase_constructor_pattern_resolution",
        "phase_literal_pattern_case",
        "phase_list_cons",
        "phase_if_expr",
        "phase_field_access",
    ];

    assert_eq!(contract_names, expected);
    assert_eq!(manifest_names, expected);
}

/// Verifies candidate contract and manifest baselines protect the same fixtures.
///
/// Inputs:
/// - Static `next_lean_model_candidate_baselines` table.
/// - Static `next_lean_model_candidate_manifest_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Extracts module names from both next-model candidate tables and checks
///   that the contract and manifest candidate lists stay aligned.
#[test]
fn proof_baseline_next_model_candidate_tables_cover_same_fixture_set() {
    let contract_names = next_lean_model_candidate_baselines()
        .iter()
        .map(|baseline| baseline.module_name)
        .collect::<Vec<_>>();
    let manifest_names = next_lean_model_candidate_manifest_baselines()
        .iter()
        .map(|baseline| baseline.module_name)
        .collect::<Vec<_>>();

    assert_eq!(contract_names, manifest_names);
}

/// Verifies candidate blocked-form snippets match manifest debt counters.
///
/// Inputs:
/// - Static `next_lean_model_candidate_baselines` table.
/// - Static `next_lean_model_candidate_manifest_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Counts concrete `:proof=proof-model-required` expression snippets in
///   each next-model candidate contract baseline, then checks the matching
///   manifest baseline reports the same `proof_model_required` count.
#[test]
fn proof_baseline_next_model_candidate_blocked_form_counts_match_manifest() {
    for contract_candidate in next_lean_model_candidate_baselines() {
        let manifest_candidate = next_lean_model_candidate_manifest_baselines()
            .iter()
            .find(|candidate| candidate.module_name == contract_candidate.module_name)
            .unwrap_or_else(|| {
                panic!(
                    "{} missing next-model candidate manifest baseline",
                    contract_candidate.module_name
                )
            });
        let contract_blocked_form_count = contract_candidate
            .required_snippets
            .iter()
            .filter(|snippet| snippet.contains(":proof=proof-model-required"))
            .count() as u64;
        let manifest_blocked_form_count = manifest_candidate
            .counts
            .iter()
            .find(|count| count.field == "proof_model_required")
            .unwrap_or_else(|| {
                panic!(
                    "{} missing proof_model_required manifest counter",
                    manifest_candidate.module_name
                )
            })
            .expected;

        assert_eq!(
            contract_blocked_form_count, manifest_blocked_form_count,
            "{} contract blocked-form snippets must match manifest proof_model_required count",
            contract_candidate.module_name
        );
    }
}

/// Verifies the Lean resume cycle has exactly one pinned candidate.
///
/// Inputs:
/// - Static `next_lean_model_candidate_baselines` table.
/// - Static `next_lean_model_candidate_manifest_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Counts next-model candidate contract and manifest entries and checks
///   that each table contains one selected fixture for the current cycle.
#[test]
fn proof_baseline_next_model_candidates_select_exactly_one_fixture() {
    assert_eq!(
        next_lean_model_candidate_baselines().len(),
        1,
        "LP8 should select exactly one next-model contract candidate"
    );
    assert_eq!(
        next_lean_model_candidate_manifest_baselines().len(),
        1,
        "LP8 should select exactly one next-model manifest candidate"
    );
}

/// Verifies next-model candidates are not mixed into Lean-ready baselines.
///
/// Inputs:
/// - Static Lean-ready and next-model candidate baseline tables.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Checks candidate snippets explicitly require `proof-model-required`
///   readiness plus at least one concrete proof-model-required contract
///   form, and that no candidate fixture is already in the Lean-ready
///   baseline table.
#[test]
fn proof_baseline_next_model_candidates_are_separate_from_ready_baselines() {
    for candidate in next_lean_model_candidate_baselines() {
        assert!(
            candidate
                .required_snippets
                .iter()
                .any(|snippet| snippet.contains("proof_readiness:proof-model-required")),
            "{} must remain a proof-model-required candidate",
            candidate.module_name
        );
        assert!(
            candidate
                .required_snippets
                .iter()
                .any(|snippet| snippet.contains(":proof=proof-model-required")),
            "{} must pin the concrete proof-model-required Core form",
            candidate.module_name
        );
        assert!(
            !contract_baselines()
                .iter()
                .any(|baseline| baseline.module_name == candidate.module_name),
            "{} must not be listed as Lean-ready yet",
            candidate.module_name
        );
    }

    for candidate in next_lean_model_candidate_manifest_baselines() {
        let proof_model_count = candidate
            .counts
            .iter()
            .find(|count| count.field == "proof_model_required")
            .unwrap_or_else(|| panic!("{} missing proof_model_required", candidate.module_name));
        assert!(
            proof_model_count.expected > 0,
            "{} must carry explicit proof-model-required debt",
            candidate.module_name
        );
        assert!(
            !manifest_baselines()
                .iter()
                .any(|baseline| baseline.module_name == candidate.module_name),
            "{} must not be listed as Lean-ready manifest baseline yet",
            candidate.module_name
        );
    }
}

/// Verifies CoreIR contract baselines include readiness-critical snippets.
///
/// Inputs:
/// - Static `contract_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Checks each baseline has nonempty snippets and explicitly requires
///   `proof_readiness:lean-covered` in the expected CoreIR contract text.
#[test]
fn proof_baseline_contracts_require_lean_covered_readiness() {
    for baseline in contract_baselines() {
        assert!(
            !baseline.required_snippets.is_empty(),
            "{} must require CoreIR snippets",
            baseline.module_name
        );
        assert!(
            baseline
                .required_snippets
                .iter()
                .any(|snippet| snippet.contains("proof_readiness:lean-covered")),
            "{} must require lean-covered metadata readiness",
            baseline.module_name
        );
    }
}

/// Verifies phase-manifest baselines include proof-export safety counters.
///
/// Inputs:
/// - Static `manifest_baselines` table.
/// - Static `next_lean_model_candidate_manifest_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Checks every Lean-ready and next-model candidate manifest baseline has
///   nonempty counters, requires zero unresolved constructor call, chain,
///   and pattern candidates, and records both expression freshness
///   partitions.
/// - Checks Lean-ready baselines, but not next-model candidates, require
///   zero proof-readiness debt counters.
#[test]
fn proof_baseline_manifests_require_resolution_and_freshness_counters() {
    for baseline in manifest_baselines()
        .iter()
        .chain(next_lean_model_candidate_manifest_baselines())
    {
        assert!(
            !baseline.counts.is_empty(),
            "{} must require manifest counters",
            baseline.module_name
        );
        for field in [
            "partial",
            "proof_model_required",
            "runtime_boundary",
            "artifact_only",
            "pattern_partial",
            "pattern_proof_model_required",
            "pattern_runtime_boundary",
            "pattern_artifact_only",
            "checked_preservation_expr_structural",
            "checked_preservation_pattern_structural",
            "checked_preservation_expr_no_runtime_bindings",
            "checked_preservation_expr_runtime_bindings_required",
            "checked_preservation_pattern_no_runtime_bindings",
            "checked_preservation_pattern_runtime_bindings_required",
        ] {
            assert!(
                baseline.counts.iter().any(|count| count.field == field),
                "{} must require {field}",
                baseline.module_name
            );
        }
        for field in RESOLVED_CONSTRUCTOR_COUNTER_FIELDS
            .iter()
            .chain(UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS)
        {
            assert!(
                baseline.counts.iter().any(|count| count.field == *field),
                "{} must require {field}",
                baseline.module_name
            );
        }
        for field in UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS {
            let count = baseline
                .counts
                .iter()
                .find(|count| count.field == *field)
                .unwrap_or_else(|| panic!("{} missing {field}", baseline.module_name));
            assert_eq!(
                count.expected, 0,
                "{} must require zero {field}",
                baseline.module_name
            );
        }
    }

    for baseline in manifest_baselines() {
        for field in [
            "partial",
            "proof_model_required",
            "runtime_boundary",
            "artifact_only",
            "pattern_partial",
            "pattern_proof_model_required",
            "pattern_runtime_boundary",
            "pattern_artifact_only",
        ] {
            let count = baseline
                .counts
                .iter()
                .find(|count| count.field == field)
                .unwrap_or_else(|| panic!("{} missing {field}", baseline.module_name));
            assert_eq!(
                count.expected, 0,
                "{} must require zero {field}",
                baseline.module_name
            );
        }
    }
}

/// Verifies each manifest baseline has one expectation per field.
///
/// Inputs:
/// - Static `manifest_baselines` table.
/// - Static `next_lean_model_candidate_manifest_baselines` table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Compares each counter field with preceding fields in the same baseline
///   and fails if a static Lean-ready or next-model candidate manifest
///   contract repeats a field name.
#[test]
fn proof_baseline_manifest_fields_are_unique() {
    for baseline in manifest_baselines()
        .iter()
        .chain(next_lean_model_candidate_manifest_baselines())
    {
        for (index, count) in baseline.counts.iter().enumerate() {
            assert!(
                !baseline.counts[..index]
                    .iter()
                    .any(|previous| previous.field == count.field),
                "{} repeats manifest field {}",
                baseline.module_name,
                count.field
            );
        }
    }
}

/// Verifies the reusable contract validator reports missing snippets.
///
/// Inputs:
/// - First static contract baseline.
/// - Deliberately empty CoreIR contract text.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Runs `validate_contract_baseline` against invalid text and checks that
///   the error identifies the affected fixture.
#[test]
fn proof_baseline_contract_validator_reports_missing_snippet() {
    let baseline = &contract_baselines()[0];
    let err =
        validate_contract_baseline(baseline, "").expect_err("empty contract text should fail");

    assert!(err.contains(baseline.module_name));
}

/// Verifies the reusable manifest validator reports counter mismatches.
///
/// Inputs:
/// - First static manifest baseline.
/// - Lookup closure that returns zero for every requested counter.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Runs `validate_manifest_baseline_counts` against intentionally stale
///   counter values and checks that the error names the mismatched fixture.
#[test]
fn proof_baseline_manifest_validator_reports_counter_mismatch() {
    let baseline = &manifest_baselines()[0];
    let err = validate_manifest_baseline_counts(baseline, |_| Some(0))
        .expect_err("zeroed manifest counters should fail");

    assert!(err.contains(baseline.module_name));
}

/// Verifies the reusable manifest artifact validator reports bad readiness.
///
/// Inputs:
/// - First static manifest baseline.
/// - Nonzero CoreIR hash.
/// - Deliberately stale proof readiness value.
/// - Lookup closure that returns expected baseline counters.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Runs `validate_manifest_baseline_artifact` against stale readiness and
///   checks that the error names the affected fixture.
#[test]
fn proof_baseline_manifest_artifact_validator_reports_bad_readiness() {
    let baseline = &manifest_baselines()[0];
    let err = validate_manifest_baseline_artifact(
        baseline,
        Some(1),
        Some("proof-model-required"),
        |field| {
            baseline
                .counts
                .iter()
                .find(|count| count.field == field)
                .map(|count| count.expected)
        },
    )
    .expect_err("stale readiness should fail");

    assert!(err.contains(baseline.module_name));
}

/// Verifies the pinned remote-call candidate keeps concrete compiler data.
///
/// Inputs:
/// - Static next-model candidate table.
///
/// Output:
/// - Test assertion only; no files or compiler artifacts are modified.
///
/// Transformation:
/// - Checks the static `phase_trait` baseline is still pinned to a
///   proof-model-required remote-call Core form. Documentation handoff
///   wording is validated by internal script tooling rather than crate
///   tests so release compiler crates do not include roadmap prose.
#[test]
fn proof_baseline_phase_trait_pins_remote_dispatch_contract() {
    let Some(phase_trait_candidate) = next_lean_model_candidate_baselines()
        .iter()
        .find(|candidate| candidate.module_name == "phase_trait")
    else {
        return;
    };

    assert!(
        phase_trait_candidate
            .required_snippets
            .iter()
            .any(|snippet| snippet.contains("RemoteCall(")
                && snippet.contains(":proof=proof-model-required")),
        "phase_trait must remain pinned to a proof-model-required remote-call Core form"
    );
}
