fn run_command_group_002(
    command: Option<&str>,
    args: &mut impl Iterator<Item = String>,
) -> Option<ExitCode> {
    let _ = &mut *args;
    let result = match command {
        Some("vm-persistent-actor-store") => match run_vm_persistent_actor_store(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-persistent-actor-store] {} adapters, {} snapshot/event fixtures, {} replay traces, {} rejected persistent actor paths checked; report written to {}.",
                    summary.adapter_matrix_count,
                    summary.snapshot_event_fixture_count,
                    summary.replay_trace_count,
                    summary.rejected_persistent_actor_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-persistent-actor-schema") => {
            match run_vm_persistent_actor_schema(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-schema] {} schema ids, {} migration graph cases, {} compatibility rows, {} rejected migration cases checked; report written to {}.",
                        summary.schema_id_count,
                        summary.migration_graph_case_count,
                        summary.compatibility_matrix_count,
                        summary.rejected_migration_case_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-compaction") => {
            match run_vm_persistent_actor_compaction(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-compaction] {} store-size cases, {} replay traces, {} retained ranges, {} rejected retention policies, {} crash cases, {} resource cleanup decisions checked; report written to {}.",
                        summary.before_after_store_size_count,
                        summary.replay_equivalence_trace_count,
                        summary.retained_range_count,
                        summary.rejected_retention_policy_count,
                        summary.crash_injection_case_count,
                        summary.resource_cleanup_decision_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-restore") => {
            match run_vm_persistent_actor_restore(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-restore] {} export manifests, {} redaction decisions, {} restore traces, {} rejected restore cases, {} cross-adapter results checked; report written to {}.",
                        summary.export_manifest_count,
                        summary.redaction_decision_count,
                        summary.restore_validation_trace_count,
                        summary.rejected_restore_case_count,
                        summary.cross_adapter_restore_result_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-adapter") => {
            match run_vm_persistent_actor_adapter(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-adapter] {} capability manifests, {} conformance rows, {} crash outcomes, {} rejected adapters checked; report written to {}.",
                        summary.adapter_capability_manifest_count,
                        summary.conformance_matrix_count,
                        summary.crash_injection_outcome_count,
                        summary.rejected_adapter_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-performance") => {
            match run_vm_persistent_actor_performance(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-performance] {} fixture budgets, {} deterministic baselines, {} timing breakdowns, {} adversarial cases, {} rejected budget paths checked; report written to {}.",
                        summary.fixture_budget_count,
                        summary.deterministic_baseline_estimate_count,
                        summary.timing_breakdown_count,
                        summary.adversarial_performance_case_count,
                        summary.rejected_budget_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-telemetry") => {
            match run_vm_persistent_actor_telemetry(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-telemetry] {} trace fixtures, {} span fields, {} deterministic trace validations, {} replay timeline steps, {} rejected telemetry paths checked; report written to {}.",
                        summary.trace_fixture_count,
                        summary.span_schema_field_count,
                        summary.deterministic_trace_validation_count,
                        summary.replay_timeline_count,
                        summary.rejected_telemetry_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-persistent-actor-policy") => {
            match run_vm_persistent_actor_policy(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-persistent-actor-policy] {} roles, {} operations, {} deterministic decisions, {} adversarial cases, {} rejected policy paths checked; report written to {}.",
                        summary.role_count,
                        summary.operation_count,
                        summary.deterministic_policy_decision_count,
                        summary.adversarial_policy_case_count,
                        summary.rejected_policy_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-http-acme-worker") => match run_vm_http_acme_worker(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-http-acme-worker] {} worker traces, {} challenge-routing traces, {} typed diagnostics, {} rejected worker paths checked; report written to {}.",
                    summary.worker_state_trace_count,
                    summary.challenge_routing_trace_count,
                    summary.typed_diagnostic_fixture_count,
                    summary.rejected_worker_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-http-acme-cache-custody") => {
            match run_vm_http_acme_cache_custody(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-http-acme-cache-custody] {} manifest fields, {} custody decisions, {} rejected fixtures, {} rejected custody paths checked; report written to {}.",
                        summary.cache_manifest_field_count,
                        summary.key_custody_decision_count,
                        summary.rejected_cache_fixture_count,
                        summary.rejected_custody_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-http-acme-renewal") => match run_vm_http_acme_renewal(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-http-acme-renewal] {} schedules, {} timer traces, {} TLS handoff events, {} rejected renewal paths checked; report written to {}.",
                    summary.renewal_schedule_count,
                    summary.timer_trace_count,
                    summary.tls_handoff_event_count,
                    summary.rejected_renewal_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-ownership-classification") => {
            match run_vm_ownership_classification(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-ownership-classification] {} entries classified: compiler-owned={}, vm-owned={}, boundary-owned={}, reference-only={}, out-of-contract={}.",
                        summary.inventory_count,
                        summary.compiler_owned_count,
                        summary.vm_owned_count,
                        summary.boundary_owned_count,
                        summary.reference_only_count,
                        summary.out_of_contract_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-runtime-concept-inventory") => {
            match run_vm_runtime_concept_inventory(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-runtime-concept-inventory] {} concepts classified: required-vm-semantics={}, library-abstraction={}, distribution-machinery={}, unsupported-otp-compatibility={}.",
                        summary.concept_count,
                        summary.required_vm_semantics_count,
                        summary.library_abstraction_count,
                        summary.distribution_machinery_count,
                        summary.unsupported_otp_compatibility_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-sql-macro-validation") => match run_vm_sql_macro_validation(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-sql-macro-validation] sqlparser {}, {} diagnostics, contract {}; report written to {}.",
                    summary.parser_version,
                    summary.diagnostic_count,
                    summary.validation_contract_fingerprint,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-db-migration-command") => match run_vm_db_migration_command(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-db-migration-command] {} migration fixtures and {} diagnostics validated, contract {}; report written to {}.",
                    summary.migration_count,
                    summary.diagnostic_count,
                    summary.contract_fingerprint,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-dev-dependency-orchestration") => {
            match run_vm_dev_dependency_orchestration(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-dev-dependency-orchestration] {} commands and {} diagnostics validated, contract {}; report written to {}.",
                        summary.command_count,
                        summary.diagnostic_count,
                        summary.contract_fingerprint,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-diagnostics-quality") => match run_vm_diagnostics_quality(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-diagnostics-quality] {} contract terms and {} exact selectors enforced.",
                    summary.required_contract_term_count, summary.exact_selector_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("no-implicit-otp-runtime") => match run_no_implicit_otp_runtime(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[no-implicit-otp-runtime] {} runtime selection markers and {} forbidden fragments enforced.",
                    summary.rule_count, summary.forbidden_fragment_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("no-default-tokio-runtime") => match run_no_default_tokio_runtime(Path::new(".")) {
            Ok(summary) => {
                let direct_dependencies = if summary.direct_tokio_dependencies.is_empty() {
                    "none".to_string()
                } else {
                    summary.direct_tokio_dependencies.join(", ")
                };
                println!(
                    "[no-default-tokio-runtime] {} Tokio references and {} direct Tokio dependency entries classified by {} inventory rows. Direct entries: {}.",
                    summary.scanned_reference_count,
                    summary.direct_tokio_dependency_count,
                    summary.inventory_row_count,
                    direct_dependencies
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-otp-abstractions-terlan-stdlib") => {
            match run_vm_otp_abstractions_terlan_stdlib(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-otp-abstractions-terlan-stdlib] {} behavior modules checked; {} policy docs checked; {} pending framework intrinsics inventoried; {} direct runtime magic keys found.",
                        summary.behavior_module_count,
                        summary.policy_doc_count,
                        summary.pending_framework_intrinsic_count,
                        summary.runtime_magic_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("otp-reference-inventory") => match run_otp_reference_inventory(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[otp-reference-inventory] {} entries: mined={}, pending={}, rejected={}.",
                    summary.entry_count,
                    summary.mined_count,
                    summary.pending_count,
                    summary.rejected_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("otp-test-pipeline-inventory") => {
            match run_otp_test_pipeline_inventory(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[otp-test-pipeline-inventory] {} inventory rows cover {} selected surfaces.",
                        summary.inventory_row_count, summary.scanned_surface_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("hex-target-metadata") => match run_hex_target_metadata(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[hex-target-metadata] {} package metadata contract terms enforced.",
                    summary.required_term_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("device-target-planner") => match run_device_target_planner(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[device-target-planner] {} profiles, {} plan hashes, {} rejected features, {} diagnostics, {} future lowering prerequisites checked; report written to {}.",
                    summary.profile_count,
                    summary.plan_hash_count,
                    summary.rejected_feature_count,
                    summary.diagnostic_count,
                    summary.future_lowering_prerequisite_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-git-source") => match run_package_git_source(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-git-source] {} Git source contract terms and {} required fields enforced.",
                    summary.required_term_count, summary.required_field_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-lockfile-contract") => match run_package_lockfile_contract(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-lockfile-contract] {} lockfile contract terms and {} required fields enforced.",
                    summary.required_term_count, summary.required_field_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-resolver-reproducibility") => {
            match run_package_resolver_reproducibility(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[package-resolver-reproducibility] lockfile {} terms/{} fields and Git source {} terms/{} fields enforced; report written to {}.",
                        summary.lockfile_term_count,
                        summary.lockfile_field_count,
                        summary.git_source_term_count,
                        summary.git_source_field_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("package-registry-publish") => match run_package_registry_publish(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-registry-publish] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-capability-contract") => {
            match run_package_capability_contract(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[package-capability-contract] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("package-release-test-matrix") => {
            match run_package_release_test_matrix(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[package-release-test-matrix] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("package-api-compatibility") => match run_package_api_compatibility(Path::new(".")) {
            Ok(summary) => {
                println!(
                        "[package-api-compatibility] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-cli-workflow") => match run_package_cli_workflow(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-cli-workflow] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-editor-integration") => {
            match run_package_editor_integration(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[package-editor-integration] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("package-cache-integrity") => match run_package_cache_integrity(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-cache-integrity] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-workspace-graph") => match run_package_workspace_graph(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[package-workspace-graph] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("package-build-artifact-isolation") => {
            match run_package_build_artifact_isolation(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[package-build-artifact-isolation] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("source-map-debug-info") => match run_source_map_debug_info(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[source-map-debug-info] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("compiler-incremental-cache") => {
            match run_compiler_incremental_cache(Path::new(".")) {
                Ok(summary) => {
                    println!(
                    "[compiler-incremental-cache] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("watch-mode-hot-reload") => match run_watch_mode_hot_reload(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[watch-mode-hot-reload] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("release-flake-detection") => match run_release_flake_detection(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[release-flake-detection] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("release-gate-shard-resume") => match run_release_gate_shard_resume(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[release-gate-shard-resume] {} required terms and {} forbidden claims enforced; report written to {}.",
                    summary.required_term_count,
                    summary.forbidden_claim_count,
                    summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("release-gate-duration-budget") => {
            match run_release_gate_duration_budget(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[release-gate-duration-budget] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("release-gate-report-schema") => {
            match run_release_gate_report_schema(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[release-gate-report-schema] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("release-failure-reproduction") => {
            match run_release_failure_reproduction(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[release-failure-reproduction] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("release-code-hygiene") => match run_release_code_hygiene(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[release-code-hygiene] {} sub-gates and {} roadmap terms validated; report written to {}.",
                    summary.sub_gate_count, summary.roadmap_term_count, summary.report_path
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("roadmap-gate-integrity") => match run_roadmap_gate_integrity(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[roadmap-gate-integrity] {} planned gates checked across {} unchecked slices and {} Make targets.",
                    summary.planned_gate_count,
                    summary.unchecked_slice_count,
                    summary.make_target_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("shape-implications") => match run_shape_implications(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[shape-implications] {} requirement terms and {} acceptance terms checked.",
                    summary.required_term_count, summary.acceptance_term_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("terlan-lint-style-profile") => match run_terlan_lint_style_profile(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[terlan-lint-style-profile] {} rule families and {} seed rule IDs checked.",
                    summary.family_count, summary.rule_id_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("terlan-vm-internal-crate") => match run_terlan_vm_internal_crate(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[terlan-vm-internal-crate] {} repository shape files checked.",
                    summary.checked_file_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("terlan-vm-external-repo-boundary") => {
            match run_terlan_vm_external_repo_boundary(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[terlan-vm-external-repo-boundary] {} files checked; {} files contain allowed external VM references.",
                        summary.checked_file_count, summary.reference_file_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("no-terlan-vm-erts-rust-dependency") => {
            match run_no_terlan_vm_erts_rust_dependency(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[no-terlan-vm-erts-rust-dependency] {} retired gates quarantined across {} default targets; {} retained inventory rows checked.",
                        summary.retired_gate_count,
                        summary.checked_default_target_count,
                        summary.retained_inventory_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("otp-runtime-exit") => match run_otp_runtime_exit(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[otp-runtime-exit] {} required terms, {} removal lanes, and {} closeout blockers checked.",
                    summary.required_term_count,
                    summary.removal_lane_count,
                    summary.closeout_blocker_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        _ => return None,
    };
    Some(result)
}
