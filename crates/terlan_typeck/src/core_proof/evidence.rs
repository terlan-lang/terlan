use super::*;

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
pub(super) fn core_expr_checked_preservation_evidence(
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
        CoreExpr::Cast { expr, .. } => core_expr_substitution_freshness_evidence(expr),
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
        CoreExpr::Cast { expr, .. } => core_expr_has_checked_preservation_evidence(expr),
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
pub(super) fn core_pattern_checked_preservation_evidence(
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
