use super::TargetProfile;
use terlan_typeck::CoreExprSummary;

/// Extends target profiles with source-summary shape gates.
///
/// Inputs:
/// - Implemented for `TargetProfile`.
///
/// Output:
/// - Boolean decisions used by CoreIR summary traversal.
///
/// Transformation:
/// - Keeps source-summary policy separate from recursive CoreIR walking so the
///   traversal module can focus on visiting nodes and collecting violations.
pub(super) trait ProfileExprShapeExtensions {
    /// Returns whether a source-summary expression kind is admitted.
    ///
    /// Inputs: expression summary from typed lowering. Output: `true` when the
    /// profile allows the source expression family. Transformation: delegates
    /// profile-specific source syntax gates to the implementation.
    fn allows_expr_summary_kind(self, summary: &CoreExprSummary) -> bool;

    /// Returns whether a typed payload is required for this summary.
    ///
    /// Inputs: expression summary from typed lowering. Output: `true` when the
    /// summary has enough typed payload for the profile. Transformation:
    /// applies profile-specific payload strictness.
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
                | Self::JsShared
                | Self::JsBrowser
                | Self::JsWorker
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

        if summary.remote.is_some() && self.allows_runtime_boundary() {
            return true;
        }

        if summary.kind == "Call" && matches!(self, Self::A020Erlang | Self::A021Erlang) {
            return true;
        }

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
            | Self::A021Erlang => summary.core_expr.is_some(),
            Self::CoreV0 => summary.core_expr.is_some(),
        }
    }
}
