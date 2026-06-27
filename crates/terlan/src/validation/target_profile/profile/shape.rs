use crate::terlan_typeck::{CoreExpr, CoreProofCoverage};

use super::TargetProfile;

impl TargetProfile {
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
    pub(in crate::validation::target_profile) const fn allows_expr_coverage(
        &self,
        coverage: CoreProofCoverage,
    ) -> bool {
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
    pub(in crate::validation::target_profile) const fn allows_pattern_coverage(
        &self,
        coverage: CoreProofCoverage,
    ) -> bool {
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
    pub(in crate::validation::target_profile) const fn allows_uncovered_pattern(&self) -> bool {
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
    pub(in crate::validation::target_profile) const fn allows_runtime_boundary(&self) -> bool {
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
    pub(in crate::validation::target_profile) const fn requires_checked_preservation_evidence(
        &self,
    ) -> bool {
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

    /// Returns whether a typed expression form is structurally acceptable for the
    /// profile.
    ///
    /// Inputs:
    /// - `expr`: typed core expression node.
    ///
    /// Output:
    /// - `true` when this profile accepts the core expression family.
    pub(in crate::validation::target_profile) fn allows_expr_shape(&self, expr: &CoreExpr) -> bool {
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
                | CoreExpr::SqlQuery { .. }
                | CoreExpr::Try { .. } => false,
            },
        }
    }
}
