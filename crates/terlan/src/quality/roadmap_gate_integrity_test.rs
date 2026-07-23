use std::collections::BTreeSet;

use super::*;

/// Verifies planned gates may be missing while an unchecked slice owns them.
///
/// Inputs:
/// - Planned gates containing one implemented gate and one future gate.
/// - One unchecked slice naming the future gate with acceptance and failure
///   language.
///
/// Output: no diagnostics.
///
/// Transformation: models the active 0.0.7 state where some named gates are
/// intentionally future work until their checklist item is complete.
#[test]
fn roadmap_gate_integrity_accepts_unchecked_owned_future_gate() {
    let planned = btreeset(["make-check", "future-check"]);
    let unchecked = vec![RoadmapSlice {
        title: "Future slice.".to_string(),
        body: concat!(
            "  - Gate: add `make future-check`.\n",
            "  - Acceptance: positive executable tests pass and the gate fails ",
            "if drift appears.\n"
        )
        .to_string(),
    }];
    let completed = Vec::new();
    let targets = btreeset(["make-check"]);

    let diagnostics = validate_roadmap_gate_integrity(&planned, &unchecked, &completed, &targets);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies unowned planned gates are rejected.
///
/// Inputs:
/// - Planned gate that is neither in the Make graph nor owned by an unchecked
///   slice.
///
/// Output: diagnostic naming the unowned gate.
///
/// Transformation: prevents the planned gate block from becoming aspirational
/// command text without implementation ownership.
#[test]
fn roadmap_gate_integrity_rejects_unowned_planned_gate() {
    let planned = btreeset(["missing-check"]);
    let diagnostics = validate_roadmap_gate_integrity(&planned, &[], &[], &BTreeSet::new());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing-check")),
        "expected missing planned gate diagnostic: {diagnostics:?}"
    );
}

/// Verifies completed slices cannot reference missing Make targets.
///
/// Inputs:
/// - Completed slice whose body names a missing gate.
///
/// Output: diagnostic naming the missing completed gate.
///
/// Transformation: makes completion mean the check is wired, not merely
/// described.
#[test]
fn roadmap_gate_integrity_rejects_completed_slice_missing_gate() {
    let completed = vec![RoadmapSlice {
        title: "Done slice.".to_string(),
        body: "  - Gate: add `make done-check`.\n".to_string(),
    }];

    let diagnostics =
        validate_roadmap_gate_integrity(&BTreeSet::new(), &[], &completed, &BTreeSet::new());

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("done-check")),
        "expected completed missing gate diagnostic: {diagnostics:?}"
    );
}

#[test]
fn roadmap_gate_integrity_rejects_completed_gate_bespoke_rust_tests() {
    let completed = vec![RoadmapSlice {
        title: "Done slice.".to_string(),
        body: "  - Gate: add `make done-check`.\n".to_string(),
    }];
    let make_graph = concat!(
        "COMPLETED_SLICE_RUST_GATES := done-check\n",
        "done-check:\n",
        "\t$(RUST_TEST) -p terlan --bin terlc done_test\n",
    );

    let diagnostics = validate_completed_slice_rust_ownership(
        &completed,
        &[],
        make_graph,
        &btreeset(["done-check"]),
    );

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("bespoke Rust tests")));
}

#[test]
fn roadmap_gate_integrity_accepts_canonical_completed_rust_owner() {
    let completed = vec![RoadmapSlice {
        title: "Done slice.".to_string(),
        body: "  - Gate: add `make done-check`.\n".to_string(),
    }];
    let make_graph = concat!(
        "COMPLETED_SLICE_RUST_GATES := done-check\n",
        "done-check: $(CANONICAL_RUST_SUITE_OWNER)\n",
        "\t$(CARGO) run -p terlan --bin terlan-quality -- done\n",
    );

    let diagnostics = validate_completed_slice_rust_ownership(
        &completed,
        &[],
        make_graph,
        &btreeset(["done-check"]),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Verifies prose using "make" as a verb is not parsed as a gate command.
///
/// Inputs: completed gate prose containing one backticked command followed by
/// "make it part of" wording.
///
/// Output: only the command-form gate is collected.
///
/// Transformation: prevents ordinary roadmap prose from inventing a Make
/// target named `it`.
#[test]
fn roadmap_gate_integrity_ignores_make_used_as_prose() {
    let slices = vec![RoadmapSlice {
        title: "Done slice.".to_string(),
        body: concat!(
            "  - Gate: add `make done-check` and make it part of release.\n",
            "  - Acceptance: executable tests pass.\n"
        )
        .to_string(),
    }];

    assert_eq!(collect_slice_gates(&slices), btreeset(["done-check"]));
}

/// Verifies unchecked slices must name an executable shape.
///
/// Inputs:
/// - Unchecked slice with no gate, no acceptance, and no stable failure text.
///
/// Output: diagnostics for all missing executable details.
///
/// Transformation: keeps active roadmap items actionable instead of vague.
#[test]
fn roadmap_gate_integrity_rejects_vague_unchecked_slice() {
    let slice = RoadmapSlice {
        title: "Vague slice.".to_string(),
        body: "  - Requirement: do important work.\n".to_string(),
    };
    let mut diagnostics = Vec::new();

    validate_unchecked_slice(&slice, &mut diagnostics);

    assert_eq!(diagnostics.len(), 4, "{diagnostics:?}");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("no gate")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("no acceptance")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("stable-failure")));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("positive executable")));
}

/// Verifies the roadmap must keep the global quality rule explicit.
///
/// Inputs:
/// - Roadmap text with a quality rule missing most required enforcement terms.
///
/// Output: diagnostics naming missing quality-rule details.
///
/// Transformation: prevents future roadmap cleanup from dropping the
/// behavioral, adversarial, file-size, or code-smell requirements.
#[test]
fn roadmap_gate_integrity_rejects_weak_quality_rule() {
    let roadmap = r#"
## Quality Enforcement Rule

Every slice needs tests.

## Planned Gates
"#;
    let mut diagnostics = Vec::new();

    validate_quality_enforcement_rule(roadmap, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("real behavior")),
        "expected real behavior diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("rust-quality-check")),
        "expected rust-quality-check diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("code smells")),
        "expected code-smell diagnostic: {diagnostics:?}"
    );
}

/// Verifies the roadmap inventory snapshot accepts current gate counts.
///
/// Inputs:
/// - Roadmap gate section with the exact expected count sentence.
///
/// Output: no diagnostics.
///
/// Transformation: keeps published roadmap counts tied to parsed gate output.
#[test]
fn roadmap_gate_integrity_accepts_current_inventory_snapshot() {
    let roadmap = r#"
### Roadmap Gate Integrity

- [x] Keep every 0.0.7 roadmap requirement executable.
  - Current gate state: current details. Current validated inventory:
    3 planned gates, 2 unchecked slices, and 7 Make targets.
"#;
    let mut diagnostics = Vec::new();

    validate_inventory_snapshot(roadmap, 3, 2, 7, &mut diagnostics);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies stale roadmap inventory snapshots fail.
///
/// Inputs:
/// - Roadmap gate section with obsolete count values.
///
/// Output: diagnostic naming the expected current count sentence.
///
/// Transformation: prevents current-state prose from drifting behind the gate.
#[test]
fn roadmap_gate_integrity_rejects_stale_inventory_snapshot() {
    let roadmap = r#"
### Roadmap Gate Integrity

- [x] Keep every 0.0.7 roadmap requirement executable.
  - Current gate state: current details. Current validated inventory:
    1 planned gates, 1 unchecked slices, and 1 Make targets.
"#;
    let mut diagnostics = Vec::new();

    validate_inventory_snapshot(roadmap, 3, 2, 7, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("3 planned gates")),
        "expected stale inventory diagnostic: {diagnostics:?}"
    );
}

/// Verifies roadmap slice evidence cannot be marker-only.
///
/// Inputs:
/// - Completed slice whose current state claims a symbol-exists check.
///
/// Output: diagnostic naming the weak evidence.
///
/// Transformation: makes roadmap completion require real feature behavior
/// instead of declaration or marker checks.
#[test]
fn roadmap_gate_integrity_rejects_weak_completion_evidence() {
    let slice = RoadmapSlice {
        title: "Weak done slice.".to_string(),
        body: "  - Current gate state: the symbol exists and source files exist.\n".to_string(),
    };
    let mut diagnostics = Vec::new();

    validate_slice_completion_evidence(&slice, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("symbol exists")),
        "expected symbol-exists diagnostic: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("source files exist")),
        "expected source-exists diagnostic: {diagnostics:?}"
    );
}

/// Verifies explicit rejection of weak patterns is allowed.
///
/// Inputs:
/// - Unchecked slice whose acceptance line says the gate rejects marker checks.
///
/// Output: no weak-evidence diagnostics.
///
/// Transformation: lets roadmap requirements describe forbidden evidence
/// without being misread as completion evidence.
#[test]
fn roadmap_gate_integrity_accepts_rejection_language_about_weak_evidence() {
    let slice = RoadmapSlice {
        title: "Strong pending slice.".to_string(),
        body: "  - Acceptance: the gate rejects marker checks and symbol exists checks.\n"
            .to_string(),
    };
    let mut diagnostics = Vec::new();

    validate_slice_completion_evidence(&slice, &mut diagnostics);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies planned gate parsing reads only the Planned Gates fenced block.
///
/// Inputs:
/// - Roadmap text with a different fenced block and a planned gate block.
///
/// Output: only the planned gate command.
///
/// Transformation: avoids treating examples elsewhere in the roadmap as gate
/// requirements.
#[test]
fn roadmap_gate_integrity_parses_planned_gate_block() {
    let roadmap = r#"
```bash
make ignored-check
```

## Planned Gates

```bash
make real-check
```
"#;

    let gates = parse_gate_block(roadmap, PLANNED_GATES_HEADING);

    assert_eq!(gates, strings(["real-check"]));
}

/// Verifies exact multicore gate promotion is accepted.
#[test]
fn roadmap_gate_integrity_accepts_exact_multicore_gate_sequence() {
    let planned = strings(["before-check", "first-check", "second-check", "after-check"]);
    let multicore = strings(["first-check", "second-check"]);
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&planned, &multicore, &mut diagnostics);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

/// Verifies a missing multicore gate is diagnosed by stable identity.
#[test]
fn roadmap_gate_integrity_rejects_missing_multicore_gate() {
    let planned = strings(["first-check"]);
    let multicore = strings(["first-check", "second-check"]);
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&planned, &multicore, &mut diagnostics);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("second-check")));
}

/// Verifies reordering or interleaving the promoted block is rejected.
#[test]
fn roadmap_gate_integrity_rejects_reordered_multicore_gate_sequence() {
    let planned = strings(["second-check", "other-check", "first-check"]);
    let multicore = strings(["first-check", "second-check"]);
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&planned, &multicore, &mut diagnostics);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("exact contiguous order")));
}

/// Verifies duplicate gates cannot hide ambiguous cross-roadmap ownership.
#[test]
fn roadmap_gate_integrity_rejects_duplicate_multicore_gate() {
    let planned = strings(["first-check", "first-check"]);
    let multicore = strings(["first-check"]);
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&planned, &multicore, &mut diagnostics);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("duplicate gates")));
}

/// Verifies an empty mini-roadmap gate inventory fails closed.
#[test]
fn roadmap_gate_integrity_rejects_empty_multicore_gate_sequence() {
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&strings(["first-check"]), &[], &mut diagnostics);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("contains no `make ...` commands")));
}

/// Verifies duplicate mini-roadmap entries identify the owning inventory.
#[test]
fn roadmap_gate_integrity_rejects_duplicate_mini_roadmap_gate() {
    let planned = strings(["first-check"]);
    let multicore = strings(["first-check", "first-check"]);
    let mut diagnostics = Vec::new();

    validate_multicore_gate_sync(&planned, &multicore, &mut diagnostics);

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.contains("multicore complete gate inventory contains duplicate gates")
    }));
}

/// Builds a `BTreeSet<String>` from static names.
fn btreeset<const N: usize>(values: [&str; N]) -> BTreeSet<String> {
    values.into_iter().map(str::to_string).collect()
}

/// Builds an ordered string vector from static names.
fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(str::to_string).collect()
}
