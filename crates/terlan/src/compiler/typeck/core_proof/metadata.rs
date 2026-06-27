//! CoreIR module metadata counting and proof-readiness helpers.

use super::*;

/// Aggregate proof-coverage buckets for CoreIR expressions or patterns.
///
/// Inputs:
/// - Individual proof-coverage classifications emitted during CoreIR lowering.
///
/// Output:
/// - Count totals used by module metadata, release checks, and proof-readiness
///   reporting.
///
/// Transformation:
/// - Accumulates each coverage class without interpreting readiness by itself.
#[derive(Clone, Default)]
pub(crate) struct CoreProofCoverageCounts {
    pub(crate) lean_covered: usize,
    pub(crate) partial: usize,
    pub(crate) proof_model_required: usize,
    pub(crate) runtime_boundary: usize,
    pub(crate) artifact_only: usize,
}

/// Counts expression summaries that carry typed CoreIR payloads.
///
/// Inputs:
/// - Core expression summaries visited inside function guards and bodies.
///
/// Output:
/// - Typed vs summary-only expression payload totals.
///
/// Transformation:
/// - Separates fully lowered expressions from textual summaries for formal
///   pipeline progress tracking.
#[derive(Clone, Default)]
struct CoreExprPayloadCounts {
    typed_core_expr: usize,
    summary_only_expr: usize,
}

/// Counts checked-preservation evidence attached to CoreIR summaries.
///
/// Inputs:
/// - Expression and pattern preservation evidence emitted by lowering.
///
/// Output:
/// - Totals for structural checks, no-runtime-binding checks, and runtime
///   binding requirements.
///
/// Transformation:
/// - Groups evidence by expression/pattern and by the kind of preservation
///   obligation the proof pipeline must satisfy.
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

/// Counts pattern summaries that carry typed CoreIR payloads.
///
/// Inputs:
/// - Core pattern summaries from function clauses.
///
/// Output:
/// - Typed vs summary-only pattern payload totals.
///
/// Transformation:
/// - Tracks how much pattern lowering is backed by structured CoreIR.
#[derive(Clone, Default)]
struct CorePatternPayloadCounts {
    typed_core_pattern: usize,
    summary_only_pattern: usize,
}

/// Counts type positions that carry typed CoreIR payloads.
///
/// Inputs:
/// - Function, constructor, and type-declaration annotations.
///
/// Output:
/// - Typed vs summary-only type payload totals.
///
/// Transformation:
/// - Records whether each type position has structured Core type data or only
///   textual summary data.
#[derive(Clone, Default)]
pub(crate) struct CoreTypePayloadCounts {
    pub(crate) typed_core_type: usize,
    pub(crate) summary_only_type: usize,
}

/// Counts constructor identities resolved during CoreIR lowering.
///
/// Inputs:
/// - Constructor calls, constructor chains, and constructor patterns found in
///   expression/pattern summaries.
///
/// Output:
/// - Resolved and unresolved constructor identity candidate totals.
///
/// Transformation:
/// - Separates confirmed constructor lowering from unresolved syntactic
///   candidates so release checks can spot remaining semantic gaps.
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
        | CoreExpr::SqlQuery { .. }
        | CoreExpr::Call { .. }
        | CoreExpr::MutableReceiverCall { .. }
        | CoreExpr::FunctionCall { .. }
        | CoreExpr::Cast { .. }
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
