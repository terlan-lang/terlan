use std::path::Path;
use std::process::ExitCode;

use terlan_syntax::{
    cached_canonical_terlan_syntax_contract_identity, syntax_contract_identity_matches_current,
    SyntaxContractIdentity,
};

/// Error message emitted when CoreIR constructor candidates remain unresolved
/// at phase-manifest validation time.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Stable validation error text shared by production checks and regression
///   tests.
///
/// Transformation:
/// - Centralizes the constructor-resolution manifest failure message without
///   allocating or reading manifest artifacts.
const PHASE_MANIFEST_UNRESOLVED_CONSTRUCTOR_ERROR: &str =
    "phase manifest constructor candidates must resolve before formal validation";

/// One compiler phase entry in a phase manifest.
#[derive(Clone, serde::Serialize)]
pub(crate) struct PhaseOutput {
    pub(crate) name: &'static str,
    pub(crate) status: &'static str,
    pub(crate) diagnostics: Vec<PhaseManifestDiagnostic>,
}

/// One diagnostic entry serialized into a phase manifest.
#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct PhaseManifestDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) severity: &'static str,
    pub(crate) message: String,
    pub(crate) path: String,
    pub(crate) span_start: usize,
    pub(crate) span_end: usize,
    pub(crate) notes: Vec<String>,
}

/// CoreIR proof-coverage counts serialized into a phase manifest.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct PhaseManifestCoreProofCoverage {
    pub(crate) readiness: String,
    pub(crate) lean_covered: usize,
    pub(crate) partial: usize,
    pub(crate) proof_model_required: usize,
    pub(crate) runtime_boundary: usize,
    pub(crate) artifact_only: usize,
    pub(crate) pattern_lean_covered: usize,
    pub(crate) pattern_partial: usize,
    pub(crate) pattern_proof_model_required: usize,
    pub(crate) pattern_runtime_boundary: usize,
    pub(crate) pattern_artifact_only: usize,
    pub(crate) typed_core_expr: usize,
    pub(crate) summary_only_expr: usize,
    pub(crate) typed_core_pattern: usize,
    pub(crate) summary_only_pattern: usize,
    pub(crate) typed_core_type: usize,
    pub(crate) summary_only_type: usize,
    pub(crate) checked_preservation_expr: usize,
    pub(crate) checked_preservation_pattern: usize,
    pub(crate) checked_preservation_expr_structural: usize,
    pub(crate) checked_preservation_pattern_structural: usize,
    pub(crate) checked_preservation_expr_no_runtime_bindings: usize,
    pub(crate) checked_preservation_pattern_no_runtime_bindings: usize,
    pub(crate) checked_preservation_expr_runtime_bindings_required: usize,
    pub(crate) checked_preservation_pattern_runtime_bindings_required: usize,
    pub(crate) resolved_constructor_call_identity: usize,
    pub(crate) resolved_constructor_chain_identity: usize,
    pub(crate) resolved_constructor_pattern_identity: usize,
    pub(crate) unresolved_constructor_call_candidate: usize,
    pub(crate) unresolved_constructor_chain_candidate: usize,
    pub(crate) unresolved_constructor_pattern_candidate: usize,
}

impl Default for PhaseManifestCoreProofCoverage {
    /// Creates zero CoreIR proof-coverage counts for skipped/error paths.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Proof coverage with `none` readiness and zero bucket counts.
    ///
    /// Transformation:
    /// - Constructs the manifest placeholder used when CoreIR was not produced.
    fn default() -> Self {
        Self {
            readiness: "none".to_string(),
            lean_covered: 0,
            partial: 0,
            proof_model_required: 0,
            runtime_boundary: 0,
            artifact_only: 0,
            pattern_lean_covered: 0,
            pattern_partial: 0,
            pattern_proof_model_required: 0,
            pattern_runtime_boundary: 0,
            pattern_artifact_only: 0,
            typed_core_expr: 0,
            summary_only_expr: 0,
            typed_core_pattern: 0,
            summary_only_pattern: 0,
            typed_core_type: 0,
            summary_only_type: 0,
            checked_preservation_expr: 0,
            checked_preservation_pattern: 0,
            checked_preservation_expr_structural: 0,
            checked_preservation_pattern_structural: 0,
            checked_preservation_expr_no_runtime_bindings: 0,
            checked_preservation_pattern_no_runtime_bindings: 0,
            checked_preservation_expr_runtime_bindings_required: 0,
            checked_preservation_pattern_runtime_bindings_required: 0,
            resolved_constructor_call_identity: 0,
            resolved_constructor_chain_identity: 0,
            resolved_constructor_pattern_identity: 0,
            unresolved_constructor_call_candidate: 0,
            unresolved_constructor_chain_candidate: 0,
            unresolved_constructor_pattern_candidate: 0,
        }
    }
}

impl PhaseManifestCoreProofCoverage {
    /// Converts CoreIR metadata into phase-manifest proof coverage.
    ///
    /// Inputs:
    /// - `metadata`: CoreIR module metadata produced by `terlan_typeck`.
    ///
    /// Output:
    /// - Phase-manifest proof coverage counts.
    ///
    /// Transformation:
    /// - Copies deterministic CoreIR proof-coverage counters into the manifest
    ///   schema without exposing the full CoreIR payload.
    pub(crate) fn from_core_metadata(metadata: &terlan_typeck::CoreModuleMetadata) -> Self {
        Self {
            readiness: metadata.proof_readiness.as_str().to_string(),
            lean_covered: metadata.lean_covered_expr_count,
            partial: metadata.partial_expr_count,
            proof_model_required: metadata.proof_model_required_expr_count,
            runtime_boundary: metadata.runtime_boundary_expr_count,
            artifact_only: metadata.artifact_only_expr_count,
            pattern_lean_covered: metadata.lean_covered_pattern_count,
            pattern_partial: metadata.partial_pattern_count,
            pattern_proof_model_required: metadata.proof_model_required_pattern_count,
            pattern_runtime_boundary: metadata.runtime_boundary_pattern_count,
            pattern_artifact_only: metadata.artifact_only_pattern_count,
            typed_core_expr: metadata.typed_core_expr_count,
            summary_only_expr: metadata.summary_only_expr_count,
            typed_core_pattern: metadata.typed_core_pattern_count,
            summary_only_pattern: metadata.summary_only_pattern_count,
            typed_core_type: metadata.typed_core_type_count,
            summary_only_type: metadata.summary_only_type_count,
            checked_preservation_expr: metadata.checked_preservation_expr_count,
            checked_preservation_pattern: metadata.checked_preservation_pattern_count,
            checked_preservation_expr_structural: metadata
                .checked_preservation_expr_structural_count,
            checked_preservation_pattern_structural: metadata
                .checked_preservation_pattern_structural_count,
            checked_preservation_expr_no_runtime_bindings: metadata
                .checked_preservation_expr_no_runtime_bindings_count,
            checked_preservation_pattern_no_runtime_bindings: metadata
                .checked_preservation_pattern_no_runtime_bindings_count,
            checked_preservation_expr_runtime_bindings_required: metadata
                .checked_preservation_expr_runtime_bindings_required_count,
            checked_preservation_pattern_runtime_bindings_required: metadata
                .checked_preservation_pattern_runtime_bindings_required_count,
            resolved_constructor_call_identity: metadata.resolved_constructor_call_identity_count,
            resolved_constructor_chain_identity: metadata.resolved_constructor_chain_identity_count,
            resolved_constructor_pattern_identity: metadata
                .resolved_constructor_pattern_identity_count,
            unresolved_constructor_call_candidate: metadata
                .unresolved_constructor_call_candidate_count,
            unresolved_constructor_chain_candidate: metadata
                .unresolved_constructor_chain_candidate_count,
            unresolved_constructor_pattern_candidate: metadata
                .unresolved_constructor_pattern_candidate_count,
        }
    }

    /// Computes the total number of non-default CoreIR proof metrics.
    ///
    /// Inputs:
    /// - `self`: phase-manifest proof coverage counts.
    ///
    /// Output:
    /// - Sum of proof-coverage and typed-payload metric buckets.
    ///
    /// Transformation:
    /// - Adds the serialized expression coverage, pattern coverage, and
    ///   typed-payload counters without inspecting full CoreIR artifacts.
    fn total(&self) -> usize {
        self.lean_covered
            + self.partial
            + self.proof_model_required
            + self.runtime_boundary
            + self.artifact_only
            + self.pattern_lean_covered
            + self.pattern_partial
            + self.pattern_proof_model_required
            + self.pattern_runtime_boundary
            + self.pattern_artifact_only
            + self.typed_core_expr
            + self.summary_only_expr
            + self.typed_core_pattern
            + self.summary_only_pattern
            + self.typed_core_type
            + self.summary_only_type
            + self.checked_preservation_expr
            + self.checked_preservation_pattern
            + self.checked_preservation_expr_structural
            + self.checked_preservation_pattern_structural
            + self.checked_preservation_expr_no_runtime_bindings
            + self.checked_preservation_pattern_no_runtime_bindings
            + self.checked_preservation_expr_runtime_bindings_required
            + self.checked_preservation_pattern_runtime_bindings_required
            + self.resolved_constructor_call_identity
            + self.resolved_constructor_chain_identity
            + self.resolved_constructor_pattern_identity
            + self.unresolved_constructor_call_candidate
            + self.unresolved_constructor_chain_candidate
            + self.unresolved_constructor_pattern_candidate
    }

    /// Computes the readiness label implied by expression and pattern buckets.
    ///
    /// Inputs:
    /// - `self`: phase-manifest proof coverage counts.
    ///
    /// Output:
    /// - Expected readiness label derived from the same precedence used by
    ///   CoreIR metadata construction.
    ///
    /// Transformation:
    /// - Combines expression and pattern proof-coverage buckets, treats
    ///   summary-only CoreType positions as proof-model debt, then applies the
    ///   CoreIR readiness order: runtime-boundary, partial,
    ///   proof-model-required, artifact-only, lean-covered, no-expressions.
    fn expected_readiness(&self) -> &'static str {
        if self.runtime_boundary + self.pattern_runtime_boundary > 0 {
            "runtime-boundary"
        } else if self.partial + self.pattern_partial > 0 {
            "partial"
        } else if self.proof_model_required
            + self.pattern_proof_model_required
            + self.summary_only_type
            > 0
        {
            "proof-model-required"
        } else if self.artifact_only + self.pattern_artifact_only > 0 {
            "artifact-only"
        } else if self.lean_covered + self.pattern_lean_covered > 0 {
            "lean-covered"
        } else {
            "no-expressions"
        }
    }

    /// Validates typed payload and checked-preservation consistency.
    ///
    /// Inputs:
    /// - `self`: phase-manifest proof coverage and typed-payload counts.
    ///
    /// Output:
    /// - `Ok(())` when expression and pattern typed payload counters cover all
    ///   Lean-covered terms, checked-preservation evidence is within the typed
    ///   payload range, and constructor candidates have no unresolved semantic
    ///   resolution debt.
    /// - `Err(String)` describing the first mismatch.
    ///
    /// Transformation:
    /// - Compares current compiler invariants without reading full CoreIR
    ///   artifacts: Lean-covered expression/pattern summaries must have typed
    ///   payloads and checked-preservation evidence, while partial and
    ///   proof-model-required summaries may also carry typed payloads.
    fn validate_typed_payload_consistency(&self) -> Result<(), String> {
        if self.readiness == "none" && self.total() != 0 {
            return Err(
                "phase manifest CoreIR proof readiness none requires zero coverage counters"
                    .to_string(),
            );
        }
        if self.readiness != "none" && self.readiness != self.expected_readiness() {
            return Err(format!(
                "phase manifest CoreIR proof readiness {} does not match coverage buckets {}",
                self.readiness,
                self.expected_readiness()
            ));
        }
        if self.lean_covered > self.typed_core_expr {
            return Err(
                "phase manifest lean-covered expression count cannot exceed typed CoreExpr count"
                    .to_string(),
            );
        }
        if self.checked_preservation_expr > self.typed_core_expr {
            return Err(
                "phase manifest checked-preservation expression count cannot exceed typed CoreExpr count"
                    .to_string(),
            );
        }
        if self.checked_preservation_expr < self.lean_covered {
            return Err(
                "phase manifest checked-preservation expression count must cover all lean-covered expressions"
                    .to_string(),
            );
        }
        if self.checked_preservation_expr_structural != self.checked_preservation_expr {
            return Err(
                "phase manifest structural checked-preservation expression count must match checked-preservation expression count"
                    .to_string(),
            );
        }
        if self.checked_preservation_expr_no_runtime_bindings
            + self.checked_preservation_expr_runtime_bindings_required
            != self.checked_preservation_expr
        {
            return Err(
                "phase manifest checked-preservation expression freshness counts must partition checked-preservation expression count"
                    .to_string(),
            );
        }
        if self.pattern_lean_covered > self.typed_core_pattern {
            return Err(
                "phase manifest lean-covered pattern count cannot exceed typed CorePattern count"
                    .to_string(),
            );
        }
        if self.checked_preservation_pattern > self.typed_core_pattern {
            return Err(
                "phase manifest checked-preservation pattern count cannot exceed typed CorePattern count"
                    .to_string(),
            );
        }
        if self.checked_preservation_pattern < self.pattern_lean_covered {
            return Err(
                "phase manifest checked-preservation pattern count must cover all lean-covered patterns"
                    .to_string(),
            );
        }
        if self.checked_preservation_pattern_structural != self.checked_preservation_pattern {
            return Err(
                "phase manifest structural checked-preservation pattern count must match checked-preservation pattern count"
                    .to_string(),
            );
        }
        if self.checked_preservation_pattern_no_runtime_bindings
            + self.checked_preservation_pattern_runtime_bindings_required
            != self.checked_preservation_pattern
        {
            return Err(
                "phase manifest checked-preservation pattern freshness counts must partition checked-preservation pattern count"
                    .to_string(),
            );
        }
        if self.readiness == "lean-covered"
            && (self.checked_preservation_expr != self.lean_covered
                || self.checked_preservation_pattern != self.pattern_lean_covered)
        {
            return Err(
                "lean-covered manifests require full checked-preservation evidence for covered terms"
                    .to_string(),
            );
        }
        if self.unresolved_constructor_call_candidate
            + self.unresolved_constructor_chain_candidate
            + self.unresolved_constructor_pattern_candidate
            != 0
        {
            return Err(PHASE_MANIFEST_UNRESOLVED_CONSTRUCTOR_ERROR.to_string());
        }
        if self.readiness != "none" && self.typed_core_type + self.summary_only_type == 0 {
            return Err(
                "phase manifest with CoreIR readiness must include CoreType signature payload counts"
                    .to_string(),
            );
        }
        Ok(())
    }
}

pub(crate) const PHASE_MANIFEST_SCHEMA: &str = "terlan-phase-manifest-v1";

/// Serializable phase manifest for one compiler pass.
///
/// Inputs:
/// - Source path, syntax contract identity, phase outputs, dependencies, and
///   optional CoreIR proof coverage collected during command execution.
///
/// Output:
/// - Manifest payload written to validation artifacts and consumed by phase
///   determinism checks.
///
/// Transformation:
/// - Collapses per-phase runtime state into a stable JSON-serializable
///   structure without embedding full compiler artifacts.
#[derive(serde::Serialize)]
struct PhaseManifest<'a> {
    schema: &'static str,
    module: &'a str,
    source_path: &'a str,
    debug_trace: PhaseManifestDebugTrace<'a>,
    syntax_contract: SyntaxContractIdentity,
    source_hash: u64,
    interface_hash: u64,
    interface_doc_hash: u64,
    core_ir_hash: u64,
    core_proof_coverage: PhaseManifestCoreProofCoverage,
    dependencies: Vec<PhaseManifestDependency<'a>>,
    phases: &'a [PhaseOutput],
}

/// Source dependency entry serialized in a phase manifest.
///
/// Inputs:
/// - Dependency path and content fingerprint discovered during compilation.
///
/// Output:
/// - Stable dependency metadata for manifest comparison.
///
/// Transformation:
/// - Records enough identity to detect dependency drift without embedding the
///   dependency source text.
#[derive(serde::Serialize)]
struct PhaseManifestDependency<'a> {
    name: &'a str,
    hash: u64,
}

/// Source-to-artifact debug identity serialized into phase manifests.
#[derive(serde::Serialize)]
pub(crate) struct PhaseManifestDebugTrace<'a> {
    pub(crate) module: &'a str,
    pub(crate) source_path: &'a str,
    pub(crate) core_ir_hash: u64,
    pub(crate) core_ir_available: bool,
    pub(crate) generated_artifact_kind: &'a str,
    pub(crate) generated_artifact_name: Option<&'a str>,
}

/// Decoded source-to-artifact debug identity used by validation tests.
#[derive(serde::Deserialize)]
pub(crate) struct PhaseManifestDebugTraceSnapshot {
    pub(crate) module: String,
    pub(crate) source_path: String,
    pub(crate) core_ir_hash: u64,
    pub(crate) core_ir_available: bool,
    pub(crate) generated_artifact_kind: String,
    pub(crate) generated_artifact_name: Option<String>,
}

/// Decoded phase manifest snapshot used by validation tests.
#[derive(serde::Deserialize)]
pub(crate) struct PhaseManifestSnapshot {
    pub(crate) schema: String,
    pub(crate) module: String,
    pub(crate) source_path: String,
    pub(crate) debug_trace: PhaseManifestDebugTraceSnapshot,
    pub(crate) syntax_contract: SyntaxContractIdentity,
    pub(crate) core_ir_hash: u64,
    pub(crate) core_proof_coverage: PhaseManifestCoreProofCoverage,
    pub(crate) phases: Vec<PhaseOutputSnapshot>,
}

/// Decoded phase snapshot used by validation tests.
#[derive(serde::Deserialize)]
pub(crate) struct PhaseOutputSnapshot {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) diagnostics: Vec<PhaseDiagnosticSnapshot>,
}

/// Decoded diagnostic snapshot used by validation tests.
#[derive(serde::Deserialize)]
pub(crate) struct PhaseDiagnosticSnapshot {
    pub(crate) code: String,
}

/// Creates a deterministic phase-manifest phase entry.
///
/// Inputs:
/// - `name`: phase name such as `parse`, `resolve`, or `typecheck`.
/// - `status`: phase status string.
/// - `diagnostics`: diagnostics produced by that phase.
///
/// Output:
/// - A `PhaseOutput` with diagnostics sorted for stable manifests.
///
/// Transformation:
/// - Sorts diagnostics by path, span, severity, code, message, and notes before
///   constructing the phase record.
pub(crate) fn create_phase(
    name: &'static str,
    status: &'static str,
    diagnostics: Vec<PhaseManifestDiagnostic>,
) -> PhaseOutput {
    let mut diagnostics = diagnostics;
    diagnostics.sort_by(|left, right| {
        (
            &left.path,
            left.span_start,
            left.span_end,
            left.severity,
            left.code,
            &left.message,
            &left.notes,
        )
            .cmp(&(
                &right.path,
                right.span_start,
                right.span_end,
                right.severity,
                right.code,
                &right.message,
                &right.notes,
            ))
    });

    PhaseOutput {
        name,
        status,
        diagnostics,
    }
}

/// Writes a best-effort phase manifest for an error path, then returns the exit code.
///
/// Inputs:
/// - `manifest_path`: optional target path.
/// - `source_path`: source path being checked.
/// - `source_hash`: source hash when available.
/// - `phases`: already known phase outputs.
/// - `additional_phases`: extra phase outputs to append.
/// - `exit_code`: exit code to return.
///
/// Output:
/// - The supplied `exit_code`.
///
/// Transformation:
/// - Combines phase lists and writes a manifest when a manifest path is present;
///   manifest write failures are logged but do not replace the original exit
///   code.
pub(crate) fn emit_or_log_phase_manifest_error(
    manifest_path: Option<&Path>,
    source_path: &str,
    source_hash: u64,
    phases: &[PhaseOutput],
    additional_phases: &[PhaseOutput],
    exit_code: ExitCode,
) -> ExitCode {
    if let Some(manifest_path) = manifest_path {
        let mut output_phases = Vec::new();
        for phase in phases {
            output_phases.push(phase.clone());
        }
        for phase in additional_phases {
            output_phases.push(phase.clone());
        }

        if let Err(err) = emit_phase_manifest(
            manifest_path,
            source_path,
            None,
            source_hash,
            0,
            0,
            0,
            PhaseManifestCoreProofCoverage::default(),
            &[],
            &output_phases,
        ) {
            eprintln!(
                "failed to write phase manifest after {} check: {}",
                source_path, err
            );
        }
    }
    exit_code
}

/// Serializes and writes a phase manifest.
///
/// Inputs:
/// - `manifest_path`: file path to write.
/// - `source_path`: source path represented by the manifest.
/// - `module_name`: parsed module name, or `None` for unparsed sources.
/// - `source_hash`: content hash for the source.
/// - `interface_hash`: hash of interface type text.
/// - `interface_doc_hash`: hash of interface documentation text.
/// - `core_ir_hash`: hash of deterministic CoreIR contract text, or `0` when
///   CoreIR was not produced.
/// - `core_proof_coverage`: aggregate proof-coverage counts for produced
///   CoreIR, or zero counts when CoreIR was not produced.
/// - `dependencies`: dependency name/hash pairs.
/// - `phases`: ordered phase outputs.
///
/// Output:
/// - `Ok(())` when the manifest is valid and written.
/// - `Err(String)` for identity, JSON, validation, or write failures.
///
/// Transformation:
/// - Attaches the current syntax contract identity, validates the generated JSON
///   manifest, and writes it with a trailing newline.
pub(crate) fn emit_phase_manifest(
    manifest_path: &Path,
    source_path: &str,
    module_name: Option<&str>,
    source_hash: u64,
    interface_hash: u64,
    interface_doc_hash: u64,
    core_ir_hash: u64,
    core_proof_coverage: PhaseManifestCoreProofCoverage,
    dependencies: &[(String, u64)],
    phases: &[PhaseOutput],
) -> Result<(), String> {
    let module_name = module_name.unwrap_or("<unparsed>");
    let manifest_dependencies = dependencies
        .iter()
        .map(|(name, hash)| PhaseManifestDependency {
            name: name.as_str(),
            hash: *hash,
        })
        .collect();
    let manifest = PhaseManifest {
        schema: PHASE_MANIFEST_SCHEMA,
        module: module_name,
        source_path,
        debug_trace: PhaseManifestDebugTrace {
            module: module_name,
            source_path,
            core_ir_hash,
            core_ir_available: core_ir_hash != 0,
            generated_artifact_kind: "none",
            generated_artifact_name: None,
        },
        syntax_contract: current_syntax_contract_identity()?,
        source_hash,
        interface_hash,
        interface_doc_hash,
        core_ir_hash,
        core_proof_coverage,
        dependencies: manifest_dependencies,
        phases,
    };
    let manifest = serde_json::to_string(&manifest).map_err(|err| err.to_string())?;
    validate_phase_manifest_contents(&manifest)?;

    std::fs::write(manifest_path, format!("{}\n", manifest)).map_err(|err| err.to_string())
}

/// Validates serialized phase-manifest contents.
///
/// Inputs:
/// - `contents`: raw JSON manifest string.
///
/// Output:
/// - Decoded `PhaseManifestSnapshot` on success.
/// - `Err(String)` when JSON, schema, syntax identity, module, CoreIR hash,
///   phase, or diagnostic requirements fail.
///
/// Transformation:
/// - Parses JSON, validates the schema and embedded syntax contract identity,
///   checks required non-empty fields, and requires a non-zero CoreIR hash for
///   manifests whose core phase completed successfully.
pub(crate) fn validate_phase_manifest_contents(
    contents: &str,
) -> Result<PhaseManifestSnapshot, String> {
    let manifest = serde_json::from_str::<PhaseManifestSnapshot>(contents)
        .map_err(|err| format!("invalid phase manifest JSON: {err}"))?;
    if manifest.schema != PHASE_MANIFEST_SCHEMA {
        return Err(format!(
            "invalid phase manifest schema: expected {}, found {}",
            PHASE_MANIFEST_SCHEMA, manifest.schema
        ));
    }
    if !syntax_contract_identity_matches_current(&manifest.syntax_contract)
        .map_err(|error| format!("failed to validate syntax contract identity: {error:?}"))?
    {
        return Err("phase manifest syntax contract identity mismatch".to_string());
    }
    if manifest.module.is_empty() {
        return Err("phase manifest module must not be empty".to_string());
    }
    validate_phase_manifest_debug_trace(&manifest)?;
    if manifest.core_ir_hash == 0
        && (manifest.core_proof_coverage.total() != 0
            || manifest.core_proof_coverage.readiness != "none")
    {
        return Err("phase manifest CoreIR proof coverage requires a CoreIR hash".to_string());
    }
    if manifest.core_ir_hash != 0 && manifest.core_proof_coverage.readiness == "none" {
        return Err(
            "phase manifest CoreIR proof readiness must not be none when CoreIR exists".to_string(),
        );
    }
    if manifest.core_ir_hash != 0 {
        manifest
            .core_proof_coverage
            .validate_typed_payload_consistency()?;
    }
    for phase in &manifest.phases {
        if phase.name.is_empty() {
            return Err("phase manifest phase name must not be empty".to_string());
        }
        if phase.name == "core" && phase.status == "ok" && manifest.core_ir_hash == 0 {
            return Err(
                "phase manifest CoreIR hash must not be zero when core phase is ok".to_string(),
            );
        }
        for diagnostic in &phase.diagnostics {
            if diagnostic.code.is_empty() {
                return Err("phase manifest diagnostic code must not be empty".to_string());
            }
        }
    }
    Ok(manifest)
}

/// Validates source-to-artifact debug identity in a phase manifest.
///
/// Inputs:
/// - `manifest`: decoded phase-manifest snapshot.
///
/// Output:
/// - `Ok(())` when debug identity agrees with top-level manifest identity.
/// - `Err(String)` describing the first debug-trace mismatch.
///
/// Transformation:
/// - Checks that the nested debug trace repeats the source module/path/CoreIR
///   identity exactly and that artifact metadata is explicit even when the
///   command did not emit a backend artifact.
fn validate_phase_manifest_debug_trace(manifest: &PhaseManifestSnapshot) -> Result<(), String> {
    if manifest.source_path.is_empty() {
        return Err("phase manifest source path must not be empty".to_string());
    }
    if manifest.debug_trace.module != manifest.module {
        return Err("phase manifest debug trace module must match manifest module".to_string());
    }
    if manifest.debug_trace.source_path != manifest.source_path {
        return Err(
            "phase manifest debug trace source path must match manifest source path".to_string(),
        );
    }
    if manifest.debug_trace.core_ir_hash != manifest.core_ir_hash {
        return Err(
            "phase manifest debug trace CoreIR hash must match manifest CoreIR hash".to_string(),
        );
    }
    if manifest.debug_trace.core_ir_available != (manifest.core_ir_hash != 0) {
        return Err(
            "phase manifest debug trace CoreIR availability must match CoreIR hash".to_string(),
        );
    }
    if manifest.debug_trace.generated_artifact_kind.is_empty() {
        return Err(
            "phase manifest debug trace generated artifact kind must not be empty".to_string(),
        );
    }
    if let Some(name) = &manifest.debug_trace.generated_artifact_name {
        if name.is_empty() {
            return Err(
                "phase manifest debug trace generated artifact name must not be empty".to_string(),
            );
        }
    }
    Ok(())
}

/// Loads the compiler's current syntax contract identity.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Current `SyntaxContractIdentity` on success.
/// - `Err(String)` if the cached canonical contract cannot be loaded.
///
/// Transformation:
/// - Delegates to `terlan_syntax` and converts its error into CLI text.
pub(crate) fn current_syntax_contract_identity() -> Result<SyntaxContractIdentity, String> {
    cached_canonical_terlan_syntax_contract_identity()
        .map_err(|error| format!("failed to load syntax contract identity: {error:?}"))
}

#[cfg(test)]
#[path = "phase_manifest_test.rs"]
mod phase_manifest_test;
