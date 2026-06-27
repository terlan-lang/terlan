use crate::terlan_typeck::CorePattern;

use super::TargetProfile;

impl TargetProfile {
    /// Returns whether a typed pattern constructor is structurally acceptable for
    /// the profile.
    ///
    /// Inputs:
    /// - `expr`: typed core pattern being considered.
    ///
    /// Output:
    /// - `true` when all current backend profiles accept the node.
    pub(in crate::validation::target_profile) fn allows_pattern_shape(
        &self,
        pattern: &CorePattern,
    ) -> bool {
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
}
