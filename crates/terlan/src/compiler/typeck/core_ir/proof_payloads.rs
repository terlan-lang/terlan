#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Proof coverage classification for CoreIR expressions.
///
/// Inputs:
/// - CoreIR expression or module proof status.
///
/// Outputs:
/// - Coarse proof coverage label used by release and conformance checks.
///
/// Transformation:
/// - Records whether the current proof model covers, partially covers, or
///   defers the compiler construct.
pub enum CoreProofCoverage {
    LeanCovered,
    Partial,
    ProofModelRequired,
    RuntimeBoundary,
    ArtifactOnly,
}

impl CoreProofCoverage {
    /// Renders the proof-coverage label used in deterministic CoreIR artifacts.
    ///
    /// Inputs:
    /// - `self`: proof coverage classification for a Core expression summary.
    ///
    /// Output:
    /// - Stable lowercase label suitable for CoreIR contract text and
    ///   conformance fixtures.
    ///
    /// Transformation:
    /// - Maps internal enum variants to the documented LP7 coverage labels.
    pub(super) fn as_str(&self) -> &'static str {
        match self {
            CoreProofCoverage::LeanCovered => "lean-covered",
            CoreProofCoverage::Partial => "partial",
            CoreProofCoverage::ProofModelRequired => "proof-model-required",
            CoreProofCoverage::RuntimeBoundary => "runtime-boundary",
            CoreProofCoverage::ArtifactOnly => "artifact-only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Module-level proof readiness classification.
///
/// Inputs:
/// - Aggregated proof coverage for a CoreIR module.
///
/// Outputs:
/// - Stable readiness label for manifests and contract text.
///
/// Transformation:
/// - Summarizes expression-level proof coverage into a module-level status.
pub enum CoreProofReadiness {
    LeanCovered,
    Partial,
    ProofModelRequired,
    RuntimeBoundary,
    ArtifactOnly,
    NoExpressions,
}

impl CoreProofReadiness {
    /// Renders the module-level proof readiness label.
    ///
    /// Inputs:
    /// - `self`: proof readiness classification for a Core module.
    ///
    /// Output:
    /// - Stable lowercase label suitable for manifests and CoreIR contract text.
    ///
    /// Transformation:
    /// - Maps the internal readiness enum to the documented proof-readiness
    ///   labels used by release tooling.
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreProofReadiness::LeanCovered => "lean-covered",
            CoreProofReadiness::Partial => "partial",
            CoreProofReadiness::ProofModelRequired => "proof-model-required",
            CoreProofReadiness::RuntimeBoundary => "runtime-boundary",
            CoreProofReadiness::ArtifactOnly => "artifact-only",
            CoreProofReadiness::NoExpressions => "no-expressions",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Kind of checked-preservation evidence attached to CoreIR payloads.
///
/// Inputs:
/// - Core expression or pattern being summarized.
///
/// Outputs:
/// - Evidence category identifying what structural property was checked.
///
/// Transformation:
/// - Separates expression and pattern evidence for future proof export.
pub enum CoreCheckedPreservationEvidenceKind {
    StructuralCoreExpr,
    StructuralCorePattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Substitution-freshness obligation carried by proof evidence.
///
/// Inputs:
/// - CoreIR structure being summarized for proof checks.
///
/// Outputs:
/// - Conservative freshness requirement for later proof tooling.
///
/// Transformation:
/// - Classifies whether runtime bindings are absent or require explicit
///   freshness evidence.
pub enum CoreSubstitutionFreshnessEvidence {
    NoRuntimeBindings,
    RuntimeBindingsRequired,
}

impl CoreSubstitutionFreshnessEvidence {
    /// Renders the substitution-freshness obligation attached to evidence.
    ///
    /// Inputs:
    /// - `self`: conservative freshness classification for one evidence-backed
    ///   Core expression or pattern.
    ///
    /// Output:
    /// - Stable lowercase label suitable for CoreIR contract text and future
    ///   Lean export.
    ///
    /// Transformation:
    /// - Maps internal freshness categories to the LP8 handoff vocabulary:
    ///   values without runtime binding introduction need no freshness payload,
    ///   while binding forms require checked runtime freshness evidence later.
    fn as_str(&self) -> &'static str {
        match self {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings => "no-runtime-bindings",
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired => {
                "runtime-bindings-required"
            }
        }
    }

    /// Combines two substitution-freshness obligations conservatively.
    ///
    /// Inputs:
    /// - `self`: current aggregate obligation.
    /// - `other`: additional nested obligation.
    ///
    /// Output:
    /// - `RuntimeBindingsRequired` when either side may introduce runtime
    ///   bindings; otherwise `NoRuntimeBindings`.
    ///
    /// Transformation:
    /// - Applies a two-point lattice join over freshness obligations.
    pub(crate) fn combine(
        self,
        other: CoreSubstitutionFreshnessEvidence,
    ) -> CoreSubstitutionFreshnessEvidence {
        if matches!(
            self,
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        ) || matches!(
            other,
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        ) {
            CoreSubstitutionFreshnessEvidence::RuntimeBindingsRequired
        } else {
            CoreSubstitutionFreshnessEvidence::NoRuntimeBindings
        }
    }
}

impl CoreCheckedPreservationEvidenceKind {
    /// Renders the checked-preservation evidence kind used in CoreIR payloads.
    ///
    /// Inputs:
    /// - `self`: checked-preservation evidence classification for one typed
    ///   Core expression or pattern.
    ///
    /// Output:
    /// - Stable lowercase evidence label suitable for deterministic contract
    ///   text and future Lean export.
    ///
    /// Transformation:
    /// - Maps the internal evidence enum to a documented LP8 evidence label.
    fn as_str(&self) -> &'static str {
        match self {
            CoreCheckedPreservationEvidenceKind::StructuralCoreExpr => "structural-core-expr",
            CoreCheckedPreservationEvidenceKind::StructuralCorePattern => "structural-core-pattern",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Checked-preservation evidence for one CoreIR target.
///
/// Inputs:
/// - Evidence kind, freshness obligation, and target contract text.
///
/// Outputs:
/// - Deterministic proof evidence payload.
///
/// Transformation:
/// - Packages proof-support metadata without changing CoreIR lowering.
pub struct CoreCheckedPreservationEvidence {
    pub kind: CoreCheckedPreservationEvidenceKind,
    pub freshness: CoreSubstitutionFreshnessEvidence,
    pub target: String,
}

impl CoreCheckedPreservationEvidence {
    /// Renders checked-preservation evidence as deterministic contract text.
    ///
    /// Inputs:
    /// - `self`: evidence object attached to a typed Core expression summary.
    ///
    /// Output:
    /// - Stable compact text for CoreIR contracts and phase goldens.
    ///
    /// Transformation:
    /// - Serializes the evidence kind, substitution-freshness obligation, and
    ///   structural Core term it covers, avoiding source spans and
    ///   backend-specific syntax.
    pub(crate) fn contract_text(&self) -> String {
        format!(
            "{}(freshness={};target={})",
            self.kind.as_str(),
            self.freshness.as_str(),
            self.target
        )
    }
}
