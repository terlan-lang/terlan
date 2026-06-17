use terlan_typeck::CoreExpr;
use terlan_typeck::CoreExprSummary;
use terlan_typeck::CoreImportKind;
use terlan_typeck::CoreModule;
use terlan_typeck::CorePattern;
use terlan_typeck::CoreProofCoverage;

use std::collections::HashSet;

/// Diagnostic code emitted when unresolved constructor metadata reaches target
/// profile validation.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Stable diagnostic code used by production validation and regression tests.
///
/// Transformation:
/// - Centralizes the unresolved-constructor target-profile violation label
///   without allocating or inspecting CoreIR.
const TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE: &str = "target_profile_unresolved_constructor";

/// Formats the unresolved-constructor target-profile diagnostic message.
///
/// Inputs:
/// - `profile`: target profile being validated.
/// - `calls`: unresolved constructor-call candidate count.
/// - `chains`: unresolved constructor-chain candidate count.
/// - `patterns`: unresolved constructor-pattern candidate count.
///
/// Output:
/// - Stable diagnostic message with profile name and candidate counters.
///
/// Transformation:
/// - Converts profile and counter values into the user-facing backend-profile
///   constructor-resolution error message.
fn unresolved_constructor_message(
    profile: TargetProfile,
    calls: usize,
    chains: usize,
    patterns: usize,
) -> String {
    format!(
        "target `{}` requires constructor candidates to resolve before backend validation: calls={calls} chains={chains} patterns={patterns}",
        profile.as_str()
    )
}

/// Backend-capability profile for backend-aware compile gating.
///
/// Inputs:
/// - Caller-selected backend profile.
///
/// Output:
/// - Profile rules used by formal pipeline profile validation.
///
/// Transformation:
/// - Encodes profile constraints over proof-coverage classes and core
///   expression form families.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TargetProfile {
    /// Current formal frontend/backend path: accepts the existing CoreIR surface.
    #[default]
    Erlang,
    /// Frozen 0.0.1 release-candidate Erlang artifact subset.
    A0Erlang,
    /// Named A0.1 successor Erlang artifact subset for simple Int expressions.
    A01Erlang,
    /// Named A0.2 successor Erlang artifact subset for boolean expressions.
    A02Erlang,
    /// Named A0.3 successor Erlang artifact subset for conditional expressions.
    A03Erlang,
    /// Named A0.4 successor Erlang artifact subset for simple case expressions.
    A04Erlang,
    /// Named A0.5 successor Erlang artifact subset for raw atom literals.
    A05Erlang,
    /// Named A0.6 successor Erlang artifact subset for tuple values.
    A06Erlang,
    /// Named A0.7 successor Erlang artifact subset for list values.
    A07Erlang,
    /// Named A0.8 successor Erlang artifact subset for binary/string literals.
    A08Erlang,
    /// Named A0.9 successor Erlang artifact subset for expression-side list cons.
    A09Erlang,
    /// Named A0.10 successor Erlang artifact subset for local named calls.
    A010Erlang,
    /// Named A0.11 successor Erlang artifact subset for unary negation.
    A011Erlang,
    /// Named A0.12 successor Erlang artifact subset for resolved constructor calls.
    A012Erlang,
    /// Named A0.13 successor Erlang artifact subset for resolved constructor patterns.
    A013Erlang,
    /// Named A0.14 successor Erlang artifact subset for anonymous function values.
    A014Erlang,
    /// Named A0.15 successor Erlang artifact subset for constructor extension.
    A015Erlang,
    /// Named A0.16 successor Erlang artifact subset for function-value invocation.
    A016Erlang,
    /// Named A0.17 successor Erlang artifact subset for struct field access.
    A017Erlang,
    /// Named A0.18 successor Erlang artifact subset for local let bindings.
    A018Erlang,
    /// Named A0.19 successor Erlang artifact subset for index access.
    A019Erlang,
    /// Named A0.20 successor Erlang artifact subset for qualified/scoped calls.
    A020Erlang,
    /// Named A0.21 successor Erlang diagnostic subset for unsupported references.
    A021Erlang,
    /// Portable CoreIR v0 subset: accepts only typed, Lean-covered CoreIR forms.
    CoreV0,
}

impl TargetProfile {
    /// Human-readable profile name.
    ///
    /// Inputs:
    /// - One profile variant.
    ///
    /// Output:
    /// - Stable ASCII profile name.
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Erlang => "erlang",
            Self::A0Erlang => "a0-erlang",
            Self::A01Erlang => "a0.1-erlang",
            Self::A02Erlang => "a0.2-erlang",
            Self::A03Erlang => "a0.3-erlang",
            Self::A04Erlang => "a0.4-erlang",
            Self::A05Erlang => "a0.5-erlang",
            Self::A06Erlang => "a0.6-erlang",
            Self::A07Erlang => "a0.7-erlang",
            Self::A08Erlang => "a0.8-erlang",
            Self::A09Erlang => "a0.9-erlang",
            Self::A010Erlang => "a0.10-erlang",
            Self::A011Erlang => "a0.11-erlang",
            Self::A012Erlang => "a0.12-erlang",
            Self::A013Erlang => "a0.13-erlang",
            Self::A014Erlang => "a0.14-erlang",
            Self::A015Erlang => "a0.15-erlang",
            Self::A016Erlang => "a0.16-erlang",
            Self::A017Erlang => "a0.17-erlang",
            Self::A018Erlang => "a0.18-erlang",
            Self::A019Erlang => "a0.19-erlang",
            Self::A020Erlang => "a0.20-erlang",
            Self::A021Erlang => "a0.21-erlang",
            Self::CoreV0 => "core-v0",
        }
    }

    /// Returns whether profile allows a given expression-level proof coverage.
    ///
    /// Inputs:
    /// - `coverage`: expression proof coverage produced during CoreIR lowering.
    ///
    /// Output:
    /// - `true` when this profile accepts that proof class.
    const fn allows_expr_coverage(&self, coverage: CoreProofCoverage) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => true,
            Self::CoreV0 => matches!(coverage, CoreProofCoverage::LeanCovered),
        }
    }

    /// Returns whether profile allows a given pattern-level proof coverage.
    ///
    /// Inputs:
    /// - `coverage`: pattern proof coverage produced during CoreIR lowering.
    ///
    /// Output:
    /// - `true` when this profile accepts that proof class.
    const fn allows_pattern_coverage(&self, coverage: CoreProofCoverage) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => true,
            Self::CoreV0 => matches!(coverage, CoreProofCoverage::LeanCovered),
        }
    }

    /// Returns whether pattern summaries with no typed payload are acceptable.
    ///
    /// Inputs:
    /// - One target profile.
    ///
    /// Output:
    /// - `true` for supported Erlang forms.
    const fn allows_uncovered_pattern(&self) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => false,
            Self::CoreV0 => false,
        }
    }

    /// Returns whether expression summaries with runtime-boundary evidence are
    /// acceptable.
    ///
    /// Inputs:
    /// - One target profile.
    ///
    /// Output:
    /// - `true` for supported Erlang forms.
    const fn allows_runtime_boundary(&self) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang => false,
            Self::A020Erlang | Self::A021Erlang => true,
            Self::CoreV0 => false,
        }
    }

    /// Returns whether typed payloads must carry checked-preservation evidence.
    ///
    /// Inputs:
    /// - One target profile.
    ///
    /// Output:
    /// - `true` when the profile requires explicit preservation evidence for
    ///   typed CoreIR payloads.
    const fn requires_checked_preservation_evidence(&self) -> bool {
        match self {
            Self::Erlang => false,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => false,
            Self::CoreV0 => true,
        }
    }

    /// Returns whether a typed pattern constructor is structurally acceptable for
    /// the profile.
    ///
    /// Inputs:
    /// - `expr`: typed core pattern being considered.
    ///
    /// Output:
    /// - `true` when all current backend profiles accept the node.
    fn allows_pattern_shape(&self, pattern: &CorePattern) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang | Self::A01Erlang | Self::A02Erlang | Self::A03Erlang => {
                matches!(pattern, CorePattern::Var(_))
            }
            Self::A04Erlang => matches!(pattern, CorePattern::Var(_) | CorePattern::Int(_)),
            Self::A05Erlang => matches!(
                pattern,
                CorePattern::Wildcard
                    | CorePattern::Var(_)
                    | CorePattern::Int(_)
                    | CorePattern::Atom(_)
            ),
            Self::A06Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A07Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A08Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A09Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A010Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A011Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A012Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A013Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                CorePattern::Constructor {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_pattern_shape(arg))
                }
                _ => false,
            },
            Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                CorePattern::Constructor {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_pattern_shape(arg))
                }
                _ => false,
            },
            Self::CoreV0 => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                CorePattern::Constructor {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_pattern_shape(arg))
                }
                CorePattern::Float(_)
                | CorePattern::ListCons { .. }
                | CorePattern::Map(_)
                | CorePattern::Record { .. } => false,
            },
        }
    }

    /// Returns whether a typed expression form is structurally acceptable for the
    /// profile.
    ///
    /// Inputs:
    /// - `expr`: typed core expression node.
    ///
    /// Output:
    /// - `true` when this profile accepts the core expression family.
    fn allows_expr_shape(&self, expr: &CoreExpr) -> bool {
        match self {
            Self::Erlang => true,
            Self::A0Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) => true,
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    operator == "+" && self.allows_expr_shape(left) && self.allows_expr_shape(right)
                }
                _ => false,
            },
            Self::A01Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) => true,
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">="
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                _ => false,
            },
            Self::A02Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) => true,
                CoreExpr::Atom(value) => matches!(value.as_str(), "true" | "false"),
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                _ => false,
            },
            Self::A03Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) => true,
                CoreExpr::Atom(value) => matches!(value.as_str(), "true" | "false"),
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                _ => false,
            },
            Self::A04Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) => true,
                CoreExpr::Atom(value) => matches!(value.as_str(), "true" | "false"),
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A05Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => true,
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A06Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => true,
                CoreExpr::Tuple(values) => values.iter().all(|value| self.allows_expr_shape(value)),
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A07Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => true,
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A08Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A09Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A010Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A011Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A012Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::FunctionCall { callee, args } => {
                    matches!(
                        self,
                        Self::A016Erlang
                            | Self::A017Erlang
                            | Self::A018Erlang
                            | Self::A019Erlang
                            | Self::A020Erlang
                            | Self::A021Erlang
                    ) && self.allows_expr_shape(callee)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::ConstructorCall {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A013Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::FunctionCall { callee, args } => {
                    matches!(
                        self,
                        Self::A016Erlang
                            | Self::A017Erlang
                            | Self::A018Erlang
                            | Self::A019Erlang
                            | Self::A020Erlang
                            | Self::A021Erlang
                    ) && self.allows_expr_shape(callee)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::ConstructorCall {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                _ => false,
            },
            Self::A014Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::FunctionCall { callee, args } => {
                    matches!(
                        self,
                        Self::A016Erlang
                            | Self::A017Erlang
                            | Self::A018Erlang
                            | Self::A019Erlang
                            | Self::A020Erlang
                            | Self::A021Erlang
                    ) && self.allows_expr_shape(callee)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::ConstructorCall {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                CoreExpr::Lam { params, body, .. } => {
                    params.iter().all(|param| self.allows_pattern_shape(param))
                        && self.allows_expr_shape(body)
                }
                _ => false,
            },
            Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) | CoreExpr::Atom(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::FunctionCall { callee, args } => {
                    matches!(
                        self,
                        Self::A016Erlang
                            | Self::A017Erlang
                            | Self::A018Erlang
                            | Self::A019Erlang
                            | Self::A020Erlang
                            | Self::A021Erlang
                    ) && self.allows_expr_shape(callee)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::ConstructorCall {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::ConstructorChain {
                    base_constructor_identity,
                    args,
                    record,
                    ..
                } => {
                    base_constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                        && match record.as_ref() {
                            CoreExpr::RecordConstruct { fields, .. } => fields
                                .iter()
                                .all(|field| self.allows_expr_shape(&field.value)),
                            _ => false,
                        }
                }
                CoreExpr::RecordConstruct { fields, .. } => fields
                    .iter()
                    .all(|field| self.allows_expr_shape(&field.value)),
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">=" | "and" | "or"
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                CoreExpr::Lam { params, body, .. } => {
                    params.iter().all(|param| self.allows_pattern_shape(param))
                        && self.allows_expr_shape(body)
                }
                CoreExpr::FieldAccess { base, .. } => {
                    matches!(
                        self,
                        Self::A017Erlang
                            | Self::A018Erlang
                            | Self::A019Erlang
                            | Self::A020Erlang
                            | Self::A021Erlang
                    ) && self.allows_expr_shape(base)
                }
                CoreExpr::Let { bindings, body } => {
                    matches!(
                        self,
                        Self::A018Erlang | Self::A019Erlang | Self::A020Erlang | Self::A021Erlang
                    ) && bindings
                        .iter()
                        .all(|binding| self.allows_expr_shape(&binding.value))
                        && self.allows_expr_shape(body)
                }
                CoreExpr::Index { base, index } => {
                    matches!(self, Self::A019Erlang | Self::A020Erlang | Self::A021Erlang)
                        && self.allows_expr_shape(base)
                        && self.allows_expr_shape(index)
                }
                CoreExpr::RemoteCall { args, .. } => {
                    matches!(self, Self::A020Erlang | Self::A021Erlang)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                _ => false,
            },
            Self::CoreV0 => match expr {
                CoreExpr::Int(_) | CoreExpr::Binary(_) | CoreExpr::Atom(_) | CoreExpr::Var(_) => {
                    true
                }
                CoreExpr::Tuple(values) | CoreExpr::List(values) => {
                    values.iter().all(|value| self.allows_expr_shape(value))
                }
                CoreExpr::ListCons { head, tail } => {
                    self.allows_expr_shape(head) && self.allows_expr_shape(tail)
                }
                CoreExpr::ConstructorCall {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::Call { args, .. } => args.iter().all(|arg| self.allows_expr_shape(arg)),
                CoreExpr::FunctionCall { callee, args } => {
                    self.allows_expr_shape(callee)
                        && args.iter().all(|arg| self.allows_expr_shape(arg))
                }
                CoreExpr::Case { scrutinee, clauses } => {
                    self.allows_expr_shape(scrutinee)
                        && clauses.iter().all(|clause| {
                            clause.guard.is_none()
                                && self.allows_pattern_shape(&clause.pattern)
                                && self.allows_expr_shape(&clause.body)
                        })
                }
                CoreExpr::If { clauses } => clauses.iter().all(|clause| {
                    self.allows_expr_shape(&clause.condition)
                        && self.allows_expr_shape(&clause.body)
                }),
                CoreExpr::FieldAccess { base, .. } => self.allows_expr_shape(base),
                CoreExpr::Lam { params, body, .. } => {
                    params.iter().all(|param| self.allows_pattern_shape(param))
                        && self.allows_expr_shape(body)
                }
                CoreExpr::UnaryOp { operator, operand } => {
                    operator == "-" && self.allows_expr_shape(operand)
                }
                CoreExpr::BinaryOp {
                    operator,
                    left,
                    right,
                } => {
                    matches!(
                        operator.as_str(),
                        "+" | "-" | "*" | "==" | "<" | "<=" | ">" | ">="
                    ) && self.allows_expr_shape(left)
                        && self.allows_expr_shape(right)
                }
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
                | CoreExpr::RemoteCall { .. }
                | CoreExpr::MutableReceiverCall { .. }
                | CoreExpr::Intrinsic(_)
                | CoreExpr::Try { .. } => false,
            },
        }
    }
}

/// Structured target-profile violation with enough context for diagnostics.
#[derive(Debug)]
pub(crate) struct TargetProfileViolation {
    /// Stable violation code.
    pub(crate) code: &'static str,
    /// Human-readable explanation.
    pub(crate) message: String,
}

/// Command-level options for target-profile validation.
///
/// Inputs:
/// - Set by the command that invokes formal compilation.
///
/// Output:
/// - Validation switches that are independent of target capability identity.
///
/// Transformation:
/// - Lets commands that own filesystem asset resolution admit asset imports
///   while generic compile/check paths still reject them before backend
///   emission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TargetProfileCheckOptions {
    pub(crate) allow_asset_imports: bool,
}

impl TargetProfileViolation {
    /// Builds an unsupported-feature violation.
    ///
    /// Inputs:
    /// - `pattern`: violated feature category.
    /// - `profile`: profile that rejected the feature.
    /// - `context`: function, clause, or payload location.
    /// - `detail`: concrete rejected coverage or shape detail.
    ///
    /// Output:
    /// - Normalized `target_profile_unsupported` violation.
    ///
    /// Transformation:
    /// - Formats profile, category, detail, and context into a CLI-safe
    ///   diagnostic message.
    fn unsupported(pattern: &str, profile: TargetProfile, context: &str, detail: &str) -> Self {
        Self {
            code: "target_profile_unsupported",
            message: format!(
                "target `{}` does not support {} {} for {}",
                profile.as_str(),
                pattern,
                detail,
                context
            ),
        }
    }

    /// Builds a missing checked-preservation evidence violation.
    ///
    /// Inputs:
    /// - `profile`: profile that requires evidence.
    /// - `context`: function, clause, or payload location.
    /// - `detail`: missing evidence class.
    ///
    /// Output:
    /// - Normalized `target_profile_missing_evidence` violation.
    ///
    /// Transformation:
    /// - Formats profile, missing evidence detail, and context into a CLI-safe
    ///   diagnostic message.
    fn missing_evidence(profile: TargetProfile, context: &str, detail: &str) -> Self {
        Self {
            code: "target_profile_missing_evidence",
            message: format!(
                "target `{}` requires checked-preservation evidence {} for {}",
                profile.as_str(),
                detail,
                context
            ),
        }
    }
}

/// Validates a lowered `CoreModule` against a target profile.
///
/// Inputs:
/// - `module`: typed core module produced by formal lowering.
/// - `profile`: backend profile requested by the caller.
///
/// Output:
/// - A list of violations when module features exceed the selected profile.
///
/// Transformation:
/// - Rejects unresolved constructor candidates from CoreIR metadata, traverses
///   every function clause body, guard, and typed pattern summary, then
///   inspects typed core nodes for structural shape constraints.
#[cfg(test)]
fn target_profile_checks(
    module: &CoreModule,
    profile: TargetProfile,
) -> Vec<TargetProfileViolation> {
    target_profile_checks_with_options(module, profile, TargetProfileCheckOptions::default())
}

/// Validates a lowered `CoreModule` against a target profile with command-level
/// validation options.
///
/// Inputs:
/// - `module`: typed core module produced by formal lowering.
/// - `profile`: backend profile requested by the caller.
/// - `options`: command-owned target validation switches.
///
/// Output:
/// - A list of violations when module features exceed the selected profile and
///   command capabilities.
///
/// Transformation:
/// - Applies import-family checks that may depend on command-owned resolvers,
///   then runs the normal CoreIR shape, evidence, and constructor-resolution
///   profile validation.
pub(crate) fn target_profile_checks_with_options(
    module: &CoreModule,
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
) -> Vec<TargetProfileViolation> {
    let mut violations = Vec::new();
    let std_call_heads = std_call_heads(module);

    validate_core_imports(profile, module, options, &mut violations);

    if module.metadata.unresolved_constructor_call_candidate_count
        + module.metadata.unresolved_constructor_chain_candidate_count
        + module
            .metadata
            .unresolved_constructor_pattern_candidate_count
        != 0
    {
        violations.push(TargetProfileViolation {
            code: TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE,
            message: unresolved_constructor_message(
                profile,
                module.metadata.unresolved_constructor_call_candidate_count,
                module.metadata.unresolved_constructor_chain_candidate_count,
                module
                    .metadata
                    .unresolved_constructor_pattern_candidate_count,
            ),
        });
    }

    for function in &module.functions {
        for (clause_index, clause) in function.clauses.iter().enumerate() {
            let context = format!("function {}@{}", function.name, clause_index);
            for (pattern_index, pattern) in clause.core_patterns.iter().enumerate() {
                let location = format!("{context} pattern#{pattern_index}");
                match (pattern, clause.pattern_proof_coverage.get(pattern_index)) {
                    (Some(pattern), Some(coverage)) => {
                        if !profile.allows_pattern_coverage(*coverage) {
                            violations.push(TargetProfileViolation::unsupported(
                                "pattern coverage",
                                profile,
                                &location,
                                &format!("{coverage:?}"),
                            ));
                        }
                        if profile.requires_checked_preservation_evidence()
                            && clause
                                .pattern_checked_preservation_evidence
                                .get(pattern_index)
                                .is_none_or(Option::is_none)
                        {
                            violations.push(TargetProfileViolation::missing_evidence(
                                profile,
                                &location,
                                "for typed pattern payload",
                            ));
                        }
                        validate_core_pattern(profile, pattern, &location, &mut violations);
                    }
                    (None, Some(coverage)) => {
                        if !profile.allows_pattern_coverage(*coverage)
                            || !profile.allows_uncovered_pattern()
                        {
                            violations.push(TargetProfileViolation::unsupported(
                                "untyped pattern",
                                profile,
                                &location,
                                &format!("coverage={coverage:?}"),
                            ));
                        }
                    }
                    (_, None) => {}
                }
            }

            if let Some(coverage) = clause.pattern_proof_coverage.first() {
                if !profile.allows_pattern_coverage(*coverage) {
                    violations.push(TargetProfileViolation::unsupported(
                        "pattern coverage",
                        profile,
                        &context,
                        &format!("{coverage:?}"),
                    ));
                }
            }

            validate_core_expr_summary(
                profile,
                &std_call_heads,
                &context,
                "body",
                &clause.body,
                &mut violations,
            );
            if let Some(guard) = &clause.guard {
                validate_core_expr_summary(
                    profile,
                    &std_call_heads,
                    &context,
                    "guard",
                    guard,
                    &mut violations,
                );
            }
        }
    }

    violations
}

/// Std module call-head aliases visible to target-profile validation.
///
/// Inputs:
/// - Built from one lowered CoreIR module.
///
/// Output:
/// - Alias sets for std modules whose executable operations are target-gated.
///
/// Transformation:
/// - Converts import declarations into both fully-qualified and short call-head
///   names so operation diagnostics can distinguish std APIs from unrelated
///   local modules with the same final segment.
struct StdCallHeads {
    task: HashSet<String>,
    beam_agent: HashSet<String>,
    beam_gen_server: HashSet<String>,
    beam_native_bridge: HashSet<String>,
    beam_supervisor: HashSet<String>,
    beam_task: HashSet<String>,
}

/// Collects target-gated std module call-head aliases.
///
/// Inputs:
/// - `module`: CoreIR module whose imports define visible remote-call heads.
///
/// Output:
/// - `StdCallHeads` containing aliases for currently target-gated std modules.
///
/// Transformation:
/// - Aggregates individual std call-head collectors behind one context object
///   so expression validation can grow without adding a new parameter for
///   every std runtime family.
fn std_call_heads(module: &CoreModule) -> StdCallHeads {
    StdCallHeads {
        task: std_module_call_heads(module, "std.core.Task", "Task"),
        beam_agent: std_module_call_heads(module, "std.beam.Agent", "Agent"),
        beam_gen_server: std_module_call_heads(module, "std.beam.GenServer", "GenServer"),
        beam_native_bridge: std_module_call_heads(module, "std.beam.NativeBridge", "NativeBridge"),
        beam_supervisor: std_module_call_heads(module, "std.beam.Supervisor", "Supervisor"),
        beam_task: std_module_call_heads(module, "std.beam.Task", "Task"),
    }
}

/// Collects module call heads that refer to an imported std runtime contract.
///
/// Inputs:
/// - `module`: CoreIR module whose imports define visible remote-call heads.
/// - `canonical_module`: fully-qualified std module identity.
/// - `short_head`: unqualified source call head made visible by module imports.
///
/// Output:
/// - Set containing the fully-qualified and short module heads when the exact
///   std module is imported.
///
/// Transformation:
/// - Converts the exact CoreIR import identity into source-call names used by
///   summaries. This lets target-profile validation recognize imported short
///   calls without treating unrelated local modules with the same final segment
///   as std runtime APIs.
fn std_module_call_heads(
    module: &CoreModule,
    canonical_module: &str,
    short_head: &str,
) -> HashSet<String> {
    let mut heads = HashSet::new();
    if module.imports.iter().any(|import| {
        import.kind == CoreImportKind::Module && import.module.as_str() == canonical_module
    }) {
        heads.insert(canonical_module.to_string());
        heads.insert(short_head.to_string());
    }
    heads
}

/// Validates module-level CoreIR import summaries against a target profile.
///
/// Inputs:
/// - `profile`: backend profile requested by the command.
/// - `module`: CoreIR module containing source import summaries.
/// - `violations`: output list for rejected profile features.
///
/// Output:
/// - Appends target-profile violations for unsupported import families.
///
/// Transformation:
/// - Rejects asset imports for normal target-profile compilation. Asset imports
///   require a command-owned resolver, such as static-site rendering, and must
///   not pass through generic backend emission silently.
fn validate_core_imports(
    profile: TargetProfile,
    module: &CoreModule,
    options: TargetProfileCheckOptions,
    violations: &mut Vec<TargetProfileViolation>,
) {
    for import in &module.imports {
        match import.kind {
            CoreImportKind::Module => {
                if is_rust_backed_std_module(&import.module)
                    && !target_profile_supports_rust_backed_std_module(profile, &import.module)
                {
                    violations.push(TargetProfileViolation::unsupported(
                        "rust-backed std module",
                        profile,
                        &format!("module {}", module.module),
                        &import.module,
                    ));
                } else if is_beam_std_module(&import.module)
                    && !target_profile_supports_beam_std_module(profile, &import.module)
                {
                    violations.push(TargetProfileViolation::unsupported(
                        "BEAM std module",
                        profile,
                        &format!("module {}", module.module),
                        &import.module,
                    ));
                }
            }
            CoreImportKind::File | CoreImportKind::Css | CoreImportKind::Markdown => {
                if !options.allow_asset_imports {
                    violations.push(TargetProfileViolation::unsupported(
                        "asset import resolution",
                        profile,
                        &format!("module {}", module.module),
                        &format!("{:?} import `{}`", import.kind, import.module),
                    ));
                }
            }
        }
    }
}

/// Returns whether a std module is portable source API backed first by Rust.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module is a portable std surface whose current executable
///   implementation depends on the Rust/SafeNative bridge.
///
/// Transformation:
/// - Centralizes the small Rust-backed std allowlist so these modules can be
///   visible to docs and summary generation while unsupported target profiles
///   reject them before backend emission.
fn is_rust_backed_std_module(module: &str) -> bool {
    matches!(
        module,
        "std.data.Json" | "std.encoding.Base64" | "std.io.Path" | "std.net.Uri"
    )
}

/// Returns whether a target profile can execute a Rust-backed std module.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified Rust-backed std module path.
///
/// Output:
/// - `true` only when the target profile owns executable lowering for the
///   module's Rust/SafeNative implementation.
///
/// Transformation:
/// - Separates the source-level std contract from executable backend support
///   so importing a module without a target implementation fails with a stable
///   capability diagnostic instead of falling into backend-specific errors.
fn target_profile_supports_rust_backed_std_module(profile: TargetProfile, module: &str) -> bool {
    let _ = (profile, module);
    false
}

/// Returns whether a std module is explicitly tied to BEAM runtime semantics.
///
/// Inputs:
/// - `module`: fully qualified module path from CoreIR import metadata.
///
/// Output:
/// - `true` when the module belongs to the BEAM target-specific std family.
///
/// Transformation:
/// - Classifies BEAM runtime contracts at the module-family boundary so they
///   can be resolved as normal std interfaces while remaining unavailable to
///   portable CoreV0 and future non-BEAM backend profiles.
fn is_beam_std_module(module: &str) -> bool {
    module == "std.beam" || module.starts_with("std.beam.")
}

/// Returns whether a target profile can execute BEAM runtime std modules.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `module`: fully qualified BEAM std module path.
///
/// Output:
/// - `true` only for the current full Erlang/BEAM backend profile.
///
/// Transformation:
/// - Keeps BEAM process, supervision, and bridge contracts out of portable
///   profile subsets while allowing the active Erlang backend to typecheck and
///   validate BEAM-specific source modules.
fn target_profile_supports_beam_std_module(profile: TargetProfile, module: &str) -> bool {
    let _ = module;
    matches!(profile, TargetProfile::Erlang)
}

/// Returns whether a remote call targets a proven std runtime contract.
///
/// Inputs:
/// - `module`: CoreIR remote-call module path or imported call head.
/// - `canonical_module`: fully-qualified std module identity.
/// - `call_heads`: module names proven by imports to refer to this std module.
///
/// Output:
/// - `true` only when the module is the canonical std module or one of the
///   proven imported call heads.
///
/// Transformation:
/// - Centralizes import-derived std runtime identity checks so target-profile
///   validation can grow new runtime std modules without duplicating alias
///   logic for every module family.
fn is_std_runtime_module(
    module: &str,
    canonical_module: &str,
    call_heads: &HashSet<String>,
) -> bool {
    module == canonical_module || call_heads.contains(module)
}

/// Returns whether the current target profile owns a concrete Task operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.core.Task` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Encodes the currently admitted Task subset separately from the type-level
///   Task contract so future runtime operations must be deliberately promoted.
fn target_profile_supports_task_operation(profile: TargetProfile, function: &str) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "done" | "result")
}

/// Returns whether the current target profile owns a concrete Agent operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Agent` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Agent type contract available while forcing executable runtime
///   operations to wait for an explicit BEAM backend implementation.
fn target_profile_supports_beam_agent_operation(profile: TargetProfile, function: &str) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(
            function,
            "start" | "get" | "get_and_update" | "update" | "cast" | "stop"
        )
}

/// Returns whether the current target profile owns a concrete GenServer operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.GenServer` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Admits only the first BEAM callback-process operations implemented
///   through the shared BEAM process intrinsic layer.
fn target_profile_supports_beam_gen_server_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(function, "start" | "call" | "cast" | "stop")
}

/// Returns whether the current target profile owns a concrete NativeBridge operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.NativeBridge` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the NativeBridge callable contract visible to source and summaries
///   while admitting only Erlang-profile operations that have compiler-owned
///   bridge proof lowering.
fn target_profile_supports_beam_native_bridge_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang)
        && matches!(function, "start" | "call" | "dispose" | "stop")
}

/// Returns whether the current target profile owns a concrete Supervisor operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Supervisor` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the Supervisor callable contract visible to source and summaries
///   while admitting only the Erlang profile operations that have local
///   compiler-owned lowering.
fn target_profile_supports_beam_supervisor_operation(
    profile: TargetProfile,
    function: &str,
) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "child_spec" | "start" | "stop")
}

/// Returns whether the current target profile owns a concrete BEAM Task operation.
///
/// Inputs:
/// - `profile`: backend profile under validation.
/// - `function`: source-level `std.beam.Task` operation name.
///
/// Output:
/// - `true` when the operation has executable backend lowering for the profile.
///
/// Transformation:
/// - Keeps the BEAM Task type contract available while rejecting executable
///   task process operations until they are implemented through the shared
///   BEAM process intrinsic layer.
fn target_profile_supports_beam_task_operation(profile: TargetProfile, function: &str) -> bool {
    matches!(profile, TargetProfile::Erlang) && matches!(function, "start" | "result" | "cancel")
}

/// Appends a target-profile violation for one std runtime operation family.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `module`: remote-call module identity.
/// - `function`: remote-call function identity.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching runtime calls
///   that do not have backend support.
///
/// Transformation:
/// - Converts concrete runtime API calls into stable unsupported-target
///   diagnostics while leaving the type-level std contracts available for
///   parsing, resolution, and typechecking. This is the shared target-profile
///   validation path for Task, Agent, and future BEAM process abstractions.
fn validate_std_runtime_operation_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    module: &str,
    function: &str,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if is_std_runtime_module(module, canonical_module, call_heads)
        && !supports_operation(profile, function)
    {
        violations.push(TargetProfileViolation::unsupported(
            diagnostic_label,
            profile,
            &format!("{function_scope} {location}"),
            &format!("{canonical_module}.{function}"),
        ));
    }
}

/// Appends a target-profile violation for summary-only std runtime operations.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `summary`: expression summary that may describe a remote std runtime call.
/// - `call_heads`: module names proven to refer to the std runtime module.
/// - `diagnostic_label`: stable feature label used in diagnostics.
/// - `canonical_module`: fully-qualified std module identity used in messages.
/// - `supports_operation`: policy function for profile/function support.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; a violation is appended for matching remote call
///   summaries without backend support.
///
/// Transformation:
/// - Reads the summary's remote module and first child callee text to detect
///   runtime operations before full typed Core payload lowering exists for that
///   call family. This keeps summary-only and typed payload validation aligned.
fn validate_std_runtime_operation_summary_support(
    profile: TargetProfile,
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    call_heads: &HashSet<String>,
    diagnostic_label: &str,
    canonical_module: &str,
    supports_operation: fn(TargetProfile, &str) -> bool,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if matches!(
        summary.core_expr.as_ref(),
        Some(CoreExpr::RemoteCall { .. } | CoreExpr::Intrinsic(_))
    ) {
        return;
    }

    let Some(module) = summary.remote.as_deref() else {
        return;
    };
    if !is_std_runtime_module(module, canonical_module, call_heads) {
        return;
    }

    let function = summary
        .children
        .first()
        .and_then(|child| child.text.as_deref())
        .unwrap_or("<unknown>");
    if supports_operation(profile, function) {
        return;
    }
    violations.push(TargetProfileViolation::unsupported(
        diagnostic_label,
        profile,
        &format!("{function_scope} {location}"),
        &format!("{canonical_module}.{function}"),
    ));
}

/// Validates one expression summary and its recursive child summaries.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `std_call_heads`: module names proven to refer to target-gated std APIs.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: summary location relative to the function scope.
/// - `summary`: expression summary produced by CoreIR lowering.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks expression proof coverage, runtime-boundary metadata,
///   checked-preservation evidence, typed payload availability, nested child
///   summaries, and typed Core expression shape.
fn validate_core_expr_summary(
    profile: TargetProfile,
    std_call_heads: &StdCallHeads,
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    violations: &mut Vec<TargetProfileViolation>,
) {
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.task,
        "task operation",
        "std.core.Task",
        target_profile_supports_task_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_agent,
        "BEAM Agent operation",
        "std.beam.Agent",
        target_profile_supports_beam_agent_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_gen_server,
        "BEAM GenServer operation",
        "std.beam.GenServer",
        target_profile_supports_beam_gen_server_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_native_bridge,
        "BEAM NativeBridge operation",
        "std.beam.NativeBridge",
        target_profile_supports_beam_native_bridge_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_supervisor,
        "BEAM Supervisor operation",
        "std.beam.Supervisor",
        target_profile_supports_beam_supervisor_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_task,
        "BEAM Task operation",
        "std.beam.Task",
        target_profile_supports_beam_task_operation,
        violations,
    );

    if !profile.allows_expr_coverage(summary.proof_coverage) {
        violations.push(TargetProfileViolation::unsupported(
            "expression coverage",
            profile,
            &format!("{function_scope} {location}"),
            &format!("{cover:?}", cover = summary.proof_coverage),
        ));
    }

    if summary.remote.is_some() && !profile.allows_runtime_boundary() {
        violations.push(TargetProfileViolation::unsupported(
            "runtime boundary",
            profile,
            &format!("{function_scope} {location}"),
            "remote call target",
        ));
    }

    if profile.requires_checked_preservation_evidence()
        && summary.core_expr.is_some()
        && summary.checked_preservation_evidence.is_none()
    {
        violations.push(TargetProfileViolation::missing_evidence(
            profile,
            &format!("{function_scope} {location}"),
            "for typed expression payload",
        ));
    }

    if !profile.allows_expr_summary_kind(summary) {
        violations.push(TargetProfileViolation::unsupported(
            "expression kind",
            profile,
            &format!("{function_scope} {location}"),
            &summary.kind,
        ));
    }

    if !profile.allows_expr_shape_if_present(summary) {
        violations.push(TargetProfileViolation::unsupported(
            "expression shape",
            profile,
            &format!("{function_scope} {location}"),
            "missing typed payload",
        ));
    }

    for (index, child) in summary.children.iter().enumerate() {
        validate_core_expr_summary(
            profile,
            std_call_heads,
            function_scope,
            &format!("{location} child[{index}]"),
            child,
            violations,
        );
    }

    if let Some(expr) = summary.core_expr.as_ref() {
        validate_core_expr(
            profile,
            std_call_heads,
            function_scope,
            location,
            expr,
            violations,
        );
    }
}

/// Validates one typed Core expression and recursively validates contained
/// expressions and patterns.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `std_call_heads`: module names proven to refer to target-gated std APIs.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `expr`: typed Core expression payload.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks the expression shape against the profile matrix and walks every
///   nested typed expression or pattern payload reachable from the node.
fn validate_core_expr(
    profile: TargetProfile,
    std_call_heads: &StdCallHeads,
    function_scope: &str,
    location: &str,
    expr: &CoreExpr,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if let CoreExpr::Binary(value) = expr {
        if binary_expr_requires_segment_lowering(value) {
            violations.push(TargetProfileViolation::unsupported(
                "binary segment lowering",
                profile,
                &format!("{function_scope} {location}"),
                value,
            ));
        }
    }

    if !profile.allows_expr_shape(expr) {
        violations.push(TargetProfileViolation::unsupported(
            "typed expression shape",
            profile,
            &format!("{function_scope} {location}"),
            &format!("{expr:?}"),
        ));
    }

    match expr {
        CoreExpr::Int(_) => {}
        CoreExpr::Float(_) => {}
        CoreExpr::Binary(_) => {}
        CoreExpr::Atom(_) => {}
        CoreExpr::Var(_) => {}
        CoreExpr::Tuple(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "tuple",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::List(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "list",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::ListCons { head, tail } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list head",
                head,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list tail",
                tail,
                violations,
            );
        }
        CoreExpr::FixedArray(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "fixed array",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::Index { base, index } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "index base",
                base,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "index value",
                index,
                violations,
            );
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list comprehension expr",
                expr,
                violations,
            );
            validate_core_pattern(profile, pattern, "list comprehension pattern", violations);
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list comprehension source",
                source,
                violations,
            );
            if let Some(guard) = guard {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "list comprehension guard",
                    guard,
                    violations,
                );
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "let binding value",
                    &binding.value,
                    violations,
                );
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "let body",
                body,
                violations,
            );
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "map field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. } => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "record field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::FieldAccess { base, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "field access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordAccess { base, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "record access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "record update base",
                base,
                violations,
            );
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "record update field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "template prop",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "constructor chain arg",
                    arg,
                    violations,
                );
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "constructor chain record",
                record,
                violations,
            );
        }
        CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.task,
                "task operation",
                "std.core.Task",
                target_profile_supports_task_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_agent,
                "BEAM Agent operation",
                "std.beam.Agent",
                target_profile_supports_beam_agent_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_gen_server,
                "BEAM GenServer operation",
                "std.beam.GenServer",
                target_profile_supports_beam_gen_server_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_native_bridge,
                "BEAM NativeBridge operation",
                "std.beam.NativeBridge",
                target_profile_supports_beam_native_bridge_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_supervisor,
                "BEAM Supervisor operation",
                "std.beam.Supervisor",
                target_profile_supports_beam_supervisor_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_task,
                "BEAM Task operation",
                "std.beam.Task",
                target_profile_supports_beam_task_operation,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "remote call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "constructor call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Call { args, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "mutable receiver call receiver",
                receiver,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "mutable receiver call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "function call callee",
                callee,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "function call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &call.args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "intrinsic arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "case scrutinee",
                scrutinee,
                violations,
            );
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "case clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "case clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(profile, &clause.pattern, "case clause pattern", violations);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "try body",
                body,
                violations,
            );
            for clause in of_clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "try of clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try of clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(
                    profile,
                    &clause.pattern,
                    "try of clause pattern",
                    violations,
                );
            }
            for clause in catch_clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "try catch clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try catch clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(
                    profile,
                    &clause.pattern,
                    "try catch clause pattern",
                    violations,
                );
            }
            if let Some(after_clause) = after_clause {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try after trigger",
                    &after_clause.trigger,
                    violations,
                );
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try after body",
                    &after_clause.body,
                    violations,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "if clause condition",
                    &clause.condition,
                    violations,
                );
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "if clause body",
                    &clause.body,
                    violations,
                );
            }
        }
        CoreExpr::Lam { params, body, .. } => {
            for param in params {
                validate_core_pattern(profile, param, "function parameter", violations);
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "lambda body",
                body,
                violations,
            );
        }
        CoreExpr::UnaryOp { operand, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "unary operand",
                operand,
                violations,
            );
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "binary left",
                left,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "binary right",
                right,
                violations,
            );
        }
    }
}

/// Returns whether a binary literal still requires segment semantic lowering.
///
/// Inputs:
/// - `value`: source-preserved binary text from `CoreExpr::Binary`.
///
/// Output:
/// - `true` when the value uses structured `<<...>>` segment syntax beyond a
///   single string segment.
/// - `false` for plain string literals, empty binaries, and `<<"text">>`
///   string-only binary literals.
///
/// Transformation:
/// - Classifies source text without parsing segment semantics so target-profile
///   validation can reject deferred binary segment lowering before backend
///   emission.
fn binary_expr_requires_segment_lowering(value: &str) -> bool {
    let trimmed = value.trim();
    if !(trimmed.starts_with("<<") && trimmed.ends_with(">>")) {
        return false;
    }

    let inner = trimmed[2..trimmed.len() - 2].trim();
    if inner.is_empty() {
        return false;
    }

    !binary_expr_is_single_string_segment(inner)
}

/// Returns whether binary contents are exactly one string literal segment.
///
/// Inputs:
/// - `inner`: text between `<<` and `>>`.
///
/// Output:
/// - `true` when `inner` is one complete double-quoted string literal.
///
/// Transformation:
/// - Scans the string literal with escape handling and rejects trailing segment
///   modifiers, commas, or additional source text.
fn binary_expr_is_single_string_segment(inner: &str) -> bool {
    let mut chars = inner.char_indices();
    if chars.next().is_none_or(|(_, ch)| ch != '"') {
        return false;
    }

    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return inner[index + ch.len_utf8()..].trim().is_empty();
        }
    }

    false
}

/// Validates one typed Core pattern and recursively validates contained
/// patterns.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `pattern`: typed Core pattern payload.
/// - `location`: pattern location label for diagnostics.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks the pattern shape against the profile matrix and walks every nested
///   typed pattern payload reachable from the node.
fn validate_core_pattern(
    profile: TargetProfile,
    pattern: &CorePattern,
    location: &str,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if !profile.allows_pattern_shape(pattern) {
        violations.push(TargetProfileViolation::unsupported(
            "pattern shape",
            profile,
            location,
            &format!("{pattern:?}"),
        ));
    }

    match pattern {
        CorePattern::Wildcard => {}
        CorePattern::Var(_) => {}
        CorePattern::Int(_) => {}
        CorePattern::Float(_) => {}
        CorePattern::Atom(_) => {}
        CorePattern::Tuple(values) => {
            for value in values {
                validate_core_pattern(profile, value, "tuple", violations);
            }
        }
        CorePattern::List(values) => {
            for value in values {
                validate_core_pattern(profile, value, "list", violations);
            }
        }
        CorePattern::ListCons { head, tail } => {
            validate_core_pattern(profile, head, "list head", violations);
            validate_core_pattern(profile, tail, "list tail", violations);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                validate_core_pattern(profile, &field.value, "map field", violations);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                validate_core_pattern(profile, &field.value, "record field", violations);
            }
        }
        CorePattern::Constructor { args, .. } => {
            for arg in args {
                validate_core_pattern(profile, arg, "constructor", violations);
            }
        }
    }
}

trait ProfileExprShapeExtensions {
    fn allows_expr_summary_kind(self, summary: &CoreExprSummary) -> bool;
    fn allows_expr_shape_if_present(self, summary: &CoreExprSummary) -> bool;
}

impl ProfileExprShapeExtensions for TargetProfile {
    /// Returns whether a syntax-summary expression kind belongs to this profile.
    ///
    /// Inputs:
    /// - `summary`: expression summary from typed lowering.
    ///
    /// Output:
    /// - `true` when the profile admits the source expression family.
    ///
    /// Transformation:
    /// - Gates dedicated function-value invocation syntax so earlier successor
    ///   profiles do not inherit `f.(args)` merely because it lowers to the
    ///   same backend call payload as local named calls.
    fn allows_expr_summary_kind(self, summary: &CoreExprSummary) -> bool {
        if summary.kind != "FunctionCall" {
            return true;
        }

        matches!(
            self,
            Self::Erlang
                | Self::A016Erlang
                | Self::A017Erlang
                | Self::A018Erlang
                | Self::A019Erlang
                | Self::A020Erlang
                | Self::A021Erlang
        )
    }

    /// Returns whether a summary payload shape is acceptable when a typed payload
    /// exists for the current profile.
    ///
    /// Inputs:
    /// - `summary`: expression summary from typed lowering.
    ///
    /// Output:
    /// - `false` if typed payload is missing and profile requires typed payload
    ///   for the observed proof class.
    fn allows_expr_shape_if_present(self, summary: &CoreExprSummary) -> bool {
        if summary.core_expr.is_some() {
            return true;
        }

        match self {
            Self::Erlang => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => summary.core_expr.is_some(),
            Self::CoreV0 => summary.core_expr.is_some(),
        }
    }
}

#[cfg(test)]
mod target_profile_test;
