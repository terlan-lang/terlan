use super::core_pattern_lowering::core_pattern_is_lean_modeled;
use super::*;

/// Classifies a syntax-output expression for Lean proof coverage.
///
/// Inputs:
/// - `expr`: syntax-output expression being summarized into CoreIR.
/// - `core_expr`: typed Core payload produced for `expr`, when available.
///
/// Output:
/// - Proof coverage label for the current production CoreIR summary.
///
/// Transformation:
/// - Marks the current Lean-covered expression families as covered only when
///   they actually carry typed `CoreExpr` payloads; unsupported members of
///   those families remain proof-model-required until their Core payload exists.
pub(crate) fn core_expr_proof_coverage(
    expr: &SyntaxExprOutput,
    core_expr: Option<&CoreExpr>,
) -> CoreProofCoverage {
    match expr.kind {
        SyntaxExprKind::Int
        | SyntaxExprKind::Binary
        | SyntaxExprKind::Atom
        | SyntaxExprKind::Var
        | SyntaxExprKind::Tuple
        | SyntaxExprKind::List
        | SyntaxExprKind::Fun => match core_expr {
            Some(core_expr) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(_) => CoreProofCoverage::ProofModelRequired,
            None => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Case => match core_expr {
            Some(CoreExpr::Case { scrutinee, clauses })
                if core_expr_is_lean_modeled(scrutinee)
                    && core_case_clauses_are_lean_modeled(clauses) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::Case { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::FunctionCall => match core_expr {
            Some(core_expr @ CoreExpr::FunctionCall { .. })
                if core_expr_is_lean_modeled(core_expr) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::FunctionCall { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Call if expr.remote.is_none() => match core_expr {
            Some(core_expr @ CoreExpr::Call { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::Call { .. }) => CoreProofCoverage::ProofModelRequired,
            Some(CoreExpr::ConstructorCall {
                constructor_identity,
                args,
                ..
            }) => {
                if constructor_identity.is_some() && args.iter().all(core_expr_is_lean_modeled) {
                    CoreProofCoverage::LeanCovered
                } else if constructor_identity.is_some() {
                    CoreProofCoverage::ProofModelRequired
                } else {
                    CoreProofCoverage::Partial
                }
            }
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Call => remote_call_proof_coverage_policy(core_expr),
        SyntaxExprKind::ConstructorChain => constructor_chain_proof_coverage_policy(core_expr),
        SyntaxExprKind::Macro
        | SyntaxExprKind::RawMacro
        | SyntaxExprKind::HtmlBlock
        | SyntaxExprKind::Quote
        | SyntaxExprKind::Unquote => CoreProofCoverage::RuntimeBoundary,
        SyntaxExprKind::ListCons => match core_expr {
            Some(core_expr @ CoreExpr::ListCons { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::ListCons { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::If => match core_expr {
            Some(core_expr @ CoreExpr::If { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::If { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::FieldAccess => match core_expr {
            Some(core_expr @ CoreExpr::FieldAccess { .. })
                if core_expr_is_lean_modeled(core_expr) =>
            {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::FieldAccess { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Let => match core_expr {
            Some(CoreExpr::Let { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Sequence => CoreProofCoverage::ProofModelRequired,
        SyntaxExprKind::Cast => match core_expr {
            Some(CoreExpr::Cast { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::Float
        | SyntaxExprKind::Map
        | SyntaxExprKind::RecordConstruct
        | SyntaxExprKind::RecordAccess
        | SyntaxExprKind::RecordUpdate
        | SyntaxExprKind::FixedArray
        | SyntaxExprKind::Index
        | SyntaxExprKind::IndexAssign
        | SyntaxExprKind::ListComprehension
        | SyntaxExprKind::Try
        | SyntaxExprKind::RemoteFunRef
        | SyntaxExprKind::TemplateInstantiate => CoreProofCoverage::ProofModelRequired,
        SyntaxExprKind::UnaryOp => match core_expr {
            Some(core_expr @ CoreExpr::UnaryOp { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::UnaryOp { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
        SyntaxExprKind::BinaryOp => match core_expr {
            Some(core_expr @ CoreExpr::BinaryOp { .. }) if core_expr_is_lean_modeled(core_expr) => {
                CoreProofCoverage::LeanCovered
            }
            Some(CoreExpr::BinaryOp { .. }) => CoreProofCoverage::ProofModelRequired,
            _ => CoreProofCoverage::ProofModelRequired,
        },
    }
}

/// Returns the active proof-coverage policy for remote-call expressions.
///
/// Inputs:
/// - `core_expr`: typed Core payload lowered from a source remote call, when
///   available.
///
/// Output:
/// - `CoreProofCoverage::ProofModelRequired` under the current remote-dispatch
///   readiness policy.
///
/// Transformation:
/// - Keeps the production CoreIR payload visible while preventing accidental
///   promotion to Lean-covered coverage before the roadmap decides whether
///   value-ready remote dispatch is acceptable as runtime-boundary evidence or
///   still requires a backend dispatch contract.
pub(crate) fn remote_call_proof_coverage_policy(core_expr: Option<&CoreExpr>) -> CoreProofCoverage {
    match core_expr {
        Some(CoreExpr::RemoteCall { .. }) | None => CoreProofCoverage::ProofModelRequired,
        Some(_) => CoreProofCoverage::ProofModelRequired,
    }
}

/// Returns the active proof-coverage policy for constructor-chain expressions.
///
/// Inputs:
/// - `core_expr`: typed Core payload lowered from source constructor-chain
///   syntax, when available.
///
/// Output:
/// - `CoreProofCoverage::Partial` under the current constructor-chain policy.
///
/// Transformation:
/// - Keeps resolved constructor-chain identity evidence separate from Lean
///   coverage. A chain may have a resolved base constructor identity and still
///   remain partial until record construction and constructor-chain semantics
///   have a dedicated proof model.
pub(crate) fn constructor_chain_proof_coverage_policy(
    core_expr: Option<&CoreExpr>,
) -> CoreProofCoverage {
    match core_expr {
        Some(CoreExpr::ConstructorChain { .. }) | None => CoreProofCoverage::Partial,
        Some(_) => CoreProofCoverage::Partial,
    }
}

/// Checks whether a Core expression maps to the current Lean expression subset.
///
/// Inputs:
/// - `expr`: typed production Core expression.
///
/// Output:
/// - `true` when the expression and all nested executable children map to the
///   current Lean `Expr` subset.
/// - `false` when the expression has a typed Core payload but still needs Lean
///   syntax, typing, or semantics.
///
/// Transformation:
/// - Recursively inspects expression and pattern children without modifying the
///   production CoreExpr payload.
pub(crate) fn core_expr_is_lean_modeled(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Int(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => true,
        CoreExpr::Tuple(items) | CoreExpr::List(items) => {
            items.iter().all(core_expr_is_lean_modeled)
        }
        CoreExpr::Call { args, .. } => args.iter().all(core_expr_is_lean_modeled),
        CoreExpr::FunctionCall { callee, args } => {
            core_expr_is_lean_modeled(callee) && args.iter().all(core_expr_is_lean_modeled)
        }
        CoreExpr::Cast { .. } => false,
        CoreExpr::ConstructorCall {
            constructor_identity,
            args,
            ..
        } => constructor_identity.is_some() && args.iter().all(core_expr_is_lean_modeled),
        CoreExpr::Case { scrutinee, clauses } => {
            core_expr_is_lean_modeled(scrutinee) && core_case_clauses_are_lean_modeled(clauses)
        }
        CoreExpr::Lam { params, body } => {
            params.iter().all(core_pattern_is_lean_modeled) && core_expr_is_lean_modeled(body)
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            matches!(
                operator.as_str(),
                "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">="
            ) && core_expr_is_lean_modeled(left)
                && core_expr_is_lean_modeled(right)
        }
        CoreExpr::UnaryOp { operator, operand } => {
            operator == "-" && core_expr_is_lean_modeled(operand)
        }
        CoreExpr::ListCons { head, tail } => {
            core_expr_is_lean_modeled(head) && core_expr_is_lean_modeled(tail)
        }
        CoreExpr::If { clauses } => core_if_clauses_are_lean_modeled(clauses),
        CoreExpr::FieldAccess { base, .. } => core_expr_is_lean_modeled(base),
        CoreExpr::Binary(_) => true,
        CoreExpr::RemoteCall { .. } => remote_call_is_promoted_to_lean_covered(),
        CoreExpr::Float(_)
        | CoreExpr::FixedArray(_)
        | CoreExpr::Index { .. }
        | CoreExpr::ListComprehension { .. }
        | CoreExpr::Let { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::ConstructorChain { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::MutableReceiverCall { .. }
        | CoreExpr::Intrinsic(_)
        | CoreExpr::Try { .. } => false,
    }
}

/// Reports whether remote calls are currently promoted to Lean-covered status.
///
/// Inputs:
/// - None; this is a compiler policy switch, not a per-expression decision yet.
///
/// Output:
/// - `false` until the formal roadmap promotes the selected remote-dispatch
///   subset and updates phase-contract goldens, proof-baseline tables, and target
///   dispatch contracts together.
///
/// Transformation:
/// - Encodes the current remote-dispatch readiness policy as an explicit helper
///   so future promotion changes happen in one named place.
pub(crate) fn remote_call_is_promoted_to_lean_covered() -> bool {
    false
}

/// Checks whether Core if clauses map to the current Lean if subset.
///
/// Inputs:
/// - `clauses`: typed Core if clauses lowered from syntax output.
///
/// Output:
/// - `true` only for the selected one-clause subset whose condition and body
///   are both Lean-modeled.
/// - `false` for empty, multi-clause, or nested unmodeled if payloads.
///
/// Transformation:
/// - Inspects clause shape and recursively checks condition/body CoreExpr
///   payloads without modifying production CoreIR.
fn core_if_clauses_are_lean_modeled(clauses: &[CoreIfClause]) -> bool {
    matches!(
        clauses,
        [CoreIfClause { condition, body }]
            if core_expr_is_lean_modeled(condition) && core_expr_is_lean_modeled(body)
    )
}

/// Checks whether Core case clauses use only Lean-modeled pattern forms.
///
/// Inputs:
/// - `clauses`: typed Core case clauses lowered from syntax output.
///
/// Output:
/// - `true` when every clause is unguarded and every branch pattern maps to
///   the current Lean `Pattern` subset.
/// - `false` when a guard or unmodeled pattern form requires new Lean syntax,
///   typing, or match semantics.
///
/// Transformation:
/// - Traverses clause guards, patterns, and branch bodies without modifying the
///   production CoreExpr payload.
fn core_case_clauses_are_lean_modeled(clauses: &[CoreCaseClause]) -> bool {
    clauses.iter().all(|clause| {
        clause.guard.is_none()
            && core_pattern_is_lean_modeled(&clause.pattern)
            && core_expr_is_lean_modeled(&clause.body)
    })
}
