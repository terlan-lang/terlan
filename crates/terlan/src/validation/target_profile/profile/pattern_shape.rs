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
            Self::Vm => true,
            Self::JsShared | Self::JsBrowser | Self::JsWorker => {
                !matches!(pattern, CorePattern::BinaryLayout { .. })
            }
            Self::WasmCore => matches!(pattern, CorePattern::Var(_)),
            Self::A0Vm | Self::A01Vm | Self::A02Vm | Self::A03Vm => {
                matches!(pattern, CorePattern::Var(_))
            }
            Self::A04Vm => matches!(pattern, CorePattern::Var(_) | CorePattern::Int(_)),
            Self::A05Vm => matches!(
                pattern,
                CorePattern::Wildcard
                    | CorePattern::Var(_)
                    | CorePattern::Int(_)
                    | CorePattern::Atom(_)
            ),
            Self::A06Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A07Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A08Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A09Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A010Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A011Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A012Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                _ => false,
            },
            Self::A013Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                CorePattern::Alias { pattern, .. } => self.allows_pattern_shape(pattern),
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
            Self::A014Vm
            | Self::A015Vm
            | Self::A016Vm
            | Self::A017Vm
            | Self::A018Vm
            | Self::A019Vm
            | Self::A020Vm
            | Self::A021Vm => match pattern {
                CorePattern::Wildcard
                | CorePattern::Var(_)
                | CorePattern::Int(_)
                | CorePattern::Atom(_) => true,
                CorePattern::Tuple(values) | CorePattern::List(values) => {
                    values.iter().all(|value| self.allows_pattern_shape(value))
                }
                CorePattern::Alias { pattern, .. } => self.allows_pattern_shape(pattern),
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
                CorePattern::Alias { pattern, .. } => self.allows_pattern_shape(pattern),
                CorePattern::Constructor {
                    constructor_identity,
                    args,
                    ..
                } => {
                    constructor_identity.is_some()
                        && args.iter().all(|arg| self.allows_pattern_shape(arg))
                }
                CorePattern::Float(_)
                | CorePattern::String(_)
                | CorePattern::StringPattern(_)
                | CorePattern::ListCons { .. }
                | CorePattern::Map(_)
                | CorePattern::Record { .. }
                | CorePattern::BinaryLayout { .. } => false,
            },
        }
    }
}
