use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use super::rustdoc_analysis::*;

pub use super::achamp_adversarial_coverage::{
    run_achamp_adversarial_coverage, AChampAdversarialCoverageSummary,
};
pub use super::aot_developer_hot_reload::{
    run_aot_developer_hot_reload, AotDeveloperHotReloadSummary,
};
pub use super::binary_descriptor_contract::{
    run_binary_descriptor_contract, BinaryDescriptorContractSummary,
};
pub use super::cli_exact_selectors::{run_cli_exact_selectors, CliExactSelectorSummary};
pub use super::compiler_incremental_cache::{
    run_compiler_incremental_cache, CompilerIncrementalCacheSummary,
};
pub use super::core_typing_spec::{run_core_typing_spec, CoreTypingSpecSummary};
pub use super::cuda_package_availability::{
    run_cuda_package_availability, run_cuda_package_check, CudaPackageAvailabilitySummary,
};
pub use super::dev_fast_feedback_profile::{
    run_dev_fast_feedback_profile, DevFastFeedbackProfileSummary,
};
pub use super::device_target_planner::{run_device_target_planner, DeviceTargetPlannerSummary};
pub use super::dormant_runtime_code::{run_dormant_runtime_code, DormantRuntimeCodeSummary};
pub use super::editor_code_action_auto_import_report::{
    run_editor_code_action_auto_import_report, EditorCodeActionAutoImportReportSummary,
};
pub use super::editor_completion_signature_report::{
    run_editor_completion_signature_report, EditorCompletionSignatureReportSummary,
};
pub use super::editor_definition_navigation_report::{
    run_editor_definition_navigation_report, EditorDefinitionNavigationReportSummary,
};
pub use super::erlang_backend_classification::{
    run_erlang_backend_classification, ErlangBackendClassificationSummary,
};
pub use super::executable_docs_vm::{run_executable_docs_vm, ExecutableDocsVmSummary};
pub use super::function_head_migration_diagnostic_policy::{
    run_function_head_migration_diagnostic_policy, FunctionHeadMigrationDiagnosticPolicySummary,
};
pub use super::function_head_migration_lint::{
    run_function_head_migration_lint, FunctionHeadMigrationLintSummary,
};
pub use super::function_head_pattern_handoff::{
    run_function_head_pattern_handoff, FunctionHeadPatternHandoffSummary,
};
pub use super::function_head_pattern_migration_assist::{
    run_function_head_pattern_migration_assist, FunctionHeadPatternMigrationAssistSummary,
};
pub use super::function_head_pattern_migration_benchmark::{
    run_function_head_pattern_migration_benchmark, FunctionHeadPatternMigrationBenchmarkSummary,
};
pub use super::function_head_pattern_migration_docs::{
    run_function_head_pattern_migration_docs, FunctionHeadPatternMigrationDocsSummary,
};
pub use super::hex_target_metadata::{run_hex_target_metadata, HexTargetMetadataSummary};
pub(crate) use super::inline_tests::has_inline_test_marker;
pub use super::internal_docs::{run_internal_docs, InternalDocFinding, InternalDocsSummary};
pub use super::js_type_emission_contract::{
    run_js_type_emission_contract, JsTypeEmissionContractSummary,
};
pub use super::language_feature_full_coverage::{
    run_language_feature_coverage_100, LanguageFeatureCoverage100Summary,
};
pub use super::lean_proof_feature_cull::{
    run_lean_proof_feature_cull, LeanProofFeatureCullSummary,
};
pub use super::lean_proof_pr::{run_lean_proof_pr, LeanProofPrSummary};
pub use super::lean_proof_regression::{run_lean_proof_regression, LeanProofRegressionSummary};
pub use super::lean_proof_runtime::{run_lean_proof_runtime, LeanProofRuntimeSummary};
pub use super::lean_proof_track::{run_lean_proof_track, LeanProofTrackSummary};
pub use super::module_readmes::{run_module_readmes, ModuleReadmeSummary};
pub use super::multicore_invariant_inventory::{
    run_multicore_invariant_inventory, MulticoreInvariantInventorySummary,
};
pub use super::native_binding_generator_contract::{
    run_native_binding_generator_contract, NativeBindingGeneratorContractSummary,
};
pub use super::native_boundary_security::{
    run_native_boundary_security, NativeBoundarySecuritySummary,
};
pub use super::native_boundary_terminology::{
    run_native_boundary_terminology, NativeBoundaryTerminologySummary,
};
pub use super::no_default_tokio_runtime::{
    run_no_default_tokio_runtime, NoDefaultTokioRuntimeSummary,
};
pub use super::no_implicit_otp_runtime::{
    run_no_implicit_otp_runtime, NoImplicitOtpRuntimeSummary,
};
pub use super::no_terlan_vm_erts_rust_dependency::{
    run_no_terlan_vm_erts_rust_dependency, NoTerlanVmErtsRustDependencySummary,
};
pub use super::operator_full_coverage::{run_operator_coverage_100, OperatorCoverage100Summary};
pub use super::otp_reference_inventory::{
    run_otp_reference_inventory, OtpReferenceInventorySummary,
};
pub use super::otp_runtime_exit::{run_otp_runtime_exit, OtpRuntimeExitSummary};
pub use super::otp_test_pipeline_inventory::{
    run_otp_test_pipeline_inventory, OtpTestPipelineInventorySummary,
};
pub use super::oxc_boundary::{run_oxc_boundary, OxcBoundaryFinding, OxcBoundarySummary};
pub use super::package_api_compatibility::{
    run_package_api_compatibility, PackageApiCompatibilitySummary,
};
pub use super::package_build_artifact_isolation::{
    run_package_build_artifact_isolation, PackageBuildArtifactIsolationSummary,
};
pub use super::package_cache_integrity::{
    run_package_cache_integrity, PackageCacheIntegritySummary,
};
pub use super::package_capability_contract::{
    run_package_capability_contract, PackageCapabilityContractSummary,
};
pub use super::package_cli_workflow::{run_package_cli_workflow, PackageCliWorkflowSummary};
pub use super::package_editor_integration::{
    run_package_editor_integration, PackageEditorIntegrationSummary,
};
pub use super::package_git_source::{run_package_git_source, PackageGitSourceSummary};
pub use super::package_lockfile_contract::{
    run_package_lockfile_contract, PackageLockfileContractSummary,
};
pub use super::package_registry_publish::{
    run_package_registry_publish, PackageRegistryPublishSummary,
};
pub use super::package_release_test_matrix::{
    run_package_release_test_matrix, PackageReleaseTestMatrixSummary,
};
pub use super::package_resolver_reproducibility::{
    run_package_resolver_reproducibility, PackageResolverReproducibilitySummary,
};
pub use super::package_workspace_graph::{
    run_package_workspace_graph, PackageWorkspaceGraphSummary,
};
pub use super::pattern_matching_support::{
    run_pattern_matching_support, PatternMatchingSupportSummary,
};
pub use super::release_code_hygiene::{run_release_code_hygiene, ReleaseCodeHygieneSummary};
pub use super::release_failure_reproduction::{
    run_release_failure_reproduction, ReleaseFailureReproductionSummary,
};
pub use super::release_flake_detection::{
    run_release_flake_detection, ReleaseFlakeDetectionSummary,
};
pub use super::release_gate_duration_budget::{
    run_release_gate_duration_budget, ReleaseGateDurationBudgetSummary,
};
pub use super::release_gate_report_schema::{
    run_release_gate_report_schema, ReleaseGateReportSchemaSummary,
};
pub use super::release_gate_shard_resume::{
    run_release_gate_shard_resume, ReleaseGateShardResumeSummary,
};
pub use super::roadmap_gate_integrity::{run_roadmap_gate_integrity, RoadmapGateIntegritySummary};
pub use super::rust_build_feature_shipping::{
    run_rust_build_feature_shipping, RustBuildFeatureShippingSummary,
};
pub use super::shape_implications::{run_shape_implications, ShapeImplicationsSummary};
pub use super::source_map_debug_info::{run_source_map_debug_info, SourceMapDebugInfoSummary};
pub use super::std_generated_metadata::{run_std_generated_metadata, StdGeneratedMetadataSummary};
pub use super::std_package_full_coverage::{
    run_std_package_coverage_100, StdPackageCoverage100Summary,
};
pub use super::std_source_naming::{run_std_source_naming, StdSourceNamingSummary};
pub use super::std_test_honesty::{run_std_test_honesty, StdTestHonestySummary};
pub use super::terlan_lint_style_profile::{
    run_terlan_lint_style_profile, TerlanLintStyleProfileSummary,
};
pub use super::terlan_vm_external_repo_boundary::{
    run_terlan_vm_external_repo_boundary, TerlanVmExternalRepoBoundarySummary,
};
pub use super::terlan_vm_internal_crate::{
    run_terlan_vm_internal_crate, TerlanVmInternalCrateSummary,
};
pub use super::test_hierarchy::{run_test_hierarchy, ScriptInvocation, TestHierarchySummary};
pub use super::typed_template_render_mode::{
    run_typed_template_render_mode, TypedTemplateRenderModeSummary,
};
pub use super::vm_artifact_format::{run_vm_artifact_format, VmArtifactFormatSummary};
pub use super::vm_db_migration_command::{
    run_vm_db_migration_command, VmDbMigrationCommandSummary,
};
pub use super::vm_deterministic_hashmap::{
    run_vm_deterministic_hashmap, VmDeterministicHashMapSummary,
};
pub use super::vm_dev_dependency_orchestration::{
    run_vm_dev_dependency_orchestration, VmDevDependencyOrchestrationSummary,
};
pub use super::vm_diagnostics_quality::{run_vm_diagnostics_quality, VmDiagnosticsQualitySummary};
pub use super::vm_http_acme_cache_custody::{
    run_vm_http_acme_cache_custody, VmHttpAcmeCacheCustodySummary,
};
pub use super::vm_http_acme_renewal::{run_vm_http_acme_renewal, VmHttpAcmeRenewalSummary};
pub use super::vm_http_acme_worker::{run_vm_http_acme_worker, VmHttpAcmeWorkerSummary};
pub use super::vm_http_handler_scheduler_fairness::{
    run_vm_http_handler_scheduler_fairness, VmHttpHandlerSchedulerFairnessSummary,
};
pub use super::vm_http_stateful_actor_session::{
    run_vm_http_stateful_actor_session, VmHttpStatefulActorSessionSummary,
};
pub use super::vm_io_reactor_runtime::{run_vm_io_reactor_runtime, VmIoReactorRuntimeSummary};
pub use super::vm_live_template_client_protocol::{
    run_vm_live_template_client_protocol, VmLiveTemplateClientProtocolSummary,
};
pub use super::vm_live_template_stream::{
    run_vm_live_template_stream, VmLiveTemplateStreamSummary,
};
pub use super::vm_model_sync_store::{run_vm_model_sync_store, VmModelSyncStoreSummary};
pub use super::vm_native_worker_runtime::{
    run_vm_native_worker_runtime, VmNativeWorkerRuntimeSummary,
};
pub use super::vm_otp_abstractions_terlan_stdlib::{
    run_vm_otp_abstractions_terlan_stdlib, VmOtpAbstractionsTerlanStdlibSummary,
};
pub use super::vm_ownership_classification::{
    run_vm_ownership_classification, VmOwnershipClassificationSummary,
};
pub use super::vm_persistent_actor_adapter::{
    run_vm_persistent_actor_adapter, VmPersistentActorAdapterSummary,
};
pub use super::vm_persistent_actor_compaction::{
    run_vm_persistent_actor_compaction, VmPersistentActorCompactionSummary,
};
pub use super::vm_persistent_actor_performance::{
    run_vm_persistent_actor_performance, VmPersistentActorPerformanceSummary,
};
pub use super::vm_persistent_actor_policy::{
    run_vm_persistent_actor_policy, VmPersistentActorPolicySummary,
};
pub use super::vm_persistent_actor_restore::{
    run_vm_persistent_actor_restore, VmPersistentActorRestoreSummary,
};
pub use super::vm_persistent_actor_schema::{
    run_vm_persistent_actor_schema, VmPersistentActorSchemaSummary,
};
pub use super::vm_persistent_actor_store::{
    run_vm_persistent_actor_store, VmPersistentActorStoreSummary,
};
pub use super::vm_persistent_actor_telemetry::{
    run_vm_persistent_actor_telemetry, VmPersistentActorTelemetrySummary,
};
pub use super::vm_release_install_validation::{
    run_vm_release_install_validation, VmReleaseInstallValidationSummary,
};
pub use super::vm_runtime_concept_inventory::{
    run_vm_runtime_concept_inventory, VmRuntimeConceptInventorySummary,
};
pub use super::vm_sql_macro_validation::{
    run_vm_sql_macro_validation, VmSqlMacroValidationSummary,
};
pub use super::vm_supervision_restart::{run_vm_supervision_restart, VmSupervisionRestartSummary};
pub use super::vm_web_config_secret_boundary::{
    run_vm_web_config_secret_boundary, VmWebConfigSecretBoundarySummary,
};
pub use super::vm_web_deployment_profile::{
    run_vm_web_deployment_profile, VmWebDeploymentProfileSummary,
};
pub use super::vm_web_lifecycle_health::{
    run_vm_web_lifecycle_health, VmWebLifecycleHealthSummary,
};
pub use super::vm_web_observability::{run_vm_web_observability, VmWebObservabilitySummary};
pub use super::vm_web_route_schema_client::{
    run_vm_web_route_schema_client, VmWebRouteSchemaClientSummary,
};
pub use super::vm_web_security_policy::{run_vm_web_security_policy, VmWebSecurityPolicySummary};
pub use super::watch_mode_hot_reload::{run_watch_mode_hot_reload, WatchModeHotReloadSummary};
pub use super::web_asset_pipeline::{run_web_asset_pipeline, WebAssetPipelineSummary};

/// Maximum lines allowed in Rust implementation files without a baseline row.
///
/// CQ-2 lowers the former 1,000-line ceiling after the fragment migration so
/// an implementation cannot sit exactly on the old mechanical split target.
pub const IMPL_LINE_LIMIT: usize = 999;

/// Maximum lines allowed in adjacent Rust test files without a baseline row.
pub const TEST_LINE_LIMIT: usize = 2000;

/// Result alias used by repository quality checks.
pub(crate) type QualityResult<T> = Result<T, String>;

/// Measured Rust source file.
/// Inputs:
/// - `path`: repository-relative Rust source path.
/// - `lines`: number of text lines in the file.
///
/// Output:
/// - Immutable file measurement used by quality checks.
///
/// Transformation:
/// - Keeps path and measured size together so diagnostics can report both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustFile {
    pub path: PathBuf,
    pub lines: usize,
}

impl RustFile {
    /// Returns the configured line limit for this Rust file.
    ///
    /// Inputs:
    /// - The file path.
    ///
    /// Output:
    /// - Test-file line limit for adjacent test files and behavior modules
    ///   nested below `*_test/`, `*_tests/`, or `tests/`.
    /// - Implementation-file line limit for all other Rust files.
    ///
    /// Transformation:
    /// - Uses the same path-aware test classification as the Rustdoc and
    ///   inline-test policies, so named CQ-2 behavior modules remain tests
    ///   after numbered textual fragments become ordinary modules.
    pub fn limit(&self) -> usize {
        if is_rustdoc_test_path(&self.path) {
            TEST_LINE_LIMIT
        } else {
            IMPL_LINE_LIMIT
        }
    }
}

/// Summary produced by the Rust quality check.
/// Inputs:
/// - Current Rust file measurements and inline-test findings.
///
/// Output:
/// - Counts used for stable success diagnostics.
///
/// Transformation:
/// - Separates successful scan metrics from diagnostic failures so callers can
///   render CLI output consistently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustQualitySummary {
    pub oversized_count: usize,
    pub inline_test_count: usize,
}

/// Rust declaration discovered by the documentation checker.
///
/// Inputs:
/// - `path`: repository-relative Rust source path.
/// - `kind`: item category such as `fn`, `struct`, or `trait`.
/// - `name`: declared Rust identifier.
/// - `signature`: normalized declaration line used as a stable baseline key.
/// - `line`: one-based source line for diagnostics.
/// - `documented`: whether adjacent Rustdoc was found.
/// - `contract`: whether an internal declaration is explicitly marked as a
///   subsystem contract or non-obvious algorithm.
///
/// Output:
/// - Immutable item record consumed by baseline validation.
///
/// Transformation:
/// - Keeps declaration identity, source location, and documentation state
///   together so quality diagnostics can be precise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustItem {
    pub path: PathBuf,
    pub kind: String,
    pub name: String,
    pub signature: String,
    pub line: usize,
    pub documented: bool,
    pub contract: bool,
}

impl RustItem {
    /// Returns the baseline key for this Rust item.
    ///
    /// Inputs:
    /// - The item path, kind, name, and normalized signature.
    ///
    /// Output:
    /// - A tab-separated key suitable for checked-in baseline files.
    ///
    /// Transformation:
    /// - Converts path and declaration identity into stable text without
    ///   embedding source line numbers.
    pub fn key(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.path.display(),
            self.kind,
            self.name,
            self.signature
        )
    }

    /// Returns whether this declaration belongs to the high-signal Rustdoc
    /// policy.
    ///
    /// Public APIs, unsafe functions, and explicitly marked internal
    /// contracts require documentation. Self-explanatory private helpers do
    /// not.
    pub(super) fn requires_documentation(&self) -> bool {
        self.signature.starts_with("pub ") || self.signature.contains("unsafe fn ") || self.contract
    }
}

/// Summary produced by the Rustdoc coverage check.
///
/// Inputs:
/// - Current undocumented Rust item set.
///
/// Output:
/// - Count used for stable success diagnostics.
///
/// Transformation:
/// - Gives the command wrapper the same success information previously printed
///   by the Python gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustdocSummary {
    pub undocumented_count: usize,
}

/// Runs the Rust quality baseline checks.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Success summary when quality debt has not grown.
/// - Diagnostics when file-size debt grows, inline-test debt grows, or
///   baselines are stale/malformed.
///
/// Transformation:
/// - Combines Rust file-size and inline-test validation into one permanent
///   repository quality gate.
pub fn run_rust_quality(root: &Path) -> QualityResult<RustQualitySummary> {
    let files = iter_rust_files(root)?;
    let (size_baseline, mut diagnostics) = read_size_baseline(root)?;
    let (inline_baseline, inline_diagnostics) = read_inline_test_baseline(root)?;
    diagnostics.extend(inline_diagnostics);
    diagnostics.extend(check_file_sizes(&files, &size_baseline));
    let inline_tests = files_with_inline_tests(root, &files)?;
    diagnostics.extend(check_inline_tests(&inline_tests, &inline_baseline));

    if !diagnostics.is_empty() {
        return Err(render_failure("rust-quality", &diagnostics));
    }

    let oversized_count = files
        .iter()
        .filter(|file| file.lines > file.limit())
        .count();
    Ok(RustQualitySummary {
        oversized_count,
        inline_test_count: inline_tests.len(),
    })
}

/// Runs Rustdoc coverage validation.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Success summary when undocumented policy items match the baseline.
/// - Diagnostics when documentation coverage regresses or baselines are stale.
///
/// Transformation:
/// - Protects public APIs, unsafe invariants, and explicitly marked subsystem
///   contracts while ignoring self-explanatory private helpers and tests.
/// - Compares remaining documentation debt to the checked-in migration
///   baseline.
pub fn run_rustdoc(root: &Path) -> QualityResult<RustdocSummary> {
    let current = undocumented_items(&discover_rustdoc_items(root)?);
    let (baseline, mut diagnostics) = read_rustdoc_baseline(root)?;
    diagnostics.extend(check_rustdoc_baseline(&current, &baseline));
    if !diagnostics.is_empty() {
        return Err(render_failure("rustdoc", &diagnostics));
    }
    Ok(RustdocSummary {
        undocumented_count: current.len(),
    })
}

/// Rewrites the undocumented Rustdoc baseline.
///
/// Inputs:
/// - `root`: repository root containing `crates/` and `tools/quality/`.
///
/// Output:
/// - Number of undocumented policy items written to the baseline.
///
/// Transformation:
/// - Discovers current undocumented items and serializes their stable keys with
///   the same header used by the previous Python gate.
pub fn write_rustdoc_baseline(root: &Path) -> QualityResult<usize> {
    let current = undocumented_items(&discover_rustdoc_items(root)?);
    let path = rustdoc_baseline_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("{}: failed to create baseline dir: {err}", parent.display()))?;
    }
    let mut lines = vec![
        "# Reviewed public-API and contract Rustdoc debt for the 0.0.7 migration.".to_string(),
        "# Private helpers and tests are intentionally outside this high-signal policy."
            .to_string(),
        "# New public APIs, unsafe functions, and `rustdoc-contract:` items require Rustdoc."
            .to_string(),
    ];
    lines.extend(current.keys().cloned());
    fs::write(&path, format!("{}\n", lines.join("\n")))
        .map_err(|err| format!("{}: failed to write baseline: {err}", path.display()))?;
    Ok(current.len())
}

/// Returns measured Rust files under `crates/`.
///
/// Inputs:
/// - `root`: repository root containing the `crates/` directory.
///
/// Output:
/// - Sorted Rust file measurements.
///
/// Transformation:
/// - Recursively scans `.rs` files, counts lines, and stores paths relative to
///   the repository root for stable baseline matching.
fn iter_rust_files(root: &Path) -> QualityResult<Vec<RustFile>> {
    let crates = root.join("crates");
    let mut files = Vec::new();
    collect_rust_files(root, &crates, &mut files)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// Discovers Rust functions and types under implementation files in `crates/`.
///
/// Inputs:
/// - `root`: repository root containing Rust source files.
///
/// Output:
/// - Sorted Rust item records.
///
/// Transformation:
/// - Skips unit/integration test paths because tests express behavior through
///   their names and assertions rather than mandatory API prose.
/// - Reads each implementation Rust file, matches declaration lines with
///   conservative regexes, and records documentation and explicit contract
///   markers.
fn discover_rustdoc_items(root: &Path) -> QualityResult<Vec<RustItem>> {
    let function_pattern = Regex::new(
        r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern(?:\s+"[^"]+")?)\s+)*fn\s+([A-Za-z_][A-Za-z0-9_]*)\b"#,
    )
    .expect("function regex");
    let type_pattern = Regex::new(
        r"^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|union|trait|type)\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    )
    .expect("type regex");
    let raw_string_open_pattern = Regex::new(r#"b?r(#+)?""#).expect("raw string regex");
    let mut files = iter_rust_files(root)?
        .into_iter()
        .filter(|file| !is_rustdoc_test_path(&file.path))
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.path.cmp(&right.path));

    let mut items = Vec::new();
    for file in files {
        let path = root.join(&file.path);
        let text = fs::read_to_string(&path)
            .map_err(|err| format!("{}: failed to read source: {err}", path.display()))?;
        let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        let mut in_escaped_string = false;
        let mut raw_string_terminator = None::<String>;
        for (index, line) in lines.iter().enumerate() {
            let (next_raw_terminator, skip_raw_string) = raw_string_state(
                line,
                raw_string_terminator.as_deref(),
                &raw_string_open_pattern,
            );
            raw_string_terminator = next_raw_terminator;
            if skip_raw_string {
                continue;
            }

            let (next_escaped, skip_line) = escaped_string_state(line, in_escaped_string);
            in_escaped_string = next_escaped;
            if skip_line {
                continue;
            }

            if let Some(captures) = function_pattern.captures(line) {
                items.push(RustItem {
                    path: file.path.clone(),
                    kind: "fn".to_string(),
                    name: captures
                        .get(1)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    signature: normalized_signature(line),
                    line: index + 1,
                    documented: line_has_rustdoc(&lines, index),
                    contract: line_has_rustdoc_contract_marker(&lines, index),
                });
                continue;
            }
            if let Some(captures) = type_pattern.captures(line) {
                items.push(RustItem {
                    path: file.path.clone(),
                    kind: captures
                        .get(1)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: captures
                        .get(2)
                        .map(|item| item.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    signature: normalized_signature(line),
                    line: index + 1,
                    documented: line_has_rustdoc(&lines, index),
                    contract: line_has_rustdoc_contract_marker(&lines, index),
                });
            }
        }
    }
    Ok(items)
}

/// Recursively collects Rust file measurements.
///
/// Inputs:
/// - `root`: repository root used for relative paths.
/// - `directory`: directory to scan.
/// - `files`: output accumulator.
///
/// Output:
/// - `Ok(())` when scanning succeeds.
/// - Error string when a directory or file cannot be read.
///
/// Transformation:
/// - Walks the filesystem using `std::fs` so the quality crate has no external
///   dependency for this simple repository scan.
fn collect_rust_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<RustFile>,
) -> QualityResult<()> {
    let entries = fs::read_dir(directory)
        .map_err(|err| format!("{}: failed to read directory: {err}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("{}: failed to read entry: {err}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: failed to read file type: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_rust_files(root, &path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            let text = fs::read_to_string(&path)
                .map_err(|err| format!("{}: failed to read source: {err}", path.display()))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("{}: failed to relativize path: {err}", path.display()))?
                .to_path_buf();
            files.push(RustFile {
                path: relative,
                lines: text.lines().count(),
            });
        }
    }
    Ok(())
}
