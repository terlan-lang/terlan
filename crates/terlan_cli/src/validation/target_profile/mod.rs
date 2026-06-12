use terlan_typeck::CoreExpr;
use terlan_typeck::CoreExprSummary;
use terlan_typeck::CoreImportKind;
use terlan_typeck::CoreModule;
use terlan_typeck::CorePattern;
use terlan_typeck::CoreProofCoverage;

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
                | CoreExpr::Intrinsic(_)
                | CoreExpr::Receive { .. }
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

            validate_core_expr_summary(profile, &context, "body", &clause.body, &mut violations);
            if let Some(guard) = &clause.guard {
                validate_core_expr_summary(profile, &context, "guard", guard, &mut violations);
            }
        }
    }

    violations
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
            CoreImportKind::Module => {}
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

/// Validates one expression summary and its recursive child summaries.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
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
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    violations: &mut Vec<TargetProfileViolation>,
) {
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
            function_scope,
            &format!("{location} child[{index}]"),
            child,
            violations,
        );
    }

    if let Some(expr) = summary.core_expr.as_ref() {
        validate_core_expr(profile, function_scope, location, expr, violations);
    }
}

/// Validates one typed Core expression and recursively validates contained
/// expressions and patterns.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
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
                validate_core_expr(profile, function_scope, "tuple", value, violations)
            });
        }
        CoreExpr::List(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(profile, function_scope, "list", value, violations)
            });
        }
        CoreExpr::ListCons { head, tail } => {
            validate_core_expr(profile, function_scope, "list head", head, violations);
            validate_core_expr(profile, function_scope, "list tail", tail, violations);
        }
        CoreExpr::FixedArray(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(profile, function_scope, "fixed array", value, violations)
            });
        }
        CoreExpr::Index { base, index } => {
            validate_core_expr(profile, function_scope, "index base", base, violations);
            validate_core_expr(profile, function_scope, "index value", index, violations);
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            validate_core_expr(
                profile,
                function_scope,
                "list comprehension expr",
                expr,
                violations,
            );
            validate_core_pattern(profile, pattern, "list comprehension pattern", violations);
            validate_core_expr(
                profile,
                function_scope,
                "list comprehension source",
                source,
                violations,
            );
            if let Some(guard) = guard {
                validate_core_expr(
                    profile,
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
                    function_scope,
                    "let binding value",
                    &binding.value,
                    violations,
                );
            }
            validate_core_expr(profile, function_scope, "let body", body, violations);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                validate_core_expr(
                    profile,
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
                function_scope,
                "field access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordAccess { base, .. } => {
            validate_core_expr(
                profile,
                function_scope,
                "record access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            validate_core_expr(
                profile,
                function_scope,
                "record update base",
                base,
                violations,
            );
            for field in fields {
                validate_core_expr(
                    profile,
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
                    function_scope,
                    "constructor chain arg",
                    arg,
                    violations,
                );
            }
            validate_core_expr(
                profile,
                function_scope,
                "constructor chain record",
                record,
                violations,
            );
        }
        CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::RemoteCall { args, .. } => {
            for arg in args {
                validate_core_expr(profile, function_scope, "remote call arg", arg, violations);
            }
        }
        CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    function_scope,
                    "constructor call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Call { args, .. } => {
            for arg in args {
                validate_core_expr(profile, function_scope, "call arg", arg, violations);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            validate_core_expr(
                profile,
                function_scope,
                "function call callee",
                callee,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    function_scope,
                    "function call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &call.args {
                validate_core_expr(profile, function_scope, "intrinsic arg", arg, violations);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            validate_core_expr(
                profile,
                function_scope,
                "case scrutinee",
                scrutinee,
                violations,
            );
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        function_scope,
                        "case clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    function_scope,
                    "case clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(profile, &clause.pattern, "case clause pattern", violations);
            }
        }
        CoreExpr::Receive {
            clauses,
            after_clause,
        } => {
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        function_scope,
                        "receive clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    function_scope,
                    "receive clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(
                    profile,
                    &clause.pattern,
                    "receive clause pattern",
                    violations,
                );
            }
            if let Some(after_clause) = after_clause {
                validate_core_expr(
                    profile,
                    function_scope,
                    "receive after trigger",
                    &after_clause.trigger,
                    violations,
                );
                validate_core_expr(
                    profile,
                    function_scope,
                    "receive after body",
                    &after_clause.body,
                    violations,
                );
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            validate_core_expr(profile, function_scope, "try body", body, violations);
            for clause in of_clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        function_scope,
                        "try of clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
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
                        function_scope,
                        "try catch clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
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
                    function_scope,
                    "try after trigger",
                    &after_clause.trigger,
                    violations,
                );
                validate_core_expr(
                    profile,
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
                    function_scope,
                    "if clause condition",
                    &clause.condition,
                    violations,
                );
                validate_core_expr(
                    profile,
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
            validate_core_expr(profile, function_scope, "lambda body", body, violations);
        }
        CoreExpr::UnaryOp { operand, .. } => {
            validate_core_expr(
                profile,
                function_scope,
                "unary operand",
                operand,
                violations,
            );
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            validate_core_expr(profile, function_scope, "binary left", left, violations);
            validate_core_expr(profile, function_scope, "binary right", right, violations);
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
mod tests {
    use super::*;

    use std::collections::{HashMap, HashSet};

    use terlan_hir::{
        load_interfaces_from_file_set, resolve_syntax_module_output_with_interfaces,
        ModuleInterface,
    };
    use terlan_syntax::{parse_module_as_syntax_output, SyntaxModuleOutput};
    use terlan_typeck::{
        CoreCaseClause, CoreCheckedPreservationEvidence, CoreCheckedPreservationEvidenceKind,
        CoreFunction, CoreFunctionClause, CoreIfClause, CoreModuleMetadata, CoreParam,
        CoreProofReadiness, CoreSourceIdentity, CoreSubstitutionFreshnessEvidence, CORE_IR_SCHEMA,
    };

    /// Lowers source text to a typed Core module through the formal syntax-output
    /// path.
    ///
    /// Inputs:
    /// - `source`: Terlan source module text.
    /// - `path`: synthetic source path used for interface lookup identity.
    ///
    /// Output:
    /// - Lowered `CoreModule` containing expression and pattern summaries.
    ///
    /// Transformation:
    /// - Parses source as syntax output, resolves it with file-set interfaces,
    ///   and lowers the result to backend-agnostic CoreIR.
    fn lower(source: &str, path: &str) -> CoreModule {
        let syntax: SyntaxModuleOutput =
            parse_module_as_syntax_output(source).expect("parse syntax output");
        let interfaces = load_interfaces_from_file_set(path);
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved)
    }

    /// Verifies target profiles reject asset imports that need command-owned
    /// filesystem resolution.
    ///
    /// Inputs:
    /// - A source module with a CSS asset import and a simple function body.
    ///
    /// Output:
    /// - Test passes when Erlang target-profile validation reports a stable
    ///   unsupported asset-import-resolution diagnostic.
    ///
    /// Transformation:
    /// - Lowers the parsed module through CoreIR, preserving the import kind,
    ///   then validates that generic backend compilation does not silently
    ///   accept the unresolved asset import.
    #[test]
    fn rejects_asset_import_resolution_for_generic_target_profile() {
        let module = lower(
            "module profile_asset_import.\n\nimport css \"./style.css\" as PageCss.\n\npub main(): Int ->\n    1.\n",
            "profile_asset_import.tl",
        );

        let violations = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            violations.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation
                        .message
                        .contains("asset import resolution Css import `PageCss<-./style.css`")
            }),
            "expected asset import target-profile diagnostic, got {violations:?}"
        );
    }

    /// Builds a Lean-covered expression summary for direct target-profile tests.
    ///
    /// Inputs:
    /// - `expr`: typed Core expression shape under test.
    ///
    /// Output:
    /// - `CoreExprSummary` carrying the expression as a typed payload.
    ///
    /// Transformation:
    /// - Wraps the expression in minimal Lean-covered summary metadata without
    ///   adding child summaries or runtime-boundary annotations.
    fn lean_expr_summary(expr: CoreExpr) -> CoreExprSummary {
        CoreExprSummary {
            kind: "direct-test".to_string(),
            core_expr: Some(expr),
            checked_preservation_evidence: Some(expr_evidence("direct-test")),
            proof_coverage: CoreProofCoverage::LeanCovered,
            text: None,
            remote: None,
            operator: None,
            arity: 0,
            children: Vec::new(),
        }
    }

    /// Builds structural checked-preservation evidence for direct expression
    /// profile tests.
    ///
    /// Inputs:
    /// - `target`: stable evidence target label.
    ///
    /// Output:
    /// - `CoreCheckedPreservationEvidence` for a typed expression payload.
    ///
    /// Transformation:
    /// - Creates structural expression evidence with a conservative
    ///   runtime-bindings-required freshness marker.
    fn expr_evidence(target: &str) -> CoreCheckedPreservationEvidence {
        CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCoreExpr,
            freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
            target: target.to_string(),
        }
    }

    /// Builds structural checked-preservation evidence for direct pattern
    /// profile tests.
    ///
    /// Inputs:
    /// - `target`: stable evidence target label.
    ///
    /// Output:
    /// - `CoreCheckedPreservationEvidence` for a typed pattern payload.
    ///
    /// Transformation:
    /// - Creates structural pattern evidence with a conservative
    ///   runtime-bindings-required freshness marker.
    fn pattern_evidence(target: &str) -> CoreCheckedPreservationEvidence {
        CoreCheckedPreservationEvidence {
            kind: CoreCheckedPreservationEvidenceKind::StructuralCorePattern,
            freshness: CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired,
            target: target.to_string(),
        }
    }

    /// Builds zeroed Lean-covered module metadata for direct profile tests.
    ///
    /// Inputs:
    /// - No runtime input.
    ///
    /// Output:
    /// - `CoreModuleMetadata` with no unresolved constructor candidates.
    ///
    /// Transformation:
    /// - Creates metadata sufficient for target-profile validation, where only
    ///   constructor-resolution counters are consumed by the validator.
    fn lean_core_metadata() -> CoreModuleMetadata {
        CoreModuleMetadata {
            interface_function_count: 1,
            interface_type_count: 0,
            constructor_count: 0,
            proof_readiness: CoreProofReadiness::LeanCovered,
            lean_covered_expr_count: 1,
            partial_expr_count: 0,
            proof_model_required_expr_count: 0,
            runtime_boundary_expr_count: 0,
            artifact_only_expr_count: 0,
            lean_covered_pattern_count: 1,
            partial_pattern_count: 0,
            proof_model_required_pattern_count: 0,
            runtime_boundary_pattern_count: 0,
            artifact_only_pattern_count: 0,
            typed_core_expr_count: 1,
            summary_only_expr_count: 0,
            typed_core_pattern_count: 1,
            summary_only_pattern_count: 0,
            typed_core_type_count: 0,
            summary_only_type_count: 0,
            checked_preservation_expr_count: 0,
            checked_preservation_pattern_count: 0,
            checked_preservation_expr_structural_count: 0,
            checked_preservation_pattern_structural_count: 0,
            checked_preservation_expr_no_runtime_bindings_count: 0,
            checked_preservation_pattern_no_runtime_bindings_count: 0,
            checked_preservation_expr_runtime_bindings_required_count: 0,
            checked_preservation_pattern_runtime_bindings_required_count: 0,
            resolved_constructor_call_identity_count: 0,
            resolved_constructor_chain_identity_count: 0,
            resolved_constructor_pattern_identity_count: 0,
            unresolved_constructor_call_candidate_count: 0,
            unresolved_constructor_chain_candidate_count: 0,
            unresolved_constructor_pattern_candidate_count: 0,
        }
    }

    /// Builds an empty interface for direct CoreIR profile tests.
    ///
    /// Inputs:
    /// - `module`: module name to attach to the interface.
    ///
    /// Output:
    /// - Empty `ModuleInterface` with no public declarations.
    ///
    /// Transformation:
    /// - Creates deterministic empty declaration maps and sets for tests that
    ///   do not inspect interface rendering.
    fn empty_interface(module: &str) -> ModuleInterface {
        ModuleInterface {
            module: module.to_string(),
            docs: Vec::new(),
            public_types: HashSet::new(),
            private_types: HashSet::new(),
            opaque_types: HashSet::new(),
            type_params: HashMap::new(),
            type_bodies: HashMap::new(),
            type_docs: HashMap::new(),
            traits: HashMap::new(),
            constructors: HashMap::new(),
            functions: HashMap::new(),
        }
    }

    /// Builds a minimal Core module around one typed expression body.
    ///
    /// Inputs:
    /// - `body`: typed Core expression to validate as a function body.
    ///
    /// Output:
    /// - `CoreModule` containing one public unary function.
    ///
    /// Transformation:
    /// - Wraps the body in a Lean-covered clause with one variable pattern and
    ///   zero unresolved constructor metadata.
    fn module_with_core_body(body: CoreExpr) -> CoreModule {
        module_with_core_body_and_evidence(
            body,
            Some(expr_evidence("direct-test")),
            vec![Some(pattern_evidence("input"))],
        )
    }

    /// Builds a direct Core module with caller-selected unresolved constructor
    /// metadata counters.
    ///
    /// Inputs:
    /// - `call_candidates`: unresolved constructor-call candidate count.
    /// - `chain_candidates`: unresolved constructor-chain candidate count.
    /// - `pattern_candidates`: unresolved constructor-pattern candidate count.
    ///
    /// Output:
    /// - `CoreModule` with a Lean-covered integer body and the provided
    ///   unresolved constructor metadata counts.
    ///
    /// Transformation:
    /// - Starts from the standard direct CoreIR test fixture and mutates only
    ///   constructor-resolution counters, isolating target-profile validation
    ///   from parser and typechecker diagnostics.
    fn module_with_unresolved_constructor_candidates(
        call_candidates: usize,
        chain_candidates: usize,
        pattern_candidates: usize,
    ) -> CoreModule {
        let mut module = module_with_core_body(CoreExpr::Int(0));
        module.metadata.unresolved_constructor_call_candidate_count = call_candidates;
        module.metadata.unresolved_constructor_chain_candidate_count = chain_candidates;
        module
            .metadata
            .unresolved_constructor_pattern_candidate_count = pattern_candidates;
        module
    }

    /// Asserts the unresolved-constructor target-profile diagnostic is present
    /// with exact counter details.
    ///
    /// Inputs:
    /// - `violations`: validation output returned by `target_profile_checks`.
    /// - `calls`: expected unresolved constructor-call candidate count.
    /// - `chains`: expected unresolved constructor-chain candidate count.
    /// - `patterns`: expected unresolved constructor-pattern candidate count.
    ///
    /// Output:
    /// - Test assertion only; no compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Locates the shared unresolved-constructor diagnostic by code and
    ///   compares its formatted message against the expected profile/count
    ///   payload.
    fn assert_unresolved_constructor_violation(
        violations: &[TargetProfileViolation],
        calls: usize,
        chains: usize,
        patterns: usize,
    ) {
        let violation = violations
            .iter()
            .find(|violation| violation.code == TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE)
            .unwrap_or_else(|| {
                panic!(
                    "Erlang profile should reject unresolved constructor candidates: {:?}",
                    violations
                )
            });
        assert_eq!(
            violation.message,
            unresolved_constructor_message(TargetProfile::Erlang, calls, chains, patterns),
            "unexpected unresolved constructor diagnostic message"
        );
    }

    /// Builds a minimal Core module around one typed expression body and caller
    /// supplied preservation evidence.
    ///
    /// Inputs:
    /// - `body`: typed Core expression to validate as a function body.
    /// - `body_evidence`: checked-preservation evidence attached to the body
    ///   summary.
    /// - `pattern_evidence`: checked-preservation evidence attached to the
    ///   single function-clause pattern.
    ///
    /// Output:
    /// - `CoreModule` containing one public unary function.
    ///
    /// Transformation:
    /// - Wraps the body in a Lean-covered clause with one variable pattern and
    ///   caller-controlled preservation evidence.
    fn module_with_core_body_and_evidence(
        body: CoreExpr,
        body_evidence: Option<CoreCheckedPreservationEvidence>,
        pattern_evidence: Vec<Option<CoreCheckedPreservationEvidence>>,
    ) -> CoreModule {
        let module_name = "profile_test_core_v0_direct".to_string();
        CoreModule {
            schema: CORE_IR_SCHEMA.to_string(),
            module: module_name.clone(),
            source: CoreSourceIdentity {
                source_kind: "direct_profile_test".to_string(),
                syntax_contract_fingerprint: None,
            },
            imports: Vec::new(),
            exports: Vec::new(),
            types: Vec::new(),
            functions: vec![CoreFunction {
                name: "value".to_string(),
                arity: 1,
                public: true,
                params: vec![CoreParam {
                    name: "input".to_string(),
                    ty: "Dynamic".to_string(),
                    core_ty: None,
                }],
                return_type: "Dynamic".to_string(),
                core_return_type: None,
                clauses: vec![CoreFunctionClause {
                    patterns: vec!["input".to_string()],
                    core_patterns: vec![Some(CorePattern::Var("input".to_string()))],
                    pattern_proof_coverage: vec![CoreProofCoverage::LeanCovered],
                    pattern_checked_preservation_evidence: pattern_evidence,
                    guard: None,
                    body: CoreExprSummary {
                        checked_preservation_evidence: body_evidence,
                        ..lean_expr_summary(body)
                    },
                }],
            }],
            constructors: Vec::new(),
            trait_conformances: Vec::new(),
            metadata: lean_core_metadata(),
            interface: empty_interface(&module_name),
        }
    }

    /// Verifies CoreV0 accepts the documented portable expression and pattern
    /// subset.
    ///
    /// Inputs:
    /// - A directly constructed typed Core expression using case, if, call,
    ///   lambda, field access, constructor call, tuple/list/list-cons, and
    ///   arithmetic/comparison operators.
    ///
    /// Output:
    /// - Test assertion only; no source fixtures or compiler artifacts are
    ///   written.
    ///
    /// Transformation:
    /// - Wraps accepted CoreIR shapes in a minimal `CoreModule` and validates
    ///   it under `TargetProfile::CoreV0`.
    #[test]
    fn target_profile_accepts_documented_core_v0_shape_matrix() {
        let body = CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("input".to_string())),
            clauses: vec![
                CoreCaseClause {
                    pattern: CorePattern::Tuple(vec![
                        CorePattern::Int(0),
                        CorePattern::Atom("zero".to_string()),
                    ]),
                    guard: None,
                    body: CoreExpr::Tuple(vec![
                        CoreExpr::Binary("zero".to_string()),
                        CoreExpr::List(vec![CoreExpr::Int(0), CoreExpr::Int(1)]),
                        CoreExpr::UnaryOp {
                            operator: "-".to_string(),
                            operand: Box::new(CoreExpr::Int(1)),
                        },
                    ]),
                },
                CoreCaseClause {
                    pattern: CorePattern::Constructor {
                        name: "Ok".to_string(),
                        constructor_identity: Some("Ok/1".to_string()),
                        args: vec![CorePattern::List(vec![CorePattern::Var(
                            "value".to_string(),
                        )])],
                    },
                    guard: None,
                    body: CoreExpr::If {
                        clauses: vec![
                            CoreIfClause {
                                condition: CoreExpr::BinaryOp {
                                    operator: "==".to_string(),
                                    left: Box::new(CoreExpr::Var("value".to_string())),
                                    right: Box::new(CoreExpr::Int(0)),
                                },
                                body: CoreExpr::Call {
                                    function: "identity".to_string(),
                                    args: vec![CoreExpr::ListCons {
                                        head: Box::new(CoreExpr::Int(1)),
                                        tail: Box::new(CoreExpr::List(Vec::new())),
                                    }],
                                },
                            },
                            CoreIfClause {
                                condition: CoreExpr::Atom("true".to_string()),
                                body: CoreExpr::ConstructorCall {
                                    constructor: "Ok".to_string(),
                                    constructor_identity: Some("Ok/1".to_string()),
                                    args: vec![CoreExpr::Lam {
                                        params: vec![CorePattern::Var("x".to_string())],
                                        body: Box::new(CoreExpr::FieldAccess {
                                            base: Box::new(CoreExpr::Var("x".to_string())),
                                            field: "name".to_string(),
                                        }),
                                    }],
                                },
                            },
                        ],
                    },
                },
            ],
        };
        let module = module_with_core_body(body);

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0.is_empty(),
            "CoreV0 profile should accept the documented portable shape matrix: {:?}",
            core_v0
        );
    }

    /// Verifies CoreV0 rejects typed expression payloads without
    /// checked-preservation evidence.
    ///
    /// Inputs:
    /// - A directly constructed Core module with a Lean-covered typed
    ///   expression payload and no expression evidence.
    ///
    /// Output:
    /// - Test assertion only; no source fixtures or compiler artifacts are
    ///   written.
    ///
    /// Transformation:
    /// - Runs target-profile validation over the direct CoreIR module and
    ///   checks for the missing-evidence diagnostic.
    #[test]
    fn target_profile_rejects_missing_expr_evidence_for_core_v0_profile() {
        let module = module_with_core_body_and_evidence(
            CoreExpr::Int(1),
            None,
            vec![Some(pattern_evidence("input"))],
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0.iter().any(
                |violation| violation.code == "target_profile_missing_evidence"
                    && violation.message.contains("typed expression payload")
            ),
            "CoreV0 profile should reject missing expression evidence: {:?}",
            core_v0
        );
    }

    /// Verifies CoreV0 rejects typed pattern payloads without
    /// checked-preservation evidence.
    ///
    /// Inputs:
    /// - A directly constructed Core module with a Lean-covered typed pattern
    ///   payload and no pattern evidence.
    ///
    /// Output:
    /// - Test assertion only; no source fixtures or compiler artifacts are
    ///   written.
    ///
    /// Transformation:
    /// - Runs target-profile validation over the direct CoreIR module and
    ///   checks for the missing-evidence diagnostic.
    #[test]
    fn target_profile_rejects_missing_pattern_evidence_for_core_v0_profile() {
        let module = module_with_core_body_and_evidence(
            CoreExpr::Int(1),
            Some(expr_evidence("body")),
            vec![None],
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0.iter().any(
                |violation| violation.code == "target_profile_missing_evidence"
                    && violation.message.contains("typed pattern payload")
            ),
            "CoreV0 profile should reject missing pattern evidence: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_accepts_float_for_erlang_profile() {
        let module = lower(
            "\
module profile_test.\n\npub f(): Int ->\n    1.0.\n",
            "src/profile_test.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should currently accept permissive coverage"
        );
    }

    /// Verifies the A0 Erlang target profile accepts the frozen arithmetic
    /// fixture shape.
    ///
    /// Inputs:
    /// - Source containing one public function with an `Int` parameter, `Int`
    ///   return annotation, and integer addition body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the frozen A0 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_mathx_for_a0_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_mathx.\n\npub add(x: Int): Int ->\n    x + 1.\n",
            "src/profile_test_a0_mathx.tl",
        );

        let a0 = target_profile_checks(&module, TargetProfile::A0Erlang);

        assert!(
            a0.is_empty(),
            "A0 Erlang profile should accept the frozen arithmetic shape: {:?}",
            a0
        );
    }

    /// Verifies the A0 Erlang target profile reports a stable unsupported-form
    /// diagnostic for features outside the frozen fixture matrix.
    ///
    /// Inputs:
    /// - Source containing a binary/string literal body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that an excluded expression shape is rejected with a stable diagnostic.
    #[test]
    fn target_profile_rejects_binary_for_a0_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
            "src/profile_test_a0_binary.tl",
        );

        let a0 = target_profile_checks(&module, TargetProfile::A0Erlang);

        assert!(
            a0.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0 Erlang profile should reject excluded binary/string literals: {:?}",
            a0
        );
    }

    /// Verifies the named A0.1 Erlang successor profile accepts simple Int
    /// arithmetic and comparison expressions.
    ///
    /// Inputs:
    /// - Source containing multiplication, subtraction, and greater-than over
    ///   `Int` parameters.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.1 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_arithmetic_for_a0_1_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_1_arithmetic.\n\npub bigger(x: Int, y: Int): Bool ->\n    x * 2 - 1 > y.\n",
            "src/profile_test_a0_1_arithmetic.tl",
        );

        let a0_1 = target_profile_checks(&module, TargetProfile::A01Erlang);

        assert!(
            a0_1.is_empty(),
            "A0.1 Erlang profile should accept simple arithmetic/comparison: {:?}",
            a0_1
        );
    }

    /// Verifies the named A0.2 Erlang successor profile accepts boolean
    /// literals and boolean operators on top of the A0.1 arithmetic subset.
    ///
    /// Inputs:
    /// - Source containing `Bool` return annotation, `true`, `and`, `or`, and
    ///   comparison expressions over `Int` parameters.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.2 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_bool_ops_for_a0_2_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_2_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    true and x > 0 or y > 0.\n",
            "src/profile_test_a0_2_bool_ops.tl",
        );

        let a0_2 = target_profile_checks(&module, TargetProfile::A02Erlang);

        assert!(
            a0_2.is_empty(),
            "A0.2 Erlang profile should accept boolean literals/operators: {:?}",
            a0_2
        );
    }

    /// Verifies the named A0.1 Erlang successor profile does not silently widen
    /// to include A0.2 boolean operators.
    ///
    /// Inputs:
    /// - Source containing `and` over comparison expressions.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.1-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.1 remains narrower than the A0.2 successor profile.
    #[test]
    fn target_profile_keeps_bool_ops_out_of_a0_1_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_1_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    x > 0 and y > 0.\n",
            "src/profile_test_a0_1_bool_ops.tl",
        );

        let a0_1 = target_profile_checks(&module, TargetProfile::A01Erlang);

        assert!(
            a0_1.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.1-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.1 Erlang profile should reject A0.2 boolean operators: {:?}",
            a0_1
        );
    }

    /// Verifies the named A0.3 Erlang successor profile accepts simple
    /// conditional expressions over the A0.2 boolean subset.
    ///
    /// Inputs:
    /// - Source containing an `if` expression with comparison and boolean
    ///   literal branch conditions.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.3 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_if_expr_for_a0_3_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_3_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
            "src/profile_test_a0_3_if_expr.tl",
        );

        let a0_3 = target_profile_checks(&module, TargetProfile::A03Erlang);

        assert!(
            a0_3.is_empty(),
            "A0.3 Erlang profile should accept simple if expressions: {:?}",
            a0_3
        );
    }

    /// Verifies the named A0.2 Erlang successor profile does not silently widen
    /// to include A0.3 conditional expressions.
    ///
    /// Inputs:
    /// - Source containing an `if` expression over A0.2-compatible child
    ///   expressions.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.2-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.2 remains narrower than the A0.3 successor profile.
    #[test]
    fn target_profile_keeps_if_expr_out_of_a0_2_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_2_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
            "src/profile_test_a0_2_if_expr.tl",
        );

        let a0_2 = target_profile_checks(&module, TargetProfile::A02Erlang);

        assert!(
            a0_2.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.2-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.2 Erlang profile should reject A0.3 if expressions: {:?}",
            a0_2
        );
    }

    /// Verifies the named A0.4 Erlang successor profile accepts simple case
    /// expressions over integer and variable patterns.
    ///
    /// Inputs:
    /// - Source containing a `case` expression with one integer pattern and one
    ///   variable pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.4 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_case_expr_for_a0_4_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_4_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
            "src/profile_test_a0_4_case_expr.tl",
        );

        let a0_4 = target_profile_checks(&module, TargetProfile::A04Erlang);

        assert!(
            a0_4.is_empty(),
            "A0.4 Erlang profile should accept simple case expressions: {:?}",
            a0_4
        );
    }

    /// Verifies the named A0.3 Erlang successor profile does not silently widen
    /// to include A0.4 case expressions.
    ///
    /// Inputs:
    /// - Source containing a `case` expression over A0.3-compatible child
    ///   expressions.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.3-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.3 remains narrower than the A0.4 successor profile.
    #[test]
    fn target_profile_keeps_case_expr_out_of_a0_3_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_3_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
            "src/profile_test_a0_3_case_expr.tl",
        );

        let a0_3 = target_profile_checks(&module, TargetProfile::A03Erlang);

        assert!(
            a0_3.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.3-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.3 Erlang profile should reject A0.4 case expressions: {:?}",
            a0_3
        );
    }

    /// Verifies the named A0.5 Erlang successor profile accepts raw atom
    /// literals as expression values and case patterns.
    ///
    /// Inputs:
    /// - Source containing a raw atom function body and a case expression with a
    ///   raw atom literal pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.5 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_raw_atoms_for_a0_5_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_5_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
            "src/profile_test_a0_5_raw_atoms.tl",
        );

        let a0_5 = target_profile_checks(&module, TargetProfile::A05Erlang);

        assert!(
            a0_5.is_empty(),
            "A0.5 Erlang profile should accept raw atom literals: {:?}",
            a0_5
        );
    }

    /// Verifies the named A0.4 Erlang successor profile does not silently widen
    /// to include A0.5 raw atom literals.
    ///
    /// Inputs:
    /// - Source containing a raw atom literal expression and pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.4-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.4 remains narrower than the A0.5 successor profile.
    #[test]
    fn target_profile_keeps_raw_atoms_out_of_a0_4_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_4_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
            "src/profile_test_a0_4_raw_atoms.tl",
        );

        let a0_4 = target_profile_checks(&module, TargetProfile::A04Erlang);

        assert!(
            a0_4.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.4-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.4 Erlang profile should reject A0.5 raw atom literals: {:?}",
            a0_4
        );
    }

    /// Verifies the named A0.6 Erlang successor profile accepts tuple
    /// expressions and tuple case patterns over A0.5-compatible children.
    ///
    /// Inputs:
    /// - Source containing tuple construction and tuple pattern matching.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.6 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_tuples_for_a0_6_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_6_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
            "src/profile_test_a0_6_tuples.tl",
        );

        let a0_6 = target_profile_checks(&module, TargetProfile::A06Erlang);

        assert!(
            a0_6.is_empty(),
            "A0.6 Erlang profile should accept tuple expressions/patterns: {:?}",
            a0_6
        );
    }

    /// Verifies the named A0.5 Erlang successor profile does not silently widen
    /// to include A0.6 tuple expressions and patterns.
    ///
    /// Inputs:
    /// - Source containing tuple construction and tuple pattern matching.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.5-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.5 remains narrower than the A0.6 successor profile.
    #[test]
    fn target_profile_keeps_tuples_out_of_a0_5_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_5_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
            "src/profile_test_a0_5_tuples.tl",
        );

        let a0_5 = target_profile_checks(&module, TargetProfile::A05Erlang);

        assert!(
            a0_5.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.5-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.5 Erlang profile should reject A0.6 tuple forms: {:?}",
            a0_5
        );
    }

    /// Verifies the named A0.7 Erlang successor profile accepts list
    /// expressions and fixed-list case patterns over A0.6-compatible children.
    ///
    /// Inputs:
    /// - Source containing list construction and fixed-list pattern matching.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.7 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_lists_for_a0_7_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_7_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
            "src/profile_test_a0_7_lists.tl",
        );

        let a0_7 = target_profile_checks(&module, TargetProfile::A07Erlang);

        assert!(
            a0_7.is_empty(),
            "A0.7 Erlang profile should accept list expressions/patterns: {:?}",
            a0_7
        );
    }

    /// Verifies the named A0.6 Erlang successor profile does not silently widen
    /// to include A0.7 list expressions and patterns.
    ///
    /// Inputs:
    /// - Source containing list construction and fixed-list pattern matching.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.6-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.6 remains narrower than the A0.7 successor profile.
    #[test]
    fn target_profile_keeps_lists_out_of_a0_6_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_6_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
            "src/profile_test_a0_6_lists.tl",
        );

        let a0_6 = target_profile_checks(&module, TargetProfile::A06Erlang);

        assert!(
            a0_6.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.6-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.6 Erlang profile should reject A0.7 list forms: {:?}",
            a0_6
        );
    }

    /// Verifies the named A0.8 Erlang successor profile accepts binary/string
    /// literal expressions over the A0.7-compatible subset.
    ///
    /// Inputs:
    /// - Source containing a `Binary` return annotation and string literal
    ///   expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.8 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_binary_for_a0_8_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_8_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
            "src/profile_test_a0_8_binary.tl",
        );

        let a0_8 = target_profile_checks(&module, TargetProfile::A08Erlang);

        assert!(
            a0_8.is_empty(),
            "A0.8 Erlang profile should accept binary/string literals: {:?}",
            a0_8
        );
    }

    /// Verifies the named A0.7 Erlang successor profile does not silently widen
    /// to include A0.8 binary/string literal expressions.
    ///
    /// Inputs:
    /// - Source containing a string literal expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.7-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.7 remains narrower than the A0.8 successor profile.
    #[test]
    fn target_profile_keeps_binary_out_of_a0_7_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_7_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
            "src/profile_test_a0_7_binary.tl",
        );

        let a0_7 = target_profile_checks(&module, TargetProfile::A07Erlang);

        assert!(
            a0_7.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.7-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.7 Erlang profile should reject A0.8 binary/string literals: {:?}",
            a0_7
        );
    }

    /// Verifies the named A0.9 Erlang successor profile accepts expression-side
    /// list cons over the A0.8-compatible subset.
    ///
    /// Inputs:
    /// - Source containing `[head | tail]` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.9 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_list_cons_for_a0_9_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_9_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
            "src/profile_test_a0_9_list_cons.tl",
        );

        let a0_9 = target_profile_checks(&module, TargetProfile::A09Erlang);

        assert!(
            a0_9.is_empty(),
            "A0.9 Erlang profile should accept expression-side list cons: {:?}",
            a0_9
        );
    }

    /// Verifies the named A0.8 Erlang successor profile does not silently widen
    /// to include A0.9 list cons expressions.
    ///
    /// Inputs:
    /// - Source containing `[head | tail]` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.8-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.8 remains narrower than the A0.9 successor profile.
    #[test]
    fn target_profile_keeps_list_cons_out_of_a0_8_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_8_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
            "src/profile_test_a0_8_list_cons.tl",
        );

        let a0_8 = target_profile_checks(&module, TargetProfile::A08Erlang);

        assert!(
            a0_8.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.8-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.8 Erlang profile should reject A0.9 list cons expressions: {:?}",
            a0_8
        );
    }

    /// Verifies the named A0.10 Erlang successor profile accepts lowercase
    /// local named calls over the A0.9-compatible subset.
    ///
    /// Inputs:
    /// - Source containing a private lowercase local function and a public
    ///   function that calls it.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.10 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_named_call_for_a0_10_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_10_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
            "src/profile_test_a0_10_named_call.tl",
        );

        let a0_10 = target_profile_checks(&module, TargetProfile::A010Erlang);

        assert!(
            a0_10.is_empty(),
            "A0.10 Erlang profile should accept lowercase local named calls: {:?}",
            a0_10
        );
    }

    /// Verifies the named A0.9 Erlang successor profile does not silently widen
    /// to include A0.10 local named-call expressions.
    ///
    /// Inputs:
    /// - Source containing a lowercase local named call.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.9-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.9 remains narrower than the A0.10 successor profile.
    #[test]
    fn target_profile_keeps_named_call_out_of_a0_9_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_9_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
            "src/profile_test_a0_9_named_call.tl",
        );

        let a0_9 = target_profile_checks(&module, TargetProfile::A09Erlang);

        assert!(
            a0_9.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.9-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.9 Erlang profile should reject A0.10 local named calls: {:?}",
            a0_9
        );
    }

    /// Verifies the named A0.11 Erlang successor profile accepts unary negation
    /// over the A0.10-compatible subset.
    ///
    /// Inputs:
    /// - Source containing `-value` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.11 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_unary_neg_for_a0_11_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_11_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
            "src/profile_test_a0_11_unary_neg.tl",
        );

        let a0_11 = target_profile_checks(&module, TargetProfile::A011Erlang);

        assert!(
            a0_11.is_empty(),
            "A0.11 Erlang profile should accept unary negation: {:?}",
            a0_11
        );
    }

    /// Verifies the named A0.10 Erlang successor profile does not silently widen
    /// to include A0.11 unary negation expressions.
    ///
    /// Inputs:
    /// - Source containing `-value` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.10-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.10 remains narrower than the A0.11 successor profile.
    #[test]
    fn target_profile_keeps_unary_neg_out_of_a0_10_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_10_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
            "src/profile_test_a0_10_unary_neg.tl",
        );

        let a0_10 = target_profile_checks(&module, TargetProfile::A010Erlang);

        assert!(
            a0_10.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.10-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.10 Erlang profile should reject A0.11 unary negation: {:?}",
            a0_10
        );
    }

    /// Verifies the named A0.12 Erlang successor profile accepts resolved
    /// constructor calls over the A0.11-compatible subset.
    ///
    /// Inputs:
    /// - Source containing an explicit constructor declaration and a matching
    ///   constructor call expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.12 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_constructor_call_for_a0_12_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_12_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
            "src/profile_test_a0_12_constructor_call.tl",
        );

        let a0_12 = target_profile_checks(&module, TargetProfile::A012Erlang);

        assert!(
            a0_12.is_empty(),
            "A0.12 Erlang profile should accept resolved constructor calls: {:?}",
            a0_12
        );
    }

    /// Verifies the named A0.11 Erlang successor profile does not silently widen
    /// to include A0.12 constructor-call expressions.
    ///
    /// Inputs:
    /// - Source containing a resolved constructor call expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.11-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.11 remains narrower than the A0.12 successor profile.
    #[test]
    fn target_profile_keeps_constructor_call_out_of_a0_11_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_11_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
            "src/profile_test_a0_11_constructor_call.tl",
        );

        let a0_11 = target_profile_checks(&module, TargetProfile::A011Erlang);

        assert!(
            a0_11.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.11-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.11 Erlang profile should reject A0.12 constructor calls: {:?}",
            a0_11
        );
    }

    /// Verifies the named A0.13 Erlang successor profile accepts resolved
    /// constructor patterns over the A0.12-compatible subset.
    ///
    /// Inputs:
    /// - Source containing an explicit constructor declaration and matching
    ///   constructor pattern in a case expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.13 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_constructor_pattern_for_a0_13_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_13_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
            "src/profile_test_a0_13_constructor_pattern.tl",
        );

        let a0_13 = target_profile_checks(&module, TargetProfile::A013Erlang);

        assert!(
            a0_13.is_empty(),
            "A0.13 Erlang profile should accept resolved constructor patterns: {:?}",
            a0_13
        );
    }

    /// Verifies the named A0.12 Erlang successor profile does not silently widen
    /// to include A0.13 constructor-pattern forms.
    ///
    /// Inputs:
    /// - Source containing a resolved constructor pattern in a case expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.12-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.12 remains narrower than the A0.13 successor profile.
    #[test]
    fn target_profile_keeps_constructor_pattern_out_of_a0_12_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_12_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
            "src/profile_test_a0_12_constructor_pattern.tl",
        );

        let a0_12 = target_profile_checks(&module, TargetProfile::A012Erlang);

        assert!(
            a0_12.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.12-erlang`")
                    && (violation.message.contains("expression")
                        || violation.message.contains("pattern"))
            }),
            "A0.12 Erlang profile should reject A0.13 constructor patterns: {:?}",
            a0_12
        );
    }

    /// Verifies the named A0.14 Erlang successor profile accepts anonymous
    /// function values over the A0.13-compatible subset.
    ///
    /// Inputs:
    /// - Source containing `(x) -> x` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.14 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_lambda_for_a0_14_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_14_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
            "src/profile_test_a0_14_lambda.tl",
        );

        let a0_14 = target_profile_checks(&module, TargetProfile::A014Erlang);

        assert!(
            a0_14.is_empty(),
            "A0.14 Erlang profile should accept anonymous function values: {:?}",
            a0_14
        );
    }

    /// Verifies the named A0.13 Erlang successor profile does not silently widen
    /// to include A0.14 anonymous function values.
    ///
    /// Inputs:
    /// - Source containing `(x) -> x` as an expression body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.13-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.13 remains narrower than the A0.14 successor profile.
    #[test]
    fn target_profile_keeps_lambda_out_of_a0_13_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_13_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
            "src/profile_test_a0_13_lambda.tl",
        );

        let a0_13 = target_profile_checks(&module, TargetProfile::A013Erlang);

        assert!(
            a0_13.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.13-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.13 Erlang profile should reject A0.14 lambda expressions: {:?}",
            a0_13
        );
    }

    /// Verifies the named A0.15 Erlang successor profile accepts constructor
    /// extension expressions over the A0.14-compatible subset.
    ///
    /// Inputs:
    /// - Source containing `User(id, name) with Admin { ... }` as an expression
    ///   body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.15 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_constructor_extension_for_a0_15_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_15_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
            "src/profile_test_a0_15_constructor_extension.tl",
        );

        let a0_15 = target_profile_checks(&module, TargetProfile::A015Erlang);

        assert!(
            a0_15.is_empty(),
            "A0.15 Erlang profile should accept constructor extension: {:?}",
            a0_15
        );
    }

    /// Verifies the named A0.14 Erlang successor profile does not silently
    /// widen to include A0.15 constructor extension expressions.
    ///
    /// Inputs:
    /// - Source containing `User(id, name) with Admin { ... }` as an expression
    ///   body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.14-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.14 remains narrower than the A0.15 successor profile.
    #[test]
    fn target_profile_keeps_constructor_extension_out_of_a0_14_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_14_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
            "src/profile_test_a0_14_constructor_extension.tl",
        );

        let a0_14 = target_profile_checks(&module, TargetProfile::A014Erlang);

        assert!(
            a0_14.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.14-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0.14 Erlang profile should reject A0.15 constructor extension: {:?}",
            a0_14
        );
    }

    /// Verifies the named A0.16 Erlang successor profile accepts dedicated
    /// function-value invocation syntax.
    ///
    /// Inputs:
    /// - Source containing `f.(value)` in a function body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.16 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_fun_call_for_a0_16_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_16_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
            "src/profile_test_a0_16_fun_call.tl",
        );

        let a0_16 = target_profile_checks(&module, TargetProfile::A016Erlang);

        assert!(
            a0_16.is_empty(),
            "A0.16 Erlang profile should accept function-value invocation: {:?}",
            a0_16
        );
    }

    /// Verifies the named A0.15 Erlang successor profile does not silently
    /// widen to include A0.16 function-value invocation syntax.
    ///
    /// Inputs:
    /// - Source containing `f.(value)` in a function body.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.15-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.15 remains narrower than the A0.16 successor profile.
    #[test]
    fn target_profile_keeps_fun_call_out_of_a0_15_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_15_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
            "src/profile_test_a0_15_fun_call.tl",
        );

        let a0_15 = target_profile_checks(&module, TargetProfile::A015Erlang);

        assert!(
            a0_15.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.15-erlang`")
                    && violation.message.contains("expression kind")
            }),
            "A0.15 Erlang profile should reject A0.16 function-value invocation: {:?}",
            a0_15
        );
    }

    /// Verifies the named A0.17 Erlang successor profile accepts struct field
    /// access expressions.
    ///
    /// Inputs:
    /// - Source containing a public struct and `point.x` expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.17 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_field_access_for_a0_17_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_17_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
            "src/profile_test_a0_17_field_access.tl",
        );

        let a0_17 = target_profile_checks(&module, TargetProfile::A017Erlang);

        assert!(
            a0_17.is_empty(),
            "A0.17 Erlang profile should accept struct field access: {:?}",
            a0_17
        );
    }

    /// Verifies the named A0.16 Erlang successor profile does not silently
    /// widen to include A0.17 struct field access.
    ///
    /// Inputs:
    /// - Source containing a public struct and `point.x` expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.16-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.16 remains narrower than the A0.17 successor profile.
    #[test]
    fn target_profile_keeps_field_access_out_of_a0_16_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_16_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
            "src/profile_test_a0_16_field_access.tl",
        );

        let a0_16 = target_profile_checks(&module, TargetProfile::A016Erlang);

        assert!(
            a0_16.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.16-erlang`")
                    && violation.message.contains("FieldAccess")
            }),
            "A0.16 Erlang profile should reject A0.17 field access: {:?}",
            a0_16
        );
    }

    /// Verifies the named A0.18 Erlang successor profile accepts local let
    /// binding expressions.
    ///
    /// Inputs:
    /// - Source containing `let y = expr; z = expr; body`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.18 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_let_expr_for_a0_18_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_18_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
            "src/profile_test_a0_18_let_expr.tl",
        );

        let a0_18 = target_profile_checks(&module, TargetProfile::A018Erlang);

        assert!(
            a0_18.is_empty(),
            "A0.18 Erlang profile should accept local let expressions: {:?}",
            a0_18
        );
    }

    /// Verifies the named A0.17 Erlang successor profile does not silently
    /// widen to include A0.18 local let binding expressions.
    ///
    /// Inputs:
    /// - Source containing `let y = expr; z = expr; body`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.17-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.17 remains narrower than the A0.18 successor profile.
    #[test]
    fn target_profile_keeps_let_expr_out_of_a0_17_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_17_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
            "src/profile_test_a0_17_let_expr.tl",
        );

        let a0_17 = target_profile_checks(&module, TargetProfile::A017Erlang);

        assert!(
            a0_17.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.17-erlang`")
                    && violation.message.contains("Let")
            }),
            "A0.17 Erlang profile should reject A0.18 let expressions: {:?}",
            a0_17
        );
    }

    /// Verifies the named A0.19 Erlang successor profile accepts index-access
    /// expressions.
    ///
    /// Inputs:
    /// - Source containing `values[0]`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.19 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_index_access_for_a0_19_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_19_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
            "src/profile_test_a0_19_index_access.tl",
        );

        let a0_19 = target_profile_checks(&module, TargetProfile::A019Erlang);

        assert!(
            a0_19.is_empty(),
            "A0.19 Erlang profile should accept index access: {:?}",
            a0_19
        );
    }

    /// Verifies the named A0.18 Erlang successor profile does not silently
    /// widen to include A0.19 index-access expressions.
    ///
    /// Inputs:
    /// - Source containing `values[0]`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.18-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.18 remains narrower than the A0.19 successor profile.
    #[test]
    fn target_profile_keeps_index_access_out_of_a0_18_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_18_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
            "src/profile_test_a0_18_index_access.tl",
        );

        let a0_18 = target_profile_checks(&module, TargetProfile::A018Erlang);

        assert!(
            a0_18.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.18-erlang`")
                    && violation.message.contains("Index")
            }),
            "A0.18 Erlang profile should reject A0.19 index access: {:?}",
            a0_18
        );
    }

    /// Verifies the named A0.20 Erlang successor profile accepts qualified and
    /// scoped call expressions.
    ///
    /// Inputs:
    /// - Source containing `std.core.Math.add(...)` and `User.default()`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no violations.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   the named A0.20 profile without mutating compiler artifacts.
    #[test]
    fn target_profile_accepts_qualified_calls_for_a0_20_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_20_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
            "src/profile_test_a0_20_qualified_calls.tl",
        );

        let a0_20 = target_profile_checks(&module, TargetProfile::A020Erlang);

        assert!(
            a0_20.is_empty(),
            "A0.20 Erlang profile should accept qualified/scoped calls: {:?}",
            a0_20
        );
    }

    /// Verifies the named A0.19 Erlang successor profile does not silently
    /// widen to include A0.20 qualified and scoped call expressions.
    ///
    /// Inputs:
    /// - Source containing `std.core.Math.add(...)` and `User.default()`.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0.19-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that A0.19 remains narrower than the A0.20 successor profile.
    #[test]
    fn target_profile_keeps_qualified_calls_out_of_a0_19_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_19_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
            "src/profile_test_a0_19_qualified_calls.tl",
        );

        let a0_19 = target_profile_checks(&module, TargetProfile::A019Erlang);

        assert!(
            a0_19.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0.19-erlang`")
                    && violation.message.contains("RemoteCall")
            }),
            "A0.19 Erlang profile should reject A0.20 qualified/scoped calls: {:?}",
            a0_19
        );
    }

    /// Verifies the named A0.21 Erlang diagnostic profile rejects
    /// backend-specific remote function references.
    ///
    /// Inputs:
    /// - Source containing backend-specific `fun module:function/arity` syntax.
    ///
    /// Output:
    /// - Test passes when parsing rejects the backend-specific source form
    ///   before target-profile validation.
    ///
    /// Transformation:
    /// - Parses through the formal syntax-output path and confirms remote
    ///   function references are no longer canonical Terlan source.
    #[test]
    fn target_profile_rejects_remote_fun_ref_for_a0_21_erlang_profile() {
        let parsed = parse_module_as_syntax_output(
            "\
module profile_test_a0_21_remote_fun_ref.\n\npub reference(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

        assert!(
            parsed.is_err(),
            "remote fun references are backend output syntax, not canonical Terlan source"
        );
    }

    /// Verifies the frozen A0 Erlang target profile does not accept A0.1
    /// successor arithmetic forms.
    ///
    /// Inputs:
    /// - Source containing subtraction in an otherwise A0-shaped function.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unsupported` for `a0-erlang`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   that the frozen A0 profile remains narrower than the successor
    ///   profile.
    #[test]
    fn target_profile_keeps_subtraction_out_of_a0_erlang_profile() {
        let module = lower(
            "\
module profile_test_a0_subtraction.\n\npub subtract(x: Int): Int ->\n    x - 1.\n",
            "src/profile_test_a0_subtraction.tl",
        );

        let a0 = target_profile_checks(&module, TargetProfile::A0Erlang);

        assert!(
            a0.iter().any(|violation| {
                violation.code == "target_profile_unsupported"
                    && violation.message.contains("target `a0-erlang`")
                    && violation.message.contains("expression")
            }),
            "A0 Erlang profile should reject successor subtraction: {:?}",
            a0
        );
    }

    /// Verifies CoreV0 rejects float literals.
    ///
    /// Inputs:
    /// - Source containing a typed float literal expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_float_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_float_core_v0.\n\npub f(): Int ->\n    1.0.\n",
            "src/profile_test_float_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Float")),
            "CoreV0 profile should reject float core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_accepts_binary_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_binary.\n\npub f(): Binary ->\n    \"hello\".\n",
            "src/profile_test_binary.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should accept typed binary literal core terms"
        );
    }

    #[test]
    fn target_profile_allows_lambda_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_lambda.\n\npub f(): Dynamic ->\n    (x) -> x.\n",
            "src/profile_test_lambda.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow lambda-shaped core terms"
        );
    }

    #[test]
    fn target_profile_allows_map_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_map_expr.\n\npub f(): Map ->\n    #{a := 1, b => 2}.\n",
            "src/profile_test_map_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed map-expression core terms"
        );
    }

    #[test]
    fn target_profile_allows_list_cons_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_list_cons_expr.\n\npub f(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
            "src/profile_test_list_cons_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed list-cons expression core terms"
        );
    }

    #[test]
    fn target_profile_allows_index_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_index_expr.\n\npub f(values: List[Int]): Dynamic ->\n    values[0].\n",
            "src/profile_test_index_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed index-expression core terms"
        );
    }

    /// Verifies CoreV0 rejects index expressions.
    ///
    /// Inputs:
    /// - Source containing a typed index expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_index_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_index_expr_core_v0.\n\npub f(values: List[Int]): Dynamic ->\n    values[0].\n",
            "src/profile_test_index_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Index")),
            "CoreV0 profile should reject index core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_fixed_array_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_fixed_array_expr.\n\npub f(): FixedArray[3, Int] ->\n    #[1, 2, 3].\n",
            "src/profile_test_fixed_array_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed fixed-array core terms"
        );
    }

    /// Verifies CoreV0 rejects fixed-array literals.
    ///
    /// Inputs:
    /// - Source containing a typed fixed-array literal expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_fixed_array_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_fixed_array_expr_core_v0.\n\npub f(): FixedArray[3, Int] ->\n    #[1, 2, 3].\n",
            "src/profile_test_fixed_array_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("FixedArray")),
            "CoreV0 profile should reject fixed-array core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_list_comprehension_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_list_comprehension_expr.\n\npub f(values: List[Int]): List[Int] ->\n    [value | value <- values].\n",
            "src/profile_test_list_comprehension_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed list-comprehension core terms"
        );
    }

    /// Verifies CoreV0 rejects list-comprehension expressions.
    ///
    /// Inputs:
    /// - Source containing a typed list-comprehension expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_list_comprehension_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_list_comprehension_expr_core_v0.\n\npub f(values: List[Int]): List[Int] ->\n    [value | value <- values].\n",
            "src/profile_test_list_comprehension_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("ListComprehension")),
            "CoreV0 profile should reject list-comprehension core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_record_construct_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_record_construct_expr.\n\npub f(): Dynamic ->\n    #Point { x = 1 }.\n",
            "src/profile_test_record_construct_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed record-construction core terms"
        );
    }

    /// Verifies CoreV0 rejects record construction expressions.
    ///
    /// Inputs:
    /// - Source containing a typed record construction expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_record_construct_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_record_construct_expr_core_v0.\n\npub f(): Dynamic ->\n    #Point { x = 1 }.\n",
            "src/profile_test_record_construct_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("RecordConstruct")),
            "CoreV0 profile should reject record-construction core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_field_access_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_field_access_expr.\n\npub f(point: Point): Dynamic ->\n    point.x.\n",
            "src/profile_test_field_access_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed field-access core terms"
        );
    }

    #[test]
    fn target_profile_allows_record_access_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_record_access_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
            "src/profile_test_record_access_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed record-access core terms"
        );
    }

    /// Verifies CoreV0 rejects record access expressions.
    ///
    /// Inputs:
    /// - Source containing a typed record access expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_record_access_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_record_access_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point.x.\n",
            "src/profile_test_record_access_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("RecordAccess")),
            "CoreV0 profile should reject record-access core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_record_update_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_record_update_expr.\n\npub f(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
            "src/profile_test_record_update_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed record-update core terms"
        );
    }

    /// Verifies CoreV0 rejects record update expressions.
    ///
    /// Inputs:
    /// - Source containing a typed record update expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_record_update_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_record_update_expr_core_v0.\n\npub f(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
            "src/profile_test_record_update_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("RecordUpdate")),
            "CoreV0 profile should reject record-update core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_template_instantiate_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_template_instantiate_expr.\n\npub f(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
            "src/profile_test_template_instantiate_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed template-instantiation core terms"
        );
    }

    /// Verifies CoreV0 rejects template instantiation expressions.
    ///
    /// Inputs:
    /// - Source containing a typed template instantiation expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_template_instantiate_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_template_instantiate_expr_core_v0.\n\npub f(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
            "src/profile_test_template_instantiate_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("TemplateInstantiate")),
            "CoreV0 profile should reject template-instantiation core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_constructor_chain_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_constructor_chain_expr.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
            "src/profile_test_constructor_chain_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed constructor-chain core terms"
        );
    }

    /// Verifies CoreV0 rejects partial constructor-chain expressions.
    ///
    /// Inputs:
    /// - Source containing a declared constructor-chain expression whose base
    ///   constructor identity resolves but whose proof coverage remains partial.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_constructor_chain_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_constructor_chain_expr_core_v0.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub f(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
            "src/profile_test_constructor_chain_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("constructor chain")),
            "CoreV0 profile should reject partial constructor-chain core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_resolved_constructor_call_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_constructor_call_candidate.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n\npub f(value: Int): Dynamic ->\n    Ok(value).\n",
            "src/profile_test_constructor_call_candidate.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow resolved constructor-call core terms"
        );
    }

    /// Verifies unresolved constructor-call metadata blocks backend validation.
    ///
    /// Inputs:
    /// - A directly constructed Lean-covered Core module whose metadata reports
    ///   one unresolved constructor-call candidate.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unresolved_constructor`.
    ///
    /// Transformation:
    /// - Uses the unresolved-constructor fixture helper to isolate the call
    ///   metadata counter from parser and typechecker diagnostics.
    #[test]
    fn target_profile_rejects_unresolved_constructor_call_candidate() {
        let module = module_with_unresolved_constructor_candidates(1, 0, 0);

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert_unresolved_constructor_violation(&erlang, 1, 0, 0);
    }

    /// Verifies unresolved constructor-pattern metadata blocks backend validation.
    ///
    /// Inputs:
    /// - A directly constructed Lean-covered Core module whose metadata reports
    ///   one unresolved constructor-pattern candidate.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unresolved_constructor`.
    ///
    /// Transformation:
    /// - Uses the unresolved-constructor fixture helper to isolate the pattern
    ///   metadata counter from parser and typechecker diagnostics.
    #[test]
    fn target_profile_rejects_unresolved_constructor_pattern_candidate() {
        let module = module_with_unresolved_constructor_candidates(0, 0, 1);

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert_unresolved_constructor_violation(&erlang, 0, 0, 1);
    }

    /// Verifies unresolved constructor-chain metadata blocks backend validation.
    ///
    /// Inputs:
    /// - A directly constructed Lean-covered Core module whose metadata reports
    ///   one unresolved constructor-chain candidate.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports
    ///   `target_profile_unresolved_constructor`.
    ///
    /// Transformation:
    /// - Uses the unresolved-constructor fixture helper to isolate the chain
    ///   metadata counter from parser and typechecker diagnostics.
    #[test]
    fn target_profile_rejects_unresolved_constructor_chain_candidate() {
        let module = module_with_unresolved_constructor_candidates(0, 1, 0);

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert_unresolved_constructor_violation(&erlang, 0, 1, 0);
    }

    #[test]
    fn target_profile_rejects_remote_fun_ref_source_syntax_before_profile_validation() {
        let parsed = parse_module_as_syntax_output(
            "\
module profile_test_remote_fun_ref_expr.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

        assert!(
            parsed.is_err(),
            "remote fun references are backend output syntax, not canonical Terlan source"
        );
    }

    /// Verifies CoreV0 rejects remote function references.
    ///
    /// Inputs:
    /// - Source containing a typed remote function reference expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_remote_fun_ref_expr_for_core_v0_profile() {
        let parsed = parse_module_as_syntax_output(
            "\
module profile_test_remote_fun_ref_expr_core_v0.\n\npub f(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

        assert!(
            parsed.is_err(),
            "remote fun references are backend output syntax, not canonical Terlan source"
        );
    }

    #[test]
    fn target_profile_allows_remote_call_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_remote_call_expr.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
            "src/profile_test_remote_call_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed remote-call core terms"
        );
    }

    /// Verifies CoreV0 rejects proof-model-required remote calls.
    ///
    /// Inputs:
    /// - Source containing a typed remote-call expression.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_remote_call_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_remote_call_expr_core_v0.\n\npub f(): Int ->\n    erlang.Math.abs(1).\n",
            "src/profile_test_remote_call_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("remote call")),
            "CoreV0 profile should reject proof-model-required remote-call core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_if_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_if_expr.\n\npub f(flag: Bool): Int ->\n    if { flag -> 1; true -> 0 }.\n",
            "src/profile_test_if_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed if-expression core terms"
        );
    }

    #[test]
    fn target_profile_allows_receive_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_receive_expr.\n\npub f(): Dynamic ->\n    receive {\n        value -> value;\n    after 0 -> :timeout\n    }.\n",
            "src/profile_test_receive_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed receive-expression core terms"
        );
    }

    /// Verifies CoreV0 rejects receive expressions.
    ///
    /// Inputs:
    /// - Source containing a typed receive expression with a timeout branch.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_receive_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_receive_expr_core_v0.\n\npub f(): Dynamic ->\n    receive {\n        value -> value;\n    after 0 -> :timeout\n    }.\n",
            "src/profile_test_receive_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Receive")),
            "CoreV0 profile should reject receive core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_try_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_try_expr.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
            "src/profile_test_try_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed try-expression core terms"
        );
    }

    /// Verifies CoreV0 rejects try expressions.
    ///
    /// Inputs:
    /// - Source containing a typed try expression with `of`, `catch`, and
    ///   `after` branches.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the expression as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_try_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_try_expr_core_v0.\n\npub f(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
            "src/profile_test_try_expr_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Try")),
            "CoreV0 profile should reject try core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_unary_op_expr_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_unary_op_expr.\n\npub f(value: Int): Int ->\n    -value.\n",
            "src/profile_test_unary_op_expr.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed unary-op core terms"
        );
    }

    #[test]
    fn target_profile_allows_map_pattern_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_map_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #{a = x} -> x;\n        _ -> value\n    }.\n",
            "src/profile_test_map_pattern.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed map-pattern core terms"
        );
    }

    /// Verifies Erlang accepts float patterns.
    ///
    /// Inputs:
    /// - Source containing a typed case expression with a float pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports no Erlang-profile
    ///   violations for the lowered module.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   permissive Erlang-profile validation without mutating compiler
    ///   artifacts.
    #[test]
    fn target_profile_allows_float_pattern_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_float_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
            "src/profile_test_float_pattern.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed float-pattern core terms"
        );
    }

    /// Verifies CoreV0 rejects float patterns.
    ///
    /// Inputs:
    /// - Source containing a typed case expression with a float pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the pattern as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_float_pattern_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_float_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
            "src/profile_test_float_pattern_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Float")),
            "CoreV0 profile should reject float-pattern core terms: {:?}",
            core_v0
        );
    }

    /// Verifies CoreV0 rejects map patterns.
    ///
    /// Inputs:
    /// - Source containing a typed case expression with a map pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the pattern as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_map_pattern_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_map_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #{a = x} -> x;\n        _ -> value\n    }.\n",
            "src/profile_test_map_pattern_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Map")),
            "CoreV0 profile should reject map-pattern core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_list_cons_pattern_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_list_cons_pattern.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
            "src/profile_test_list_cons_pattern.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed list-cons pattern core terms"
        );
    }

    /// Verifies CoreV0 rejects list-cons patterns.
    ///
    /// Inputs:
    /// - Source containing a typed case expression with a list-cons pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the pattern as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_list_cons_pattern_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_list_cons_pattern_core_v0.\n\npub f(value: List[Int]): Dynamic ->\n    case value {\n        [head | tail] -> head;\n        _ -> value\n    }.\n",
            "src/profile_test_list_cons_pattern_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("ListCons")),
            "CoreV0 profile should reject list-cons pattern core terms: {:?}",
            core_v0
        );
    }

    #[test]
    fn target_profile_allows_record_pattern_for_erlang_profile() {
        let module = lower(
            "\
module profile_test_record_pattern.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #Point { x = x } -> x;\n        _ -> value\n    }.\n",
            "src/profile_test_record_pattern.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);

        assert!(
            erlang.is_empty(),
            "Erlang profile should allow typed record-pattern core terms"
        );
    }

    /// Verifies CoreV0 rejects record patterns.
    ///
    /// Inputs:
    /// - Source containing a typed case expression with a record pattern.
    ///
    /// Output:
    /// - Test passes when target-profile validation reports the pattern as
    ///   unsupported for `core-v0`.
    ///
    /// Transformation:
    /// - Lowers source through the formal syntax-output/CoreIR path and checks
    ///   target-subset validation without mutating compiler artifacts.
    #[test]
    fn target_profile_rejects_record_pattern_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_record_pattern_core_v0.\n\npub f(value: Dynamic): Dynamic ->\n    case value {\n        #Point { x = x } -> x;\n        _ -> value\n    }.\n",
            "src/profile_test_record_pattern_core_v0.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"
                    && violation.message.contains("Record")),
            "CoreV0 profile should reject record-pattern core terms: {:?}",
            core_v0
        );
    }

    /// Verifies the portable CoreIR v0 profile accepts a Lean-covered arithmetic
    /// expression.
    ///
    /// Inputs:
    /// - A source module whose function body lowers to typed `BinaryOp(-)`.
    ///
    /// Output:
    /// - Test assertion only; no compiler artifacts are written.
    ///
    /// Transformation:
    /// - Lowers source through syntax output to CoreIR, then validates it under
    ///   the `core-v0` target profile.
    #[test]
    fn target_profile_accepts_subtraction_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_core_v0_sub.\n\npub f(x: Int, y: Int): Int ->\n    x - y.\n",
            "src/profile_test_core_v0_sub.tl",
        );

        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            core_v0.is_empty(),
            "Core v0 profile should accept Lean-covered subtraction: {:?}",
            core_v0
        );
    }

    /// Verifies the portable CoreIR v0 profile rejects a broad backend-specific
    /// expression form while the Erlang profile remains permissive.
    ///
    /// Inputs:
    /// - A source module whose function body lowers to typed map CoreIR.
    ///
    /// Output:
    /// - Test assertion only; no compiler artifacts are written.
    ///
    /// Transformation:
    /// - Lowers source through syntax output to CoreIR, checks that Erlang still
    ///   accepts the shape, then checks that `core-v0` reports unsupported
    ///   expression coverage or shape.
    #[test]
    fn target_profile_rejects_map_expr_for_core_v0_profile() {
        let module = lower(
            "\
module profile_test_core_v0_map.\n\npub f(): Map ->\n    #{a := 1}.\n",
            "src/profile_test_core_v0_map.tl",
        );

        let erlang = target_profile_checks(&module, TargetProfile::Erlang);
        let core_v0 = target_profile_checks(&module, TargetProfile::CoreV0);

        assert!(
            erlang.is_empty(),
            "Erlang profile should remain permissive for map core terms"
        );
        assert!(
            core_v0
                .iter()
                .any(|violation| violation.code == "target_profile_unsupported"),
            "Core v0 profile should reject map core terms: {:?}",
            core_v0
        );
    }
}
