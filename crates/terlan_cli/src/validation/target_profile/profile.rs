use terlan_typeck::{CoreExpr, CorePattern, CoreProofCoverage};

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
    /// Shared JavaScript module profile with no browser-only ambient access.
    JsShared,
    /// Browser JavaScript profile for explicit browser and DOM bindings.
    JsBrowser,
    /// Worker JavaScript profile for explicit worker-safe bindings.
    JsWorker,
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
            Self::JsShared => "js.shared",
            Self::JsBrowser => "js.browser",
            Self::JsWorker => "js.worker",
            Self::CoreV0 => "core-v0",
        }
    }

    /// Returns whether this profile targets JavaScript emission.
    ///
    /// Inputs:
    /// - One profile variant.
    ///
    /// Output:
    /// - `true` for JavaScript target profiles.
    ///
    /// Transformation:
    /// - Groups the initial JS profile family behind one predicate so import
    ///   validation can gate `std.js.*` without duplicating enum matches.
    pub(crate) const fn is_js(&self) -> bool {
        matches!(self, Self::JsShared | Self::JsBrowser | Self::JsWorker)
    }

    /// Returns whether profile allows a given expression-level proof coverage.
    ///
    /// Inputs:
    /// - `coverage`: expression proof coverage produced during CoreIR lowering.
    ///
    /// Output:
    /// - `true` when this profile accepts that proof class.
    ///
    /// Transformation:
    /// - Delegates to the shared coverage gate so expression and pattern proof
    ///   policy cannot drift.
    pub(super) const fn allows_expr_coverage(&self, coverage: CoreProofCoverage) -> bool {
        self.allows_core_proof_coverage(coverage)
    }

    /// Returns whether profile allows a given pattern-level proof coverage.
    ///
    /// Inputs:
    /// - `coverage`: pattern proof coverage produced during CoreIR lowering.
    ///
    /// Output:
    /// - `true` when this profile accepts that proof class.
    ///
    /// Transformation:
    /// - Delegates to the shared coverage gate so expression and pattern proof
    ///   policy cannot drift.
    pub(super) const fn allows_pattern_coverage(&self, coverage: CoreProofCoverage) -> bool {
        self.allows_core_proof_coverage(coverage)
    }

    /// Returns whether a proof coverage class is accepted by this profile.
    ///
    /// Inputs:
    /// - `coverage`: proof coverage produced during CoreIR lowering.
    ///
    /// Output:
    /// - `true` for all profiles except CoreV0 partial/non-Lean coverage.
    ///
    /// Transformation:
    /// - Encodes the common expression/pattern proof-coverage rule in one
    ///   place, with CoreV0 as the only current profile requiring Lean coverage.
    const fn allows_core_proof_coverage(&self, coverage: CoreProofCoverage) -> bool {
        !matches!(self, Self::CoreV0) || matches!(coverage, CoreProofCoverage::LeanCovered)
    }

    /// Returns whether pattern summaries with no typed payload are acceptable.
    ///
    /// Inputs:
    /// - One target profile.
    ///
    /// Output:
    /// - `true` for supported Erlang forms.
    pub(super) const fn allows_uncovered_pattern(&self) -> bool {
        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => true,
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
    pub(super) const fn allows_runtime_boundary(&self) -> bool {
        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => true,
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
    pub(super) const fn requires_checked_preservation_evidence(&self) -> bool {
        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => false,
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
    pub(super) fn allows_pattern_shape(&self, pattern: &CorePattern) -> bool {
        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => true,
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
    pub(super) fn allows_expr_shape(&self, expr: &CoreExpr) -> bool {
        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => true,
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
                CoreExpr::Cast { expr, .. } => self.allows_expr_shape(expr),
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
