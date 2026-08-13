use std::env;
use std::path::Path;
use std::process::ExitCode;

use crate::terlan_quality::lean_proof_track::gap_hygiene::run_lean_proof_gap_hygiene;
use crate::terlan_quality::vm_http_benchmark_contract::{
    run_vm_http_benchmark_comparability, run_vm_http_runtime_attribution_contract,
};
use crate::terlan_quality::{
    run_achamp_adversarial_coverage, run_aot_developer_hot_reload, run_binary_descriptor_contract,
    run_cli_exact_selectors, run_compiler_incremental_cache, run_core_typing_spec,
    run_cuda_package_availability, run_cuda_package_check, run_dev_fast_feedback_profile,
    run_device_target_planner, run_dormant_runtime_code, run_editor_code_action_auto_import_report,
    run_editor_completion_signature_report, run_editor_definition_navigation_report,
    run_erlang_backend_classification, run_executable_docs_vm,
    run_function_head_migration_diagnostic_policy, run_function_head_migration_lint,
    run_function_head_pattern_handoff, run_function_head_pattern_migration_assist,
    run_function_head_pattern_migration_benchmark, run_function_head_pattern_migration_docs,
    run_hex_target_metadata, run_internal_docs, run_js_type_emission_contract,
    run_language_feature_coverage_100, run_lean_proof_feature_cull, run_lean_proof_pr,
    run_lean_proof_regression, run_lean_proof_runtime, run_lean_proof_track, run_module_readmes,
    run_multicore_invariant_inventory, run_native_binding_generator_contract,
    run_native_boundary_security, run_native_boundary_terminology, run_no_default_tokio_runtime,
    run_no_implicit_otp_runtime, run_no_terlan_vm_erts_rust_dependency, run_operator_coverage_100,
    run_otp_reference_inventory, run_otp_runtime_exit, run_otp_test_pipeline_inventory,
    run_oxc_boundary, run_package_api_compatibility, run_package_build_artifact_isolation,
    run_package_cache_integrity, run_package_capability_contract, run_package_cli_workflow,
    run_package_editor_integration, run_package_git_source, run_package_lockfile_contract,
    run_package_registry_publish, run_package_release_test_matrix,
    run_package_resolver_reproducibility, run_package_workspace_graph,
    run_pattern_matching_support, run_release_code_hygiene, run_release_failure_reproduction,
    run_release_flake_detection, run_release_gate_duration_budget, run_release_gate_report_schema,
    run_release_gate_shard_resume, run_roadmap_gate_integrity, run_rust_build_feature_shipping,
    run_rust_quality, run_rustdoc, run_shape_implications, run_source_map_debug_info,
    run_std_generated_metadata, run_std_package_coverage_100, run_std_source_naming,
    run_std_test_honesty, run_terlan_lint_style_profile, run_terlan_vm_external_repo_boundary,
    run_terlan_vm_internal_crate, run_test_hierarchy, run_typed_template_render_mode,
    run_vm_artifact_format, run_vm_db_migration_command, run_vm_deterministic_hashmap,
    run_vm_dev_dependency_orchestration, run_vm_diagnostics_quality,
    run_vm_http_acme_cache_custody, run_vm_http_acme_renewal, run_vm_http_acme_worker,
    run_vm_http_handler_scheduler_fairness, run_vm_http_stateful_actor_session,
    run_vm_io_reactor_runtime, run_vm_live_template_client_protocol, run_vm_live_template_stream,
    run_vm_model_sync_store, run_vm_native_worker_runtime, run_vm_otp_abstractions_terlan_stdlib,
    run_vm_ownership_classification, run_vm_persistent_actor_adapter,
    run_vm_persistent_actor_compaction, run_vm_persistent_actor_performance,
    run_vm_persistent_actor_policy, run_vm_persistent_actor_restore,
    run_vm_persistent_actor_schema, run_vm_persistent_actor_store,
    run_vm_persistent_actor_telemetry, run_vm_release_install_validation,
    run_vm_runtime_concept_inventory, run_vm_sql_macro_validation, run_vm_supervision_restart,
    run_vm_web_config_secret_boundary, run_vm_web_deployment_profile, run_vm_web_lifecycle_health,
    run_vm_web_observability, run_vm_web_route_schema_client, run_vm_web_security_policy,
    run_watch_mode_hot_reload, run_web_asset_pipeline, write_rustdoc_baseline,
};

/// Prints one quality-gate failure and returns the standard failure status.
fn failure(message: String) -> ExitCode {
    eprintln!("{message}");
    ExitCode::from(1)
}

mod compiler_and_runtime_commands;
mod native_package_commands;
mod runtime_and_release_commands;

use compiler_and_runtime_commands::run_compiler_and_runtime_command;
use native_package_commands::run_native_package_command;
use runtime_and_release_commands::run_runtime_and_release_command;

/// Runs repository quality checks from the command line.
///
/// Inputs:
/// - First positional argument naming the quality check.
/// - Optional `--write-baseline` for `rust-docs`.
/// - Current working directory as the repository root.
///
/// Output:
/// - Exit status 0 with a success summary when the check passes.
/// - Exit status 1/2 with stable diagnostics for check failures or bad usage.
///
/// Transformation:
/// - Routes permanent checks to Rust while keeping `terlc` free of maintenance commands.
pub fn run_from_env() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next();
    let command = command.as_deref();
    if let Some(result) = run_compiler_and_runtime_command(command, &mut args) {
        return result;
    }
    if let Some(result) = run_runtime_and_release_command(command, &mut args) {
        return result;
    }
    if let Some(result) = run_native_package_command(command, &mut args) {
        return result;
    }
    match command {
        Some(command) => {
            eprintln!("unsupported terlan-quality command: {command}");
            ExitCode::from(2)
        }
        None => {
            eprintln!(
                "usage: terlan-quality <rust-quality|rust-docs|module-readmes|cli-exact-selectors|test-hierarchy|internal-docs|oxc-boundary|erlang-backend-classification|vm-artifact-format|vm-native-worker-runtime|vm-io-reactor-runtime|vm-http-handler-scheduler-fairness|vm-http-benchmark-comparability|vm-http-runtime-attribution|vm-http-stateful-actor-session|vm-http-acme-worker|vm-http-acme-cache-custody|vm-http-acme-renewal|vm-live-template-stream|vm-live-template-client-protocol|typed-template-render-mode|web-asset-pipeline|vm-web-security-policy|vm-web-config-secret-boundary|vm-web-observability|vm-web-lifecycle-health|vm-web-deployment-profile|vm-web-route-schema-client|vm-model-sync-store|vm-persistent-actor-store|vm-persistent-actor-schema|vm-persistent-actor-compaction|vm-persistent-actor-restore|vm-persistent-actor-adapter|vm-persistent-actor-performance|vm-persistent-actor-telemetry|vm-persistent-actor-policy|vm-otp-abstractions-terlan-stdlib|vm-ownership-classification|vm-runtime-concept-inventory|vm-dev-dependency-orchestration|vm-db-migration-command|vm-sql-macro-validation|vm-diagnostics-quality|vm-deterministic-hashmap|vm-multicore-invariant-inventory|otp-reference-inventory|otp-test-pipeline-inventory|otp-runtime-exit|std-test-honesty|std-package-coverage-100|language-feature-coverage-100|operator-coverage-100|js-type-emission-contract|rust-build-feature-shipping|pattern-matching-support|function-head-migration-diagnostic-policy|function-head-migration-lint|function-head-pattern-migration-assist|function-head-pattern-migration-benchmark|function-head-pattern-migration-docs|function-head-pattern-handoff|hex-target-metadata|device-target-planner|package-lockfile-contract|package-resolver-reproducibility|package-registry-publish|package-capability-contract|package-release-test-matrix|package-api-compatibility|package-cli-workflow|package-editor-integration|package-cache-integrity|package-workspace-graph|package-build-artifact-isolation|source-map-debug-info|compiler-incremental-cache|watch-mode-hot-reload|aot-developer-hot-reload|release-flake-detection|release-gate-shard-resume|release-gate-duration-budget|release-gate-report-schema|release-failure-reproduction|roadmap-gate-integrity|shape-implications|terlan-lint-style-profile|editor-code-action-auto-import-report|editor-completion-signature-report|editor-definition-navigation-report|terlan-vm-internal-crate|terlan-vm-external-repo-boundary|no-terlan-vm-erts-rust-dependency|native-boundary-terminology|native-boundary-security|native-binding-generator-contract|cuda-package-availability|cuda-package-check|vm-release-install-validation|executable-docs-vm|lean-proof-feature-cull|no-default-tokio-runtime>"
            );
            ExitCode::from(2)
        }
    }
}
