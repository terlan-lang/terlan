use super::core_expr_lowering::core_expr_from_syntax;
use super::core_expr_proof::core_expr_proof_coverage;
use super::core_intrinsic_lowering::core_mutable_receiver_call_expr_from_syntax;
use super::core_pattern_lowering::{
    core_pattern_from_syntax, core_pattern_proof_coverage, core_pattern_summary_text,
};
use super::*;

#[derive(Clone, Default)]
pub(crate) struct CoreProofCoverageCounts {
    pub(crate) lean_covered: usize,
    pub(crate) partial: usize,
    pub(crate) proof_model_required: usize,
    pub(crate) runtime_boundary: usize,
    pub(crate) artifact_only: usize,
}

#[derive(Clone, Default)]
struct CoreExprPayloadCounts {
    typed_core_expr: usize,
    summary_only_expr: usize,
}

#[derive(Clone, Default)]
struct CoreCheckedPreservationCounts {
    expr: usize,
    pattern: usize,
    expr_structural: usize,
    pattern_structural: usize,
    expr_no_runtime_bindings: usize,
    pattern_no_runtime_bindings: usize,
    expr_runtime_bindings_required: usize,
    pattern_runtime_bindings_required: usize,
}

#[derive(Clone, Default)]
struct CorePatternPayloadCounts {
    typed_core_pattern: usize,
    summary_only_pattern: usize,
}

#[derive(Clone, Default)]
pub(crate) struct CoreTypePayloadCounts {
    pub(crate) typed_core_type: usize,
    pub(crate) summary_only_type: usize,
}

#[derive(Clone, Default)]
struct CoreConstructorIdentityCounts {
    resolved_constructor_call_identity: usize,
    resolved_constructor_chain_identity: usize,
    resolved_constructor_pattern_identity: usize,
    unresolved_constructor_call_candidate: usize,
    unresolved_constructor_chain_candidate: usize,
    unresolved_constructor_pattern_candidate: usize,
}

/// Builds CoreIR module metadata from declarations and expression summaries.
///
/// Inputs:
/// - `functions`: Core functions whose clauses may contain expression
///   summaries.
/// - `types`: Core type declarations whose bodies may carry typed Core
///   payloads.
/// - `constructors`: Core constructor declarations whose signature types may
///   carry typed Core payloads.
///
/// Output:
/// - `CoreModuleMetadata` with declaration counts and recursive proof-coverage
///   expression/pattern counts plus typed-payload counts.
///
/// Transformation:
/// - Counts declarations directly, traverses function guards/bodies for
///   expression coverage and typed-payload coverage, counts clause pattern
///   coverage and pattern payload coverage, counts signature type payloads,
///   counts resolved constructor identities, and derives module readiness from
///   the combined coverage buckets.
pub(crate) fn core_module_metadata(
    functions: &[CoreFunction],
    types: &[CoreTypeDecl],
    constructors: &[CoreConstructorDecl],
) -> CoreModuleMetadata {
    let mut expr_coverage = CoreProofCoverageCounts::default();
    let mut expr_payloads = CoreExprPayloadCounts::default();
    let mut checked_counts = CoreCheckedPreservationCounts::default();
    let mut pattern_coverage = CoreProofCoverageCounts::default();
    let mut pattern_payloads = CorePatternPayloadCounts::default();
    let mut type_payloads = CoreTypePayloadCounts::default();
    let mut constructor_identities = CoreConstructorIdentityCounts::default();
    for function in functions {
        count_core_function_type_payloads(function, &mut type_payloads);
        for clause in &function.clauses {
            for coverage in &clause.pattern_proof_coverage {
                count_core_pattern_proof_coverage(*coverage, &mut pattern_coverage);
            }
            count_core_pattern_payloads(&clause.core_patterns, &mut pattern_payloads);
            count_core_function_clause_pattern_constructor_identities(
                &clause.core_patterns,
                &mut constructor_identities,
            );
            count_core_pattern_checked_preservation(
                &clause.pattern_checked_preservation_evidence,
                &mut checked_counts,
            );
            if let Some(guard) = &clause.guard {
                count_core_expr_proof_coverage(guard, &mut expr_coverage);
                count_core_expr_payloads(guard, &mut expr_payloads);
                count_core_expr_checked_preservation(guard, &mut checked_counts);
                count_core_expr_summary_constructor_identities(guard, &mut constructor_identities);
            }
            count_core_expr_proof_coverage(&clause.body, &mut expr_coverage);
            count_core_expr_payloads(&clause.body, &mut expr_payloads);
            count_core_expr_checked_preservation(&clause.body, &mut checked_counts);
            count_core_expr_summary_constructor_identities(
                &clause.body,
                &mut constructor_identities,
            );
        }
    }
    for type_decl in types {
        count_core_type_decl_payloads(type_decl, &mut type_payloads);
    }
    for constructor in constructors {
        count_core_constructor_type_payloads(constructor, &mut type_payloads);
    }
    let combined_coverage = combined_core_proof_coverage(&expr_coverage, &pattern_coverage);

    CoreModuleMetadata {
        interface_function_count: functions.len(),
        interface_type_count: types.len(),
        constructor_count: constructors.len(),
        proof_readiness: core_module_proof_readiness(&combined_coverage, &type_payloads),
        lean_covered_expr_count: expr_coverage.lean_covered,
        partial_expr_count: expr_coverage.partial,
        proof_model_required_expr_count: expr_coverage.proof_model_required,
        runtime_boundary_expr_count: expr_coverage.runtime_boundary,
        artifact_only_expr_count: expr_coverage.artifact_only,
        lean_covered_pattern_count: pattern_coverage.lean_covered,
        partial_pattern_count: pattern_coverage.partial,
        proof_model_required_pattern_count: pattern_coverage.proof_model_required,
        runtime_boundary_pattern_count: pattern_coverage.runtime_boundary,
        artifact_only_pattern_count: pattern_coverage.artifact_only,
        typed_core_expr_count: expr_payloads.typed_core_expr,
        summary_only_expr_count: expr_payloads.summary_only_expr,
        typed_core_pattern_count: pattern_payloads.typed_core_pattern,
        summary_only_pattern_count: pattern_payloads.summary_only_pattern,
        typed_core_type_count: type_payloads.typed_core_type,
        summary_only_type_count: type_payloads.summary_only_type,
        checked_preservation_expr_count: checked_counts.expr,
        checked_preservation_pattern_count: checked_counts.pattern,
        checked_preservation_expr_structural_count: checked_counts.expr_structural,
        checked_preservation_pattern_structural_count: checked_counts.pattern_structural,
        checked_preservation_expr_no_runtime_bindings_count: checked_counts
            .expr_no_runtime_bindings,
        checked_preservation_pattern_no_runtime_bindings_count: checked_counts
            .pattern_no_runtime_bindings,
        checked_preservation_expr_runtime_bindings_required_count: checked_counts
            .expr_runtime_bindings_required,
        checked_preservation_pattern_runtime_bindings_required_count: checked_counts
            .pattern_runtime_bindings_required,
        resolved_constructor_call_identity_count: constructor_identities
            .resolved_constructor_call_identity,
        resolved_constructor_chain_identity_count: constructor_identities
            .resolved_constructor_chain_identity,
        resolved_constructor_pattern_identity_count: constructor_identities
            .resolved_constructor_pattern_identity,
        unresolved_constructor_call_candidate_count: constructor_identities
            .unresolved_constructor_call_candidate,
        unresolved_constructor_chain_candidate_count: constructor_identities
            .unresolved_constructor_chain_candidate,
        unresolved_constructor_pattern_candidate_count: constructor_identities
            .unresolved_constructor_pattern_candidate,
    }
}

/// Adds type-declaration body payloads to typed-payload counts.
///
/// Inputs:
/// - `type_decl`: Core type declaration whose body may carry a typed
///   `CoreType` payload.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts the type declaration body as typed when a `CoreType` payload exists
///   and summary-only when the declaration body remains textual.
fn count_core_type_decl_payloads(type_decl: &CoreTypeDecl, counts: &mut CoreTypePayloadCounts) {
    count_core_type_payload(type_decl.core_body.as_ref(), counts);
}

/// Adds a function signature's Core type payloads to aggregate counts.
///
/// Inputs:
/// - `function`: Core function whose parameter and return annotations may
///   carry typed `CoreType` payloads.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts each function parameter annotation and the function return
///   annotation as typed when a `CoreType` payload exists, otherwise as
///   summary-only.
fn count_core_function_type_payloads(function: &CoreFunction, counts: &mut CoreTypePayloadCounts) {
    for param in &function.params {
        count_core_type_payload(param.core_ty.as_ref(), counts);
    }
    count_core_type_payload(function.core_return_type.as_ref(), counts);
}

/// Adds a constructor signature's Core type payloads to aggregate counts.
///
/// Inputs:
/// - `constructor`: Core constructor whose parameters, optional vararg, and
///   return annotation may carry typed `CoreType` payloads.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts fixed parameters, the optional vararg parameter, and the return
///   annotation as typed when a `CoreType` payload exists, otherwise as
///   summary-only.
fn count_core_constructor_type_payloads(
    constructor: &CoreConstructorDecl,
    counts: &mut CoreTypePayloadCounts,
) {
    for param in &constructor.params {
        count_core_type_payload(param.core_ty.as_ref(), counts);
    }
    if let Some(vararg) = &constructor.vararg {
        count_core_type_payload(vararg.core_ty.as_ref(), counts);
    }
    count_core_type_payload(constructor.core_return_type.as_ref(), counts);
}

/// Adds one optional Core type payload to aggregate counts.
///
/// Inputs:
/// - `ty`: optional typed `CoreType` payload for one signature position.
/// - `counts`: mutable aggregate type-payload counts for the containing Core
///   module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the typed bucket when a Core type payload exists and the
///   summary-only bucket when the signature position is still textual only.
fn count_core_type_payload(ty: Option<&CoreType>, counts: &mut CoreTypePayloadCounts) {
    if ty.is_some() {
        counts.typed_core_type += 1;
    } else {
        counts.summary_only_type += 1;
    }
}

/// Combines expression and pattern proof-coverage counts.
///
/// Inputs:
/// - `expr_coverage`: aggregate counts from Core expression summaries.
/// - `pattern_coverage`: aggregate counts from Core pattern summaries.
///
/// Output:
/// - Combined proof-coverage counts for module readiness decisions.
///
/// Transformation:
/// - Adds each coverage bucket pairwise while preserving separate source
///   counters on `CoreModuleMetadata`.
fn combined_core_proof_coverage(
    expr_coverage: &CoreProofCoverageCounts,
    pattern_coverage: &CoreProofCoverageCounts,
) -> CoreProofCoverageCounts {
    CoreProofCoverageCounts {
        lean_covered: expr_coverage.lean_covered + pattern_coverage.lean_covered,
        partial: expr_coverage.partial + pattern_coverage.partial,
        proof_model_required: expr_coverage.proof_model_required
            + pattern_coverage.proof_model_required,
        runtime_boundary: expr_coverage.runtime_boundary + pattern_coverage.runtime_boundary,
        artifact_only: expr_coverage.artifact_only + pattern_coverage.artifact_only,
    }
}

/// Derives a module-level proof readiness label from coverage counts.
///
/// Inputs:
/// - `coverage`: aggregate proof-coverage counts for a Core module.
///
/// Output:
/// - Conservative module readiness label.
///
/// Transformation:
/// - Chooses the most restrictive present label, with runtime-boundary and
///   partial forms taking precedence over proof-model work; returns
///   `NoExpressions` for modules without expression or pattern summaries.
pub(crate) fn core_proof_readiness(coverage: &CoreProofCoverageCounts) -> CoreProofReadiness {
    if coverage.runtime_boundary > 0 {
        CoreProofReadiness::RuntimeBoundary
    } else if coverage.partial > 0 {
        CoreProofReadiness::Partial
    } else if coverage.proof_model_required > 0 {
        CoreProofReadiness::ProofModelRequired
    } else if coverage.artifact_only > 0 {
        CoreProofReadiness::ArtifactOnly
    } else if coverage.lean_covered > 0 {
        CoreProofReadiness::LeanCovered
    } else {
        CoreProofReadiness::NoExpressions
    }
}

/// Derives module-level readiness from term coverage and type payload debt.
///
/// Inputs:
/// - `coverage`: aggregate expression and pattern proof-coverage counts.
/// - `type_payloads`: aggregate CoreType payload counts for type declarations,
///   function signatures, and constructor signatures.
///
/// Output:
/// - Conservative module readiness label.
///
/// Transformation:
/// - Starts from expression/pattern readiness, then promotes otherwise covered
///   or expression-free modules to `ProofModelRequired` when any type position
///   remains summary-only.
pub(crate) fn core_module_proof_readiness(
    coverage: &CoreProofCoverageCounts,
    type_payloads: &CoreTypePayloadCounts,
) -> CoreProofReadiness {
    let readiness = core_proof_readiness(coverage);
    if type_payloads.summary_only_type == 0 {
        return readiness;
    }
    match readiness {
        CoreProofReadiness::RuntimeBoundary
        | CoreProofReadiness::Partial
        | CoreProofReadiness::ProofModelRequired => readiness,
        CoreProofReadiness::ArtifactOnly
        | CoreProofReadiness::LeanCovered
        | CoreProofReadiness::NoExpressions => CoreProofReadiness::ProofModelRequired,
    }
}

/// Adds one expression summary tree to proof-coverage counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Records the current expression's proof-coverage label and recursively
///   visits child expression summaries.
fn count_core_expr_proof_coverage(expr: &CoreExprSummary, counts: &mut CoreProofCoverageCounts) {
    match expr.proof_coverage {
        CoreProofCoverage::LeanCovered => counts.lean_covered += 1,
        CoreProofCoverage::Partial => counts.partial += 1,
        CoreProofCoverage::ProofModelRequired => counts.proof_model_required += 1,
        CoreProofCoverage::RuntimeBoundary => counts.runtime_boundary += 1,
        CoreProofCoverage::ArtifactOnly => counts.artifact_only += 1,
    }
    for child in &expr.children {
        count_core_expr_proof_coverage(child, counts);
    }
}

/// Adds one expression summary tree to typed-payload counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate payload counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Records whether the current expression summary has a typed `CoreExpr`
///   payload and recursively visits child expression summaries.
fn count_core_expr_payloads(expr: &CoreExprSummary, counts: &mut CoreExprPayloadCounts) {
    if expr.core_expr.is_some() {
        counts.typed_core_expr += 1;
    } else {
        counts.summary_only_expr += 1;
    }
    for child in &expr.children {
        count_core_expr_payloads(child, counts);
    }
}

/// Adds one expression summary tree to checked-preservation-evidence counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to count.
/// - `counts`: mutable aggregate checked-preservation counts for the containing
///   Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Marks the current expression summary as evidence-backed when its typed
///   `CoreExpr` can be shown to satisfy recursive checked-preservation
///   conditions and recursively checks all child summaries.
fn count_core_expr_checked_preservation(
    expr: &CoreExprSummary,
    counts: &mut CoreCheckedPreservationCounts,
) {
    if let Some(evidence) = &expr.checked_preservation_evidence {
        counts.expr += 1;
        if matches!(
            evidence.kind,
            CoreCheckedPreservationEvidenceKind::StructuralCoreExpr
        ) {
            counts.expr_structural += 1;
        }
        match evidence.freshness {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => {
                counts.expr_no_runtime_bindings += 1;
            }
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                counts.expr_runtime_bindings_required += 1;
            }
        }
    }
    for child in &expr.children {
        count_core_expr_checked_preservation(child, counts);
    }
}

/// Builds checked-preservation evidence for a typed Core expression.
///
/// Inputs:
/// - `expr`: typed Core expression payload attached to a CoreIR summary.
///
/// Output:
/// - `Some(CoreCheckedPreservationEvidence)` when the expression and all
///   recursive children satisfy the current checked-preservation predicate.
/// - `None` when the expression has no checked-preservation evidence yet.
///
/// Transformation:
/// - Reuses the structural evidence predicate, then records the covered Core
///   term as deterministic Core contract text for future Lean export.
fn core_expr_checked_preservation_evidence(
    expr: &CoreExpr,
) -> Option<CoreCheckedPreservationEvidence> {
    core_expr_has_checked_preservation_evidence(expr).then(|| CoreCheckedPreservationEvidence {
        kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
        freshness: core_expr_substitution_freshness_evidence(expr),
        target: expr.contract_text(),
    })
}

/// Classifies the runtime substitution-freshness obligation for an expression.
///
/// Inputs:
/// - `expr`: typed Core expression that already has structural preservation
///   evidence.
///
/// Output:
/// - Conservative freshness obligation for future Lean export.
///
/// Transformation:
/// - Recursively joins nested expression and pattern obligations, marking
///   expression forms that can bind runtime values (`case`, `try`,
///   comprehensions, lambdas) as requiring runtime binding freshness whenever
///   their patterns bind names.
fn core_expr_substitution_freshness_evidence(expr: &CoreExpr) -> CoreSubstitutionFreshnessEvidence {
    let none = CoreSubstitutionFreshnessEvidence::NoRuntimeBindings;
    match expr {
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => none,
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            combine_expr_freshness(items.iter().map(core_expr_substitution_freshness_evidence))
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            combine_expr_freshness(args.iter().map(core_expr_substitution_freshness_evidence))
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            core_expr_substitution_freshness_evidence(receiver).combine(combine_expr_freshness(
                args.iter().map(core_expr_substitution_freshness_evidence),
            ))
        }
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_substitution_freshness_evidence(callee).combine(combine_expr_freshness(
                args.iter().map(core_expr_substitution_freshness_evidence),
            ))
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => core_expr_substitution_freshness_evidence(head)
            .combine(core_expr_substitution_freshness_evidence(tail)),
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => core_expr_substitution_freshness_evidence(expr)
            .combine(core_pattern_substitution_freshness_evidence(pattern))
            .combine(core_expr_substitution_freshness_evidence(source))
            .combine(
                guard
                    .as_ref()
                    .map(|guard| core_expr_substitution_freshness_evidence(guard))
                    .unwrap_or(none),
            ),
        CoreExpr::Let { bindings, body } => combine_expr_freshness(
            bindings
                .iter()
                .map(|binding| core_expr_substitution_freshness_evidence(&binding.value)),
        )
        .combine(core_expr_substitution_freshness_evidence(body)),
        CoreExpr::Map(fields) => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
        ),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            combine_expr_freshness(
                fields
                    .iter()
                    .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
            )
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            core_expr_substitution_freshness_evidence(base)
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            core_expr_substitution_freshness_evidence(base).combine(combine_expr_freshness(
                fields
                    .iter()
                    .map(|field| core_expr_substitution_freshness_evidence(&field.value)),
            ))
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            combine_expr_freshness(args.iter().map(core_expr_substitution_freshness_evidence))
                .combine(core_expr_substitution_freshness_evidence(record))
        }
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_substitution_freshness_evidence(scrutinee).combine(combine_expr_freshness(
                clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => core_expr_substitution_freshness_evidence(body)
            .combine(combine_expr_freshness(
                of_clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
            .combine(combine_expr_freshness(
                catch_clauses
                    .iter()
                    .map(core_case_clause_substitution_freshness_evidence),
            ))
            .combine(
                after_clause
                    .as_ref()
                    .map(core_try_after_substitution_freshness_evidence)
                    .unwrap_or(none),
            ),
        CoreExpr::If { clauses } => combine_expr_freshness(
            clauses
                .iter()
                .map(core_if_clause_substitution_freshness_evidence),
        ),
        CoreExpr::Lam { params, body } => combine_expr_freshness(
            params
                .iter()
                .map(core_pattern_substitution_freshness_evidence),
        )
        .combine(core_expr_substitution_freshness_evidence(body)),
    }
}

/// Combines an iterator of expression or pattern freshness obligations.
///
/// Inputs:
/// - `items`: freshness obligations from nested Core payloads.
///
/// Output:
/// - Aggregate freshness obligation for the enclosing Core payload.
///
/// Transformation:
/// - Starts from `NoRuntimeBindings` and joins every nested obligation using
///   the conservative freshness lattice.
fn combine_expr_freshness(
    items: impl IntoIterator<Item = CoreSubstitutionFreshnessEvidence>,
) -> CoreSubstitutionFreshnessEvidence {
    items.into_iter().fold(
        CoreSubstitutionFreshnessEvidence::NoRuntimeBindings,
        |acc, item| acc.combine(item),
    )
}

/// Classifies substitution freshness for a Core case-like clause.
///
/// Inputs:
/// - `clause`: typed case/try clause.
///
/// Output:
/// - Aggregate freshness obligation for the clause.
///
/// Transformation:
/// - Joins the pattern, optional guard, and body obligations so pattern
///   bindings are visible to future Lean export.
fn core_case_clause_substitution_freshness_evidence(
    clause: &CoreCaseClause,
) -> CoreSubstitutionFreshnessEvidence {
    core_pattern_substitution_freshness_evidence(&clause.pattern)
        .combine(
            clause
                .guard
                .as_ref()
                .map(core_expr_substitution_freshness_evidence)
                .unwrap_or(CoreSubstitutionFreshnessEvidence::NoRuntimeBindings),
        )
        .combine(core_expr_substitution_freshness_evidence(&clause.body))
}

/// Classifies substitution freshness for a Core if clause.
///
/// Inputs:
/// - `clause`: typed if condition/body pair.
///
/// Output:
/// - Aggregate freshness obligation for the clause.
///
/// Transformation:
/// - Joins condition and body obligations without adding new binding
///   obligations, since `if` does not bind runtime pattern names.
fn core_if_clause_substitution_freshness_evidence(
    clause: &CoreIfClause,
) -> CoreSubstitutionFreshnessEvidence {
    core_expr_substitution_freshness_evidence(&clause.condition)
        .combine(core_expr_substitution_freshness_evidence(&clause.body))
}

/// Classifies substitution freshness for a Core try cleanup branch.
///
/// Inputs:
/// - `after_clause`: typed try cleanup trigger/body pair.
///
/// Output:
/// - Aggregate freshness obligation for the cleanup branch.
///
/// Transformation:
/// - Joins trigger and body obligations without adding new binding
///   obligations.
fn core_try_after_substitution_freshness_evidence(
    after_clause: &CoreTryAfter,
) -> CoreSubstitutionFreshnessEvidence {
    core_expr_substitution_freshness_evidence(&after_clause.trigger).combine(
        core_expr_substitution_freshness_evidence(&after_clause.body),
    )
}

/// Checks whether a typed Core expression carries checked-preservation evidence.
///
/// Inputs:
/// - `expr`: typed Core expression to validate.
///
/// Output:
/// - `true` when the term and all recursive children are in the evidence-backed
///   covered subset.
///
/// Transformation:
/// - Applies structural recursion over the current covered Core expression
///   constructors (`Int`/`Atom`/`Var`/`Tuple`/`List`/`Call`/`Case`/`Lam`).
fn core_expr_has_checked_preservation_evidence(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Int(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => true,
        CoreExpr::Float(_) => true,
        CoreExpr::Binary(_) => true,
        CoreExpr::Tuple(items) | CoreExpr::List(items) => items
            .iter()
            .all(core_expr_has_checked_preservation_evidence),
        CoreExpr::ListCons { head, tail } => {
            core_expr_has_checked_preservation_evidence(head)
                && core_expr_has_checked_preservation_evidence(tail)
        }
        CoreExpr::FixedArray(items) => items
            .iter()
            .all(core_expr_has_checked_preservation_evidence),
        CoreExpr::Index { base, index } => {
            core_expr_has_checked_preservation_evidence(base)
                && core_expr_has_checked_preservation_evidence(index)
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            core_expr_has_checked_preservation_evidence(expr)
                && core_pattern_has_checked_preservation_evidence(pattern)
                && core_expr_has_checked_preservation_evidence(source)
                && guard
                    .as_ref()
                    .is_none_or(|guard| core_expr_has_checked_preservation_evidence(guard))
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .all(|binding| core_expr_has_checked_preservation_evidence(&binding.value))
                && core_expr_has_checked_preservation_evidence(body)
        }
        CoreExpr::Map(fields) => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::FieldAccess { base, .. } => core_expr_has_checked_preservation_evidence(base),
        CoreExpr::RecordAccess { base, .. } => core_expr_has_checked_preservation_evidence(base),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            core_expr_has_checked_preservation_evidence(base)
                && fields
                    .iter()
                    .all(|field| core_expr_has_checked_preservation_evidence(&field.value))
        }
        CoreExpr::TemplateInstantiate { fields, .. } => fields
            .iter()
            .all(|field| core_expr_has_checked_preservation_evidence(&field.value)),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
                && core_expr_has_checked_preservation_evidence(record)
        }
        CoreExpr::RemoteFunRef { .. } => true,
        CoreExpr::RemoteCall { args, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::ConstructorCall { args, .. } => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Call { args, .. } => args.iter().all(core_expr_has_checked_preservation_evidence),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            core_expr_has_checked_preservation_evidence(receiver)
                && args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_has_checked_preservation_evidence(callee)
                && args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Intrinsic(CoreIntrinsicCall { args, .. }) => {
            args.iter().all(core_expr_has_checked_preservation_evidence)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_has_checked_preservation_evidence(scrutinee)
                && clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            core_expr_has_checked_preservation_evidence(body)
                && of_clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
                && catch_clauses
                    .iter()
                    .all(core_case_clause_has_checked_preservation_evidence)
                && after_clause
                    .as_ref()
                    .is_none_or(core_try_after_has_checked_preservation_evidence)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .all(core_if_clause_has_checked_preservation_evidence),
        CoreExpr::Lam { params, body } => {
            params
                .iter()
                .all(core_pattern_has_checked_preservation_evidence)
                && core_expr_has_checked_preservation_evidence(body)
        }
        CoreExpr::UnaryOp { operand, .. } => core_expr_has_checked_preservation_evidence(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            core_expr_has_checked_preservation_evidence(left)
                && core_expr_has_checked_preservation_evidence(right)
        }
    }
}

/// Checks whether a Core case clause has checked-preservation evidence.
///
/// Inputs:
/// - `clause`: typed case clause with one pattern and a body expression.
///
/// Output:
/// - `true` when both pattern and body are evidence-backed.
///
/// Transformation:
/// - Recursively validates the clause pattern and body using the same proof
///   evidence predicates as expression-level coverage.
fn core_case_clause_has_checked_preservation_evidence(clause: &CoreCaseClause) -> bool {
    core_pattern_has_checked_preservation_evidence(&clause.pattern)
        && clause
            .guard
            .as_ref()
            .is_none_or(core_expr_has_checked_preservation_evidence)
        && core_expr_has_checked_preservation_evidence(&clause.body)
}

/// Checks whether a Core if clause has checked-preservation evidence.
///
/// Inputs:
/// - `clause`: typed if clause with a condition and body expression.
///
/// Output:
/// - `true` when both condition and body expressions are evidence-backed.
///
/// Transformation:
/// - Recursively validates the condition and body using the expression-level
///   checked-preservation predicate.
fn core_if_clause_has_checked_preservation_evidence(clause: &CoreIfClause) -> bool {
    core_expr_has_checked_preservation_evidence(&clause.condition)
        && core_expr_has_checked_preservation_evidence(&clause.body)
}

/// Checks whether a Core try cleanup branch has preservation evidence.
///
/// Inputs:
/// - `after_clause`: typed try cleanup trigger/body payload.
///
/// Output:
/// - `true` when both cleanup trigger and body are evidence-backed.
///
/// Transformation:
/// - Recursively validates trigger and body expressions with the expression
///   checked-preservation predicate.
fn core_try_after_has_checked_preservation_evidence(after_clause: &CoreTryAfter) -> bool {
    core_expr_has_checked_preservation_evidence(&after_clause.trigger)
        && core_expr_has_checked_preservation_evidence(&after_clause.body)
}

/// Adds function-clause pattern payloads to typed-payload counts.
///
/// Inputs:
/// - `patterns`: optional typed Core pattern payloads for one function clause.
/// - `counts`: mutable aggregate pattern payload counts for the containing
///   Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts each top-level function-clause pattern as typed when a
///   `CorePattern` payload exists, otherwise as summary-only.
fn count_core_pattern_payloads(
    patterns: &[Option<CorePattern>],
    counts: &mut CorePatternPayloadCounts,
) {
    for pattern in patterns {
        if pattern.is_some() {
            counts.typed_core_pattern += 1;
        } else {
            counts.summary_only_pattern += 1;
        }
    }
}

/// Adds top-level function-clause constructor-pattern identities to counts.
///
/// Inputs:
/// - `patterns`: optional typed Core pattern payloads for one function clause.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Traverses each typed function-clause pattern and records resolved
///   constructor-pattern identity and unresolved-candidate buckets without
///   affecting proof coverage.
fn count_core_function_clause_pattern_constructor_identities(
    patterns: &[Option<CorePattern>],
    counts: &mut CoreConstructorIdentityCounts,
) {
    for pattern in patterns.iter().flatten() {
        count_core_pattern_constructor_identities(pattern, counts);
    }
}

/// Adds constructor identities from an expression-summary tree to counts.
///
/// Inputs:
/// - `expr`: Core expression summary tree to scan.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts constructor-resolution evidence owned by the current summary's
///   typed Core node, then recurses through summary children. The current-node
///   scan does not recurse into nested expressions because those have their
///   own summary entries; this avoids double-counting expression candidates.
fn count_core_expr_summary_constructor_identities(
    expr: &CoreExprSummary,
    counts: &mut CoreConstructorIdentityCounts,
) {
    if let Some(core_expr) = &expr.core_expr {
        count_core_expr_local_constructor_identities(core_expr, counts);
    }
    for child in &expr.children {
        count_core_expr_summary_constructor_identities(child, counts);
    }
}

/// Adds constructor identities owned directly by one Core expression node.
///
/// Inputs:
/// - `expr`: typed Core expression at one expression-summary node.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Counts resolved and unresolved constructor-call/constructor-chain
///   candidates on the expression itself, and scans embedded pattern positions
///   owned by the expression node. Nested expression children are counted by
///   their own expression-summary entries.
fn count_core_expr_local_constructor_identities(
    expr: &CoreExpr,
    counts: &mut CoreConstructorIdentityCounts,
) {
    match expr {
        CoreExpr::ConstructorCall {
            constructor_identity,
            ..
        } => {
            if constructor_identity.is_some() {
                counts.resolved_constructor_call_identity += 1;
            } else {
                counts.unresolved_constructor_call_candidate += 1;
            }
        }
        CoreExpr::ConstructorChain {
            base_constructor_identity,
            ..
        } => {
            if base_constructor_identity.is_some() {
                counts.resolved_constructor_chain_identity += 1;
            } else {
                counts.unresolved_constructor_chain_candidate += 1;
            }
        }
        CoreExpr::ListComprehension { pattern, .. } => {
            count_core_pattern_constructor_identities(pattern, counts);
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                count_core_expr_local_constructor_identities(&binding.value, counts);
            }
            count_core_expr_local_constructor_identities(body, counts);
        }
        CoreExpr::Case { clauses, .. } => {
            for clause in clauses {
                count_core_pattern_constructor_identities(&clause.pattern, counts);
            }
        }
        CoreExpr::Try {
            of_clauses,
            catch_clauses,
            ..
        } => {
            for clause in of_clauses.iter().chain(catch_clauses) {
                count_core_pattern_constructor_identities(&clause.pattern, counts);
            }
        }
        CoreExpr::Lam { params, .. } => {
            for param in params {
                count_core_pattern_constructor_identities(param, counts);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::Tuple(_)
        | CoreExpr::List(_)
        | CoreExpr::ListCons { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::FieldAccess { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::RemoteCall { .. }
        | CoreExpr::Intrinsic(_)
        | CoreExpr::Call { .. }
        | CoreExpr::MutableReceiverCall { .. }
        | CoreExpr::FunctionCall { .. }
        | CoreExpr::If { .. }
        | CoreExpr::UnaryOp { .. }
        | CoreExpr::BinaryOp { .. }
        | CoreExpr::FixedArray(_)
        | CoreExpr::Index { .. } => {}
    }
}

/// Adds constructor-pattern resolution buckets from one Core pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern to scan.
/// - `counts`: mutable aggregate constructor-identity counters for the
///   containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Recursively scans structural pattern positions and increments either the
///   resolved identity bucket or unresolved candidate bucket for each
///   constructor pattern.
fn count_core_pattern_constructor_identities(
    pattern: &CorePattern,
    counts: &mut CoreConstructorIdentityCounts,
) {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                count_core_pattern_constructor_identities(element, counts);
            }
        }
        CorePattern::ListCons { head, tail } => {
            count_core_pattern_constructor_identities(head, counts);
            count_core_pattern_constructor_identities(tail, counts);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                count_core_pattern_constructor_identities(&field.value, counts);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                count_core_pattern_constructor_identities(&field.value, counts);
            }
        }
        CorePattern::Constructor {
            constructor_identity,
            args,
            ..
        } => {
            if constructor_identity.is_some() {
                counts.resolved_constructor_pattern_identity += 1;
            } else {
                counts.unresolved_constructor_pattern_candidate += 1;
            }
            for arg in args {
                count_core_pattern_constructor_identities(arg, counts);
            }
        }
    }
}

/// Adds one function-clause pattern summary vector to checked-preservation counts.
///
/// Inputs:
/// - `pattern_checked_preservation_evidence`: top-level function-clause
///   pattern evidence payloads for one function clause.
/// - `counts`: mutable aggregate checked-preservation counters.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the pattern bucket once per pattern that has an explicit
///   checked-preservation evidence payload.
fn count_core_pattern_checked_preservation(
    pattern_checked_preservation_evidence: &[Option<CoreCheckedPreservationEvidence>],
    counts: &mut CoreCheckedPreservationCounts,
) {
    for evidence in pattern_checked_preservation_evidence {
        if let Some(evidence) = evidence {
            counts.pattern += 1;
            if matches!(
                evidence.kind,
                CoreCheckedPreservationEvidenceKind::StructuralCorePattern
            ) {
                counts.pattern_structural += 1;
            }
            match evidence.freshness {
                CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => {
                    counts.pattern_no_runtime_bindings += 1;
                }
                CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                    counts.pattern_runtime_bindings_required += 1;
                }
            }
        }
    }
}

/// Builds checked-preservation evidence for a typed Core pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern payload attached to a top-level function
///   clause pattern summary.
///
/// Output:
/// - `Some(CoreCheckedPreservationEvidence)` when the pattern and all recursive
///   children satisfy the current checked-preservation predicate.
/// - `None` when the pattern has no checked-preservation evidence yet.
///
/// Transformation:
/// - Reuses the structural pattern evidence predicate, then records the
///   covered Core pattern as deterministic Core contract text for future Lean
///   export.
fn core_pattern_checked_preservation_evidence(
    pattern: &CorePattern,
) -> Option<CoreCheckedPreservationEvidence> {
    core_pattern_has_checked_preservation_evidence(pattern).then(|| {
        CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
            freshness: core_pattern_substitution_freshness_evidence(pattern),
            target: pattern.contract_text(),
        }
    })
}

/// Classifies the runtime substitution-freshness obligation for a pattern.
///
/// Inputs:
/// - `pattern`: typed Core pattern that already has structural preservation
///   evidence.
///
/// Output:
/// - Conservative freshness obligation for future Lean export.
///
/// Transformation:
/// - Marks variable patterns as requiring runtime binding freshness and joins
///   nested obligations for compound patterns; literal/wildcard patterns do
///   not introduce runtime bindings.
fn core_pattern_substitution_freshness_evidence(
    pattern: &CorePattern,
) -> CoreSubstitutionFreshnessEvidence {
    let none = CoreSubstitutionFreshnessEvidence::NoRuntimeBindings;
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => none,
        CorePattern::Var(_) => CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
        CorePattern::Tuple(elements) | CorePattern::List(elements) => combine_expr_freshness(
            elements
                .iter()
                .map(core_pattern_substitution_freshness_evidence),
        ),
        CorePattern::ListCons { head, tail } => core_pattern_substitution_freshness_evidence(head)
            .combine(core_pattern_substitution_freshness_evidence(tail)),
        CorePattern::Map(fields) => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_pattern_substitution_freshness_evidence(&field.value)),
        ),
        CorePattern::Record { fields, .. } => combine_expr_freshness(
            fields
                .iter()
                .map(|field| core_pattern_substitution_freshness_evidence(&field.value)),
        ),
        CorePattern::Constructor { args, .. } => combine_expr_freshness(
            args.iter()
                .map(core_pattern_substitution_freshness_evidence),
        ),
    }
}

/// Checks whether a typed Core pattern carries checked-preservation evidence.
///
/// Inputs:
/// - `pattern`: typed Core pattern to validate.
///
/// Output:
/// - `true` when all recursive pieces are evidence-backed in the covered
///   subset.
///
/// Transformation:
/// - Applies structural recursion over covered pattern constructors.
fn core_pattern_has_checked_preservation_evidence(pattern: &CorePattern) -> bool {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => true,
        CorePattern::Tuple(elements) | CorePattern::List(elements) => elements
            .iter()
            .all(core_pattern_has_checked_preservation_evidence),
        CorePattern::ListCons { head, tail } => {
            core_pattern_has_checked_preservation_evidence(head)
                && core_pattern_has_checked_preservation_evidence(tail)
        }
        CorePattern::Map(fields) => fields
            .iter()
            .all(|field| core_pattern_has_checked_preservation_evidence(&field.value)),
        CorePattern::Record { fields, .. } => fields
            .iter()
            .all(|field| core_pattern_has_checked_preservation_evidence(&field.value)),
        CorePattern::Constructor { args, .. } => args
            .iter()
            .all(core_pattern_has_checked_preservation_evidence),
    }
}

/// Adds one pattern proof-coverage label to aggregate counts.
///
/// Inputs:
/// - `coverage`: proof-coverage label attached to a Core pattern summary.
/// - `counts`: mutable aggregate counts for the containing Core module.
///
/// Output:
/// - None; `counts` is updated in place.
///
/// Transformation:
/// - Increments the matching coverage bucket without inspecting rendered
///   pattern text.
fn count_core_pattern_proof_coverage(
    coverage: CoreProofCoverage,
    counts: &mut CoreProofCoverageCounts,
) {
    match coverage {
        CoreProofCoverage::LeanCovered => counts.lean_covered += 1,
        CoreProofCoverage::Partial => counts.partial += 1,
        CoreProofCoverage::ProofModelRequired => counts.proof_model_required += 1,
        CoreProofCoverage::RuntimeBoundary => counts.runtime_boundary += 1,
        CoreProofCoverage::ArtifactOnly => counts.artifact_only += 1,
    }
}

/// Collects explicit source imports into deterministic CoreIR import summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted Core import summaries for imports written in source.
///
/// Transformation:
/// - Converts syntax-output import declarations into backend-neutral module
///   imports and excludes implicit/builtin interface-map entries.
pub(crate) fn core_syntax_imports(module: &SyntaxModuleOutput) -> Vec<CoreImport> {
    let mut imports = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind,
                module_name,
                items,
                source_path,
                ..
            } => Some(CoreImport {
                module: core_import_identity(import_kind, module_name, items, source_path),
                kind: core_import_kind(*import_kind),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
    imports
}

/// Collects provider modules that were resolved through type or trait imports.
///
/// Inputs:
/// - `resolved`: resolver artifact containing imported type and trait aliases.
///
/// Output:
/// - Core module imports for the actual provider modules backing those aliases.
///
/// Transformation:
/// - Converts alias-level resolver facts such as `Task -> std.core.Task.Task`
///   into module-level CoreIR imports such as `std.core.Task`. This preserves
///   default-export imports in target-profile validation without relying on the
///   raw parser split between module prefixes and imported symbols.
pub(crate) fn core_resolved_imported_modules(resolved: &ResolvedModule) -> Vec<CoreImport> {
    let mut imports = resolved
        .imported_types
        .values()
        .chain(resolved.imported_traits.values())
        .map(|imported| CoreImport {
            module: imported.source_module.clone(),
            kind: CoreImportKind::Module,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| left.module.cmp(&right.module));
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
    imports
}

/// Merges CoreIR imports while preserving deterministic order and uniqueness.
///
/// Inputs:
/// - `imports`: mutable base import list.
/// - `extra`: additional imports discovered after initial syntax lowering.
///
/// Output:
/// - No direct return value; `imports` is sorted and deduplicated in place.
///
/// Transformation:
/// - Appends resolved-provider imports to syntax imports, then normalizes by
///   module identity and import kind so contract text remains stable.
pub(crate) fn merge_core_imports(imports: &mut Vec<CoreImport>, extra: Vec<CoreImport>) {
    imports.extend(extra);
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
}

/// Collects source trait conformance facts into deterministic CoreIR summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted, deduplicated Core trait conformance summaries.
///
/// Transformation:
/// - Converts declaration-site `implements` and explicit `impl Trait for Type`
///   blocks into backend-neutral conformance facts while preserving source
///   category and visibility. Struct `derives` is not included because it is
///   struct-to-struct shape derivation, not trait conformance.
pub(crate) fn core_syntax_trait_conformances(
    module: &SyntaxModuleOutput,
) -> Vec<CoreTraitConformance> {
    let mut conformances = Vec::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                implements,
                ..
            }
            | SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                implements,
                ..
            } => {
                conformances.extend(implements.iter().map(|trait_ref| CoreTraitConformance {
                    trait_ref: normalize_trait_type_text(&trait_ref.text),
                    for_type: name.clone(),
                    source: CoreTraitConformanceSource::Implements,
                    public: *is_public,
                }));
            }
            _ => {}
        }

        if let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } = &declaration.payload
        {
            conformances.push(CoreTraitConformance {
                trait_ref: normalize_trait_type_text(&trait_ref.text),
                for_type: normalize_trait_type_text(&for_type.text),
                source: CoreTraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort_by(|left, right| {
        left.trait_ref
            .cmp(&right.trait_ref)
            .then_with(|| left.for_type.cmp(&right.for_type))
            .then_with(|| format!("{:?}", left.source).cmp(&format!("{:?}", right.source)))
            .then_with(|| left.public.cmp(&right.public))
    });
    conformances.dedup();
    conformances
}

/// Converts syntax-output import kind into CoreIR import kind.
///
/// Inputs:
/// - `kind`: parser-preserved syntax import kind.
///
/// Output:
/// - Matching CoreIR import kind.
///
/// Transformation:
/// - Copies the import family tag while keeping target resolver behavior out of
///   CoreIR.
fn core_import_kind(kind: SyntaxImportKind) -> CoreImportKind {
    match kind {
        SyntaxImportKind::Module => CoreImportKind::Module,
        SyntaxImportKind::File => CoreImportKind::File,
        SyntaxImportKind::Css => CoreImportKind::Css,
        SyntaxImportKind::Markdown => CoreImportKind::Markdown,
    }
}

/// Builds a stable CoreIR identity for a syntax import declaration.
///
/// Inputs:
/// - `kind`: syntax import family.
/// - `module_name`: dotted module path for normal imports.
/// - `items`: imported items or asset alias.
/// - `source_path`: asset source path when present.
///
/// Output:
/// - Import identity string used by CoreIR contract text and target validation.
///
/// Transformation:
/// - Keeps module imports keyed by module path and asset imports keyed by
///   `alias<-source` so multiple assets remain distinguishable without reading
///   the filesystem.
fn core_import_identity(
    kind: &SyntaxImportKind,
    module_name: &str,
    items: &[terlan_syntax::SyntaxImportItem],
    source_path: &Option<String>,
) -> String {
    match kind {
        SyntaxImportKind::Module => module_name.to_string(),
        SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown => {
            let alias = items
                .first()
                .map(|item| item.name.as_str())
                .unwrap_or("<missing-alias>");
            let source = source_path.as_deref().unwrap_or("<missing-source>");
            format!("{alias}<-{source}")
        }
    }
}

/// Collects CoreIR function clause summaries from syntax output.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Map keyed by function name and arity.
///
/// Transformation:
/// - Converts syntax-output clause patterns, guards, and bodies into stable
///   backend-neutral summaries for the initial CoreIR lowering slice.
pub(crate) fn core_syntax_function_clauses(
    module: &SyntaxModuleOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) -> HashMap<(String, usize), Vec<CoreFunctionClause>> {
    let mut clauses = HashMap::new();
    for declaration in &module.declarations {
        if let SyntaxDeclarationPayload::Function {
            name,
            params,
            clauses: function_clauses,
            ..
        } = &declaration.payload
        {
            clauses.insert(
                (name.clone(), params.len()),
                function_clauses
                    .iter()
                    .map(|clause| core_function_clause_summary(clause, receiver_methods))
                    .collect(),
            );
        }
    }
    clauses
}

/// Annotates syntax-lowered Core clauses with resolved constructor identities.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
/// - `constructor_identities`: local constructor names mapped to stable
///   semantic constructor identities.
///
/// Output:
/// - None; constructor-call candidate payloads are updated in place.
///
/// Transformation:
/// - Recursively annotates `CoreExpr::ConstructorCall`,
///   `CoreExpr::ConstructorChain`, and `CorePattern::Constructor` nodes whose
///   candidate name resolves in the current module, an eligible single-shape
///   type alias, or imported public constructor/type-alias surface. Unknown
///   uppercase calls and patterns remain candidate-only.
pub(crate) fn resolve_constructor_identities_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
    constructor_identities: &HashMap<String, String>,
) {
    if constructor_identities.is_empty() {
        return;
    }

    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for pattern in clause.core_patterns.iter_mut().flatten() {
                resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            }
            if let Some(guard) = &mut clause.guard {
                resolve_constructor_identities_in_expr_summary(guard, constructor_identities);
            }
            resolve_constructor_identities_in_expr_summary(
                &mut clause.body,
                constructor_identities,
            );
        }
    }
}

/// Refreshes proof evidence after Core payload annotation.
///
/// Inputs:
/// - `clauses`: mutable syntax-output Core function-clause summaries.
///
/// Output:
/// - None; evidence payloads and annotation-dependent proof labels are updated
///   in place.
///
/// Transformation:
/// - Recomputes expression-summary and top-level pattern preservation evidence
///   from final typed Core payloads after semantic annotation passes have
///   changed Core contract text, such as constructor identity resolution.
/// - Recomputes proof coverage for forms whose coverage depends on final
///   semantic annotation, such as resolved constructor calls.
pub(crate) fn refresh_core_evidence_in_function_clauses(
    clauses: &mut HashMap<(String, usize), Vec<CoreFunctionClause>>,
) {
    for function_clauses in clauses.values_mut() {
        for clause in function_clauses {
            for (evidence, pattern) in clause
                .pattern_checked_preservation_evidence
                .iter_mut()
                .zip(&clause.core_patterns)
            {
                if let Some(pattern) = pattern {
                    *evidence = core_pattern_checked_preservation_evidence(pattern);
                }
            }
            if let Some(guard) = &mut clause.guard {
                refresh_core_evidence_in_expr_summary(guard);
            }
            refresh_core_evidence_in_expr_summary(&mut clause.body);
        }
    }
}

/// Refreshes proof evidence in one expression-summary tree.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
///
/// Output:
/// - None; expression evidence payloads and annotation-dependent proof labels
///   are updated in place.
///
/// Transformation:
/// - Recomputes the current summary's evidence from its final typed Core
///   payload.
/// - Promotes resolved constructor calls to Lean-covered proof coverage while
///   leaving unresolved constructor-call candidates partial.
/// - Recursively refreshes all child summaries.
fn refresh_core_evidence_in_expr_summary(summary: &mut CoreExprSummary) {
    summary.checked_preservation_evidence = summary
        .core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    if let Some(CoreExpr::ConstructorCall {
        constructor_identity,
        ..
    }) = &summary.core_expr
    {
        summary.proof_coverage = if constructor_identity.is_some() {
            CoreProofCoverage::LeanCovered
        } else {
            CoreProofCoverage::Partial
        };
    }
    for child in &mut summary.children {
        refresh_core_evidence_in_expr_summary(child);
    }
}

/// Collects receiver-method dispatch metadata for syntax-to-Core lowering.
///
/// Inputs:
/// - `module`: syntax-output module whose local receiver methods should be
///   available to Core expression summarization.
/// - `resolved`: resolved module state containing imported type names and
///   imported type-alias interfaces.
///
/// Output:
/// - Receiver-method dispatch signatures keyed by `(method name, non-receiver
///   arity)`.
///
/// Transformation:
/// - Rebuilds the same alias/type-name context used by typechecking, then
///   delegates to the receiver-method dispatch collector so CoreIR lowering can
///   preserve the declared mutability marker without reading backend syntax.
pub(crate) fn core_receiver_method_dispatch_signatures(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>> {
    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut alias_names = collect_syntax_type_names(module);
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    alias_names.extend(primitive_type_names());

    collect_syntax_receiver_method_dispatch_signatures_with_imports(
        module,
        resolved,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    )
}

/// Collects constructor identities eligible for CoreIR identity annotation.
///
/// Inputs:
/// - `module`: syntax-output module whose declarations may include local
///   constructors and eligible single-shape type aliases.
/// - `resolved`: resolved module context containing imported item metadata and
///   interface snapshots.
/// - `constructors`: Core constructor declarations from the resolved interface.
///
/// Output:
/// - Map from source-visible constructor name to stable CoreIR constructor
///   identity.
///
/// Transformation:
/// - Preserves local constructor identities as their source-visible name.
/// - Preserves eligible local single-shape type aliases as their source-visible
///   name.
/// - Adds imported public constructors as `module.name` identities so aliased
///   imports can be distinguished from local constructor declarations.
/// - Adds imported public eligible single-shape type aliases as `module.name`
///   identities for the same reason.
/// - Uses both syntax-output declarations and resolved Core constructor
///   declarations so identity annotation can proceed while the Core constructor
///   declaration migration is still catching up.
pub(crate) fn core_constructor_identities(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
    constructors: &[CoreConstructorDecl],
) -> HashMap<String, String> {
    let mut identities = constructors
        .iter()
        .map(|constructor| (constructor.name.clone(), constructor.name.clone()))
        .collect::<HashMap<_, _>>();
    identities.extend(module.declarations.iter().filter_map(
        |declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Constructor { name, .. } => {
                Some((name.clone(), name.clone()))
            }
            _ => None,
        },
    ));
    let local_aliases = collect_syntax_type_aliases(module);
    identities.extend(local_aliases.iter().filter_map(|(name, _)| {
        alias_constructor_schemes(name, &local_aliases).map(|_| (name.clone(), name.clone()))
    }));
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let signatures = interface.constructors.get(&imported.source_name)?;
                signatures
                    .iter()
                    .any(|signature| signature.public)
                    .then(|| {
                        (
                            local_name.clone(),
                            format!("{}.{}", imported.source_module, imported.source_name),
                        )
                    })
            }),
    );
    identities.extend(
        resolved
            .imported_types
            .iter()
            .filter_map(|(local_name, imported)| {
                let interface = resolved.interface_map.get(&imported.source_module)?;
                let interface_aliases = interface_type_aliases(interface);
                alias_constructor_schemes(&imported.source_name, &interface_aliases).map(|_| {
                    (
                        local_name.clone(),
                        format!("{}.{}", imported.source_module, imported.source_name),
                    )
                })
            }),
    );
    identities
}

/// Annotates one Core expression summary tree with constructor identities.
///
/// Inputs:
/// - `summary`: mutable Core expression summary.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; nested Core expression payloads are updated in place.
///
/// Transformation:
/// - Recursively walks both the typed Core payload and summary children so the
///   current node and all nested expression summaries agree on constructor
///   identity annotations.
fn resolve_constructor_identities_in_expr_summary(
    summary: &mut CoreExprSummary,
    constructor_identities: &HashMap<String, String>,
) {
    if let Some(core_expr) = &mut summary.core_expr {
        resolve_constructor_identities_in_core_expr(core_expr, constructor_identities);
    }
    for child in &mut summary.children {
        resolve_constructor_identities_in_expr_summary(child, constructor_identities);
    }
}

/// Annotates one typed Core expression with constructor identities.
///
/// Inputs:
/// - `expr`: mutable typed Core expression.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-call and constructor-pattern payloads are
///   updated in place.
///
/// Transformation:
/// - Traverses every recursive expression and embedded-pattern position and
///   sets constructor identity fields when a candidate name is declared by the
///   resolved module interface.
fn resolve_constructor_identities_in_core_expr(
    expr: &mut CoreExpr,
    constructor_identities: &HashMap<String, String>,
) {
    match expr {
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::Tuple(items)
        | CoreExpr::List(items)
        | CoreExpr::FixedArray(items)
        | CoreExpr::RemoteCall { args: items, .. }
        | CoreExpr::Call { args: items, .. }
        | CoreExpr::Intrinsic(CoreIntrinsicCall { args: items, .. }) => {
            for item in items {
                resolve_constructor_identities_in_core_expr(item, constructor_identities);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_constructor_identities_in_core_expr(callee, constructor_identities);
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            resolve_constructor_identities_in_core_expr(receiver, constructor_identities);
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            resolve_constructor_identities_in_core_expr(head, constructor_identities);
            resolve_constructor_identities_in_core_expr(tail, constructor_identities);
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            resolve_constructor_identities_in_core_expr(expr, constructor_identities);
            resolve_constructor_identities_in_core_pattern(pattern, constructor_identities);
            resolve_constructor_identities_in_core_expr(source, constructor_identities);
            if let Some(guard) = guard {
                resolve_constructor_identities_in_core_expr(guard, constructor_identities);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                resolve_constructor_identities_in_core_expr(
                    &mut binding.value,
                    constructor_identities,
                );
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. }
        | CoreExpr::RecordUpdate { fields, .. }
        | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_expr(
                    &mut field.value,
                    constructor_identities,
                );
            }
            if let CoreExpr::RecordUpdate { base, .. } = expr {
                resolve_constructor_identities_in_core_expr(base, constructor_identities);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            resolve_constructor_identities_in_core_expr(base, constructor_identities);
        }
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => {
            if let Some(identity) = constructor_identities.get(base) {
                *base_constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(record, constructor_identities);
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(constructor) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_expr(arg, constructor_identities);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_constructor_identities_in_core_expr(scrutinee, constructor_identities);
            for clause in clauses {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
            for clause in of_clauses.iter_mut().chain(catch_clauses.iter_mut()) {
                resolve_constructor_identities_in_core_pattern(
                    &mut clause.pattern,
                    constructor_identities,
                );
                if let Some(guard) = &mut clause.guard {
                    resolve_constructor_identities_in_core_expr(guard, constructor_identities);
                }
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
            if let Some(after_clause) = after_clause {
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.trigger,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut after_clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_constructor_identities_in_core_expr(
                    &mut clause.condition,
                    constructor_identities,
                );
                resolve_constructor_identities_in_core_expr(
                    &mut clause.body,
                    constructor_identities,
                );
            }
        }
        CoreExpr::Lam { params, body } => {
            for param in params {
                resolve_constructor_identities_in_core_pattern(param, constructor_identities);
            }
            resolve_constructor_identities_in_core_expr(body, constructor_identities);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            resolve_constructor_identities_in_core_expr(left, constructor_identities);
            resolve_constructor_identities_in_core_expr(right, constructor_identities);
        }
    }
}

/// Annotates one typed Core pattern with constructor identities.
///
/// Inputs:
/// - `pattern`: mutable typed Core pattern.
/// - `constructor_identities`: source-visible constructor names mapped to
///   stable semantic identities.
///
/// Output:
/// - None; matching constructor-pattern payloads are updated in place.
///
/// Transformation:
/// - Recursively traverses compound pattern positions and sets
///   `constructor_identity` when a constructor-pattern candidate name is
///   declared locally or imported from a public constructor interface.
fn resolve_constructor_identities_in_core_pattern(
    pattern: &mut CorePattern,
    constructor_identities: &HashMap<String, String>,
) {
    match pattern {
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::Atom(_) => {}
        CorePattern::Tuple(elements) | CorePattern::List(elements) => {
            for element in elements {
                resolve_constructor_identities_in_core_pattern(element, constructor_identities);
            }
        }
        CorePattern::ListCons { head, tail } => {
            resolve_constructor_identities_in_core_pattern(head, constructor_identities);
            resolve_constructor_identities_in_core_pattern(tail, constructor_identities);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                resolve_constructor_identities_in_core_pattern(
                    &mut field.value,
                    constructor_identities,
                );
            }
        }
        CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } => {
            if let Some(identity) = constructor_identities.get(name) {
                *constructor_identity = Some(identity.clone());
            }
            for arg in args {
                resolve_constructor_identities_in_core_pattern(arg, constructor_identities);
            }
        }
    }
}

/// Converts one syntax function clause into a CoreIR clause summary.
///
/// Inputs:
/// - `clause`: syntax-output function clause.
///
/// Output:
/// - Core function clause summary.
///
/// Transformation:
/// - Renders patterns into stable syntax summaries and recursively summarizes
///   guard/body expressions without backend lowering. Pattern proof labels are
///   retained in the same order as the rendered pattern summaries.
fn core_function_clause_summary(
    clause: &terlan_syntax::SyntaxFunctionClauseOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) -> CoreFunctionClause {
    let patterns = clause
        .patterns
        .iter()
        .map(core_pattern_summary_text)
        .collect();
    let core_patterns: Vec<Option<CorePattern>> = clause
        .patterns
        .iter()
        .map(core_pattern_from_syntax)
        .collect();
    let pattern_proof_coverage = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(pattern, core_pattern)| core_pattern_proof_coverage(pattern, core_pattern.as_ref()))
        .collect();
    let pattern_checked_preservation_evidence = clause
        .patterns
        .iter()
        .zip(core_patterns.iter())
        .map(|(_, core_pattern)| {
            core_pattern
                .as_ref()
                .and_then(core_pattern_checked_preservation_evidence)
        })
        .collect();
    CoreFunctionClause {
        patterns,
        core_patterns,
        pattern_proof_coverage,
        pattern_checked_preservation_evidence,
        guard: clause
            .guard
            .as_ref()
            .map(|guard| core_expr_summary(guard, receiver_methods)),
        body: core_expr_summary(&clause.body, receiver_methods),
    }
}

/// Converts a syntax expression into a recursive CoreIR expression summary.
///
/// Inputs:
/// - `expr`: syntax-output expression.
///
/// Output:
/// - Core expression summary.
///
/// Transformation:
/// - Preserves semantic expression kind, arity, text, remote target, operator,
///   and recursively summarized child expressions while intentionally omitting
///   backend rendering details.
fn core_expr_summary(
    expr: &SyntaxExprOutput,
    receiver_methods: &HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
) -> CoreExprSummary {
    let mut children = expr
        .children
        .iter()
        .map(|child| core_expr_summary(child, receiver_methods))
        .collect::<Vec<_>>();
    children.extend(
        expr.fields
            .iter()
            .map(|field| core_expr_summary(&field.value, receiver_methods)),
    );
    children.extend(expr.clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(guard, receiver_methods));
        }
        clause_children.push(core_expr_summary(&clause.body, receiver_methods));
        clause_children
    }));
    children.extend(expr.catch_clauses.iter().flat_map(|clause| {
        let mut clause_children = Vec::new();
        if let Some(guard) = &clause.guard {
            clause_children.push(core_expr_summary(guard, receiver_methods));
        }
        clause_children.push(core_expr_summary(&clause.body, receiver_methods));
        clause_children
    }));
    if let Some(after) = &expr.try_after {
        children.push(core_expr_summary(&after.trigger, receiver_methods));
        children.push(core_expr_summary(&after.body, receiver_methods));
    }
    let core_expr = core_mutable_receiver_call_expr_from_syntax(expr, receiver_methods)
        .or_else(|| core_expr_from_syntax(expr));
    let checked_preservation_evidence = core_expr
        .as_ref()
        .and_then(core_expr_checked_preservation_evidence);
    let proof_coverage = core_expr_proof_coverage(expr, core_expr.as_ref());

    CoreExprSummary {
        kind: format!("{:?}", expr.kind),
        core_expr,
        checked_preservation_evidence,
        proof_coverage,
        text: expr.text.clone(),
        remote: expr.remote.clone(),
        operator: expr.operator.clone(),
        arity: expr.arity,
        children,
    }
}
