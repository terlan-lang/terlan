//! Shared expression and pattern proof-coverage policy.

use super::TargetProfile;
use crate::terlan_typeck::CoreProofCoverage;

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
}
