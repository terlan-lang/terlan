mod abi1_pre_freeze;
mod abi1_release;
mod achamp_adversarial_coverage;
mod aot_developer_hot_reload;
mod binary_descriptor_contract;
mod cli;
mod cli_exact_selectors;
mod compiler_incremental_cache;
mod core_typing_spec;
mod cuda_package_availability;
mod dev_fast_feedback_profile;
mod device_target_planner;
mod dormant_runtime_code;
mod editor_code_action_auto_import_report;
mod editor_completion_signature_report;
mod editor_definition_navigation_report;
mod editor_report_selector;
mod erlang_backend_classification;
mod executable_docs_vm;
mod function_head_migration_diagnostic_policy;
mod function_head_migration_lint;
mod function_head_pattern_handoff;
mod function_head_pattern_migration_assist;
mod function_head_pattern_migration_benchmark;
mod function_head_pattern_migration_docs;
mod hex_target_metadata;
mod inline_tests;
mod internal_docs;
mod js_type_emission_contract;
mod language_feature_full_coverage;
mod lean_proof_closeout;
mod lean_proof_feature_cull;
mod lean_proof_pr;
mod lean_proof_regression;
mod lean_proof_runtime;
pub(crate) mod lean_proof_track;
mod makefile_list;
mod module_readmes;
mod multicore_invariant_inventory;
mod native_binding_generator_contract;
mod native_boundary_security;
mod native_boundary_terminology;
mod native_no_std_target_feasibility;
mod no_default_tokio_runtime;
mod no_implicit_otp_runtime;
mod no_terlan_vm_erts_rust_dependency;
mod operator_full_coverage;
mod otp_reference_inventory;
mod otp_runtime_exit;
mod otp_test_pipeline_inventory;
mod oxc_boundary;
mod package_api_compatibility;
mod package_build_artifact_isolation;
mod package_cache_integrity;
mod package_capability_contract;
mod package_cli_workflow;
mod package_editor_integration;
mod package_git_source;
mod package_lockfile_contract;
mod package_registry_publish;
mod package_release_test_matrix;
mod package_resolver_reproducibility;
mod package_workspace_graph;
mod pattern_matching_support;
mod placeholder_terms;
mod release_code_hygiene;
mod release_failure_reproduction;
mod release_flake_detection;
mod release_gate_duration_budget;
mod release_gate_report_schema;
mod release_gate_shard_resume;
mod rust_build_feature_shipping;
mod rust_quality;
mod rustdoc_analysis;
mod source_map_debug_info;
mod std_generated_metadata;
mod std_package_full_coverage;
mod std_source_naming;
mod std_test_honesty;
mod support;
mod terlan_lint_style_profile;
mod terlan_vm_external_repo_boundary;
mod terlan_vm_internal_crate;
mod test_hierarchy;
mod typed_template_render_mode;
mod vm_artifact_format;
mod vm_db_migration_command;
mod vm_deterministic_hashmap;
mod vm_dev_dependency_orchestration;
mod vm_diagnostics_quality;
mod vm_http_acme_cache_custody;
mod vm_http_acme_renewal;
mod vm_http_acme_worker;
pub(crate) mod vm_http_benchmark_contract;
mod vm_http_handler_scheduler_fairness;
mod vm_http_stateful_actor_session;
mod vm_io_reactor_runtime;
mod vm_live_template_client_protocol;
mod vm_live_template_stream;
mod vm_model_sync_store;
mod vm_native_worker_runtime;
mod vm_otp_abstractions_terlan_stdlib;
mod vm_ownership_classification;
mod vm_persistent_actor_adapter;
mod vm_persistent_actor_compaction;
mod vm_persistent_actor_performance;
mod vm_persistent_actor_policy;
mod vm_persistent_actor_restore;
mod vm_persistent_actor_schema;
mod vm_persistent_actor_store;
mod vm_persistent_actor_telemetry;
mod vm_release_install_validation;
mod vm_runtime_concept_inventory;
mod vm_sql_macro_validation;
mod vm_supervision_restart;
mod vm_web_config_secret_boundary;
mod vm_web_deployment_profile;
mod vm_web_lifecycle_health;
mod vm_web_observability;
mod vm_web_route_schema_client;
mod vm_web_security_policy;
mod watch_mode_hot_reload;
mod web_asset_pipeline;

pub use abi1_pre_freeze::*;
pub use abi1_release::*;
pub use cli::run_from_env;
pub use rust_quality::*;
pub(crate) use rustdoc_analysis::render_failure;

/// Runs the native-target feasibility command from the workspace root.
pub fn run_native_target_feasibility_from_workspace() -> std::process::ExitCode {
    match native_no_std_target_feasibility::run_native_no_std_target_feasibility(
        std::path::Path::new("."),
    ) {
        Ok(summary) => {
            println!(
                "[native-no-std-target-feasibility] {} targets, {} features, {} rejected features, and {} adversarial cases checked; report written to {}.",
                summary.target_count,
                summary.feature_count,
                summary.rejected_feature_count,
                summary.adversarial_case_count,
                summary.report_path.display()
            );
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::from(1)
        }
    }
}

/// Runs the Lean proof closeout command from the workspace root.
pub fn run_lean_proof_closeout_from_workspace() -> std::process::ExitCode {
    match lean_proof_closeout::run_lean_proof_closeout(std::path::Path::new(".")) {
        Ok(summary) => {
            println!(
                "[lean-proof-closeout] {} families, {} hard lanes, and {} baseline classes verified; baseline {}; gate report {}.",
                summary.family_count,
                summary.lane_count,
                summary.baseline_count,
                summary.baseline_hash,
                summary.gate_report.display()
            );
            std::process::ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::from(1)
        }
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
#[cfg(test)]
mod lib_test;
