fn run_command_group_001(
    command: Option<&str>,
    args: &mut impl Iterator<Item = String>,
) -> Option<ExitCode> {
    let _ = &mut *args;
    let result = match command {
        Some("rust-quality") => match run_rust_quality(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[rust-quality] baseline enforced: {} oversized files, {} inline-test files.",
                    summary.oversized_count, summary.inline_test_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("dormant-runtime-code") => match run_dormant_runtime_code(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[dormant-runtime-code] {} dormant VM modules classified across {} inventory rows.",
                    summary.dormant_module_count, summary.inventory_row_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-deterministic-hashmap") => match run_vm_deterministic_hashmap(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-deterministic-hashmap] {} VM HashMap/RandomState references classified across {} inventory rows.",
                    summary.scanned_reference_count, summary.inventory_row_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("editor-completion-signature-report") => {
            match run_editor_completion_signature_report(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[editor-completion-signature-report] {} selectors across {} report categories; report written to {}.",
                        summary.selector_count,
                        summary.category_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("editor-code-action-auto-import-report") => {
            match run_editor_code_action_auto_import_report(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[editor-code-action-auto-import-report] {} fixtures across {} report categories; report written to {}.",
                        summary.fixture_count,
                        summary.category_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("editor-definition-navigation-report") => {
            match run_editor_definition_navigation_report(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[editor-definition-navigation-report] {} selectors across {} report categories; report written to {}.",
                        summary.selector_count,
                        summary.category_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("rust-docs") => {
            let write_baseline = args.any(|arg| arg == "--write-baseline");
            if write_baseline {
                match write_rustdoc_baseline(Path::new(".")) {
                    Ok(count) => {
                        println!("[rustdoc] wrote baseline with {count} undocumented items.");
                        ExitCode::SUCCESS
                    }
                    Err(message) => failure(message),
                }
            } else {
                match run_rustdoc(Path::new(".")) {
                    Ok(summary) => {
                        println!(
                            "[rustdoc] baseline enforced: {} undocumented items.",
                            summary.undocumented_count
                        );
                        ExitCode::SUCCESS
                    }
                    Err(message) => failure(message),
                }
            }
        }
        Some("module-readmes") => match run_module_readmes(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[module-readmes] baseline enforced: {} missing README files.",
                    summary.missing_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("cli-exact-selectors") => match run_cli_exact_selectors(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[cli-exact-selector] {} exact selectors resolve.",
                    summary.selector_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("core-typing-spec") => match run_core_typing_spec(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[core-typing-spec] {} core typing forms classified.",
                    summary.classified_form_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("binary-descriptor-contract") => {
            match run_binary_descriptor_contract(Path::new(".")) {
                Ok(summary) => {
                    println!(
                    "[binary-descriptor-contract] {} descriptors, {} unsupported-runtime tests, {} coverage inventories checked.",
                    summary.descriptor_count,
                    summary.unsupported_runtime_test_count,
                    summary.coverage_inventory_count
                );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("lean-proof-track") => match run_lean_proof_track(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-track] {} inventory rows, {} proof-gap rows, {} Lean files checked.",
                    summary.inventory_row_count, summary.gap_row_count, summary.lean_file_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("lean-proof-gap-hygiene") => match run_lean_proof_gap_hygiene(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-gap-hygiene] {} gaps checked against {} current proof features, {} executable follow-up gates, {} closure notes, and {} lifecycle transitions.",
                    summary.gap_count,
                    summary.current_proof_feature_count,
                    summary.follow_up_gate_count,
                    summary.closure_note_count,
                    summary.transition_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("lean-proof-feature-cull") => match run_lean_proof_feature_cull(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-feature-cull] {} removed features, {} rejection theorems, and {} forbidden aliases checked.",
                    summary.removed_feature_count,
                    summary.rejection_theorem_count,
                    summary.forbidden_alias_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("lean-proof-regression") => match run_lean_proof_regression(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-regression] {} feature classes checked, {} warnings; report written to {}.",
                    summary.feature_class_count,
                    summary.warning_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("lean-proof-runtime") => match run_lean_proof_runtime(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-runtime] {} scheduling groups checked; report written to {}.",
                    summary.group_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("lean-proof-pr") => match run_lean_proof_pr(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[lean-proof-pr] {} owner rows, {} unresolved gaps; report written to {}.",
                    summary.owner_count,
                    summary.unresolved_gap_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("test-hierarchy") => match run_test_hierarchy(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[test-hierarchy] {} Makefile script gates are release-owned.",
                    summary.invocation_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("dev-fast-feedback-profile") => match run_dev_fast_feedback_profile(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[dev-fast-feedback-profile] {} profiles, {} gate mappings; report written to {}.",
                    summary.profile_count,
                    summary.mapping_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("std-source-naming") => match run_std_source_naming(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-source-naming] {} hand-authored std sources match module filenames.",
                    summary.checked_source_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("std-generated-metadata") => match run_std_generated_metadata(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-generated-metadata] {} generated std artifacts have complete provenance.",
                    summary.checked_file_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("std-test-honesty") => match run_std_test_honesty(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-test-honesty] {} tests checked across {} std test files.",
                    summary.checked_test_count, summary.checked_file_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("std-package-coverage-100") => match run_std_package_coverage_100(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[std-package-coverage-100] {} release API rows checked: {} executable tests, {} generated contracts; {} release modules checked with {} baseline gaps.",
                    summary.api_row_count,
                    summary.executable_test_row_count,
                    summary.generated_contract_row_count,
                    summary.release_module_count,
                    summary.uncovered_module_baseline_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("achamp-adversarial-coverage") => {
            match run_achamp_adversarial_coverage(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[achamp-adversarial-coverage] {} layout tests, {} active map tests, {} node variants, {} map operations, {} source fragments, {} randomized-backend guards checked.",
                        summary.layout_test_count,
                        summary.value_test_count,
                        summary.node_variant_count,
                        summary.map_operation_count,
                        summary.source_fragment_count,
                        summary.randomized_backend_guard_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("js-type-emission-contract") => match run_js_type_emission_contract(Path::new(".")) {
            Ok(summary) => {
                println!(
                        "[js-type-emission-contract] {} mapping categories, {} generated outputs, {} skipped declarations checked.",
                        summary.mapping_category_count,
                        summary.generated_output_count,
                        summary.skipped_declaration_count
                    );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("rust-build-feature-shipping") => {
            match run_rust_build_feature_shipping(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[rust-build-feature-shipping] {} features, {} release profiles, {} metadata fields checked.",
                        summary.classified_feature_count,
                        summary.release_profile_count,
                        summary.release_metadata_field_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("mobile-boundary") => match run_mobile_boundary(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[mobile-boundary] {} mobile files, {} compiler files, {} allowed hooks checked.",
                    summary.mobile_file_count,
                    summary.compiler_file_count,
                    summary.allowed_hook_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("pattern-matching-support") => match run_pattern_matching_support(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[pattern-matching-support] {} families, {} long-tail contexts, {} shape-synonym contexts, {} positive test anchors, {} adversarial references checked.",
                    summary.family_count,
                    summary.long_tail_context_count,
                    summary.shape_synonym_context_count,
                    summary.positive_test_count,
                    summary.adversarial_test_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("function-head-migration-diagnostic-policy") => {
            match run_function_head_migration_diagnostic_policy(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-migration-diagnostic-policy] {} required terms and {} forbidden claims enforced; report written to {}.",
                        summary.required_term_count,
                        summary.forbidden_claim_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("function-head-migration-lint") => {
            match run_function_head_migration_lint(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-migration-lint] {} migration rows, {} parser anchors, and {} Make targets checked; manifest written to {}.",
                        summary.migration_row_count,
                        summary.parser_anchor_count,
                        summary.make_target_count,
                        summary.manifest_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("function-head-pattern-migration-assist") => {
            match run_function_head_pattern_migration_assist(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-pattern-migration-assist] {} terms, {} tests, and {} Make targets checked; report written to {}.",
                        summary.required_term_count,
                        summary.required_test_count,
                        summary.make_target_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("function-head-pattern-migration-docs") => {
            match run_function_head_pattern_migration_docs(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-pattern-migration-docs] {} migration IDs and {} required terms enforced; report written to {}.",
                        summary.migration_id_count,
                        summary.required_term_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("function-head-pattern-migration-benchmark") => {
            match run_function_head_pattern_migration_benchmark(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-pattern-migration-benchmark] {} scenarios and {} tracked metrics checked; report written to {}.",
                        summary.scenario_count,
                        summary.metric_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("function-head-pattern-handoff") => {
            match run_function_head_pattern_handoff(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[function-head-pattern-handoff] {} gates and {} closure rows recorded; report written to {}.",
                        summary.required_gate_count,
                        summary.closure_matrix_row_count,
                        summary.report_path
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("operator-coverage-100") => match run_operator_coverage_100(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[operator-coverage-100] {} operators, {} positive tests, {} adversarial references, {} source fragments checked.",
                    summary.operator_count,
                    summary.positive_test_count,
                    summary.adversarial_reference_count,
                    summary.source_fragment_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("language-feature-coverage-100") => {
            match run_language_feature_coverage_100(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[language-feature-coverage-100] {} features, {} positive tests, {} adversarial references, {} source fragments checked.",
                        summary.feature_count,
                        summary.positive_test_count,
                        summary.adversarial_reference_count,
                        summary.source_fragment_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("internal-docs") => match run_internal_docs(Path::new(".")) {
            Ok(_) => {
                println!("[internal-docs] published docs contain no roadmap or scratch packets.");
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("oxc-boundary") => match run_oxc_boundary(Path::new(".")) {
            Ok(_) => {
                println!(
                    "[oxc-boundary] Oxc is confined to JS backend and binding-generator ownership."
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("erlang-backend-classification") => {
            match run_erlang_backend_classification(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[erlang-backend-classification] {} paths classified: remove={}, reference-only={}, temporary-bridge={}, historical={}.",
                        summary.classified_count,
                        summary.remove_count,
                        summary.reference_only_count,
                        summary.temporary_bridge_count,
                        summary.historical_artifact_count
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-artifact-format") => match run_vm_artifact_format(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-artifact-format] {} artifact contract groups enforced.",
                    summary.required_group_count
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-native-worker-runtime") => match run_vm_native_worker_runtime(Path::new(".")) {
            Ok(summary) => {
                println!(
                        "[vm-native-worker-runtime] {} worker policies, {} trace cases, {} rejected runtime paths, {} canonical Rust tests checked; report written to {}.",
                        summary.policy_count,
                        summary.trace_case_count,
                        summary.rejected_runtime_count,
                        summary.canonical_rust_test_count,
                        summary.report_path.display()
                    );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-io-reactor-runtime") => match run_vm_io_reactor_runtime(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-io-reactor-runtime] {} reactor fixtures, {} rejected runtime paths, {} exact selectors checked; report written to {}.",
                    summary.fixture_count,
                    summary.rejected_runtime_count,
                    summary.exact_selector_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-supervision-restart") => match run_vm_supervision_restart(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-supervision-restart] {} supervision fixtures, {} open restart gaps, {} exact selectors checked; report written to {}.",
                    summary.fixture_count,
                    summary.open_gap_count,
                    summary.exact_selector_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-http-handler-scheduler-fairness") => {
            match run_vm_http_handler_scheduler_fairness(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-http-handler-scheduler-fairness] {} fairness fixtures, {} rejected fairness paths, {} benchmark commands, {} exact selectors checked; report written to {}.",
                        summary.fixture_count,
                        summary.rejected_fairness_count,
                        summary.benchmark_command_count,
                        summary.exact_selector_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-http-benchmark-comparability") => {
            match run_vm_http_benchmark_comparability(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-http-benchmark-comparability] {} concurrency levels and {} adversarial scenarios checked; profile fingerprint {}; report written to {}.",
                        summary.concurrency_count,
                        summary.scenario_count,
                        summary.profile_fingerprint,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-http-runtime-attribution") => {
            match run_vm_http_runtime_attribution_contract(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-http-runtime-attribution] {} telemetry buckets and {} accounting invariants checked; report written to {}.",
                        summary.bucket_count,
                        summary.invariant_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-http-stateful-actor-session") => {
            match run_vm_http_stateful_actor_session(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-http-stateful-actor-session] {} affinity fixtures, {} lifecycle traces, {} rejected session paths, {} exact selectors checked; report written to {}.",
                        summary.affinity_fixture_count,
                        summary.lifecycle_trace_count,
                        summary.rejected_session_path_count,
                        summary.exact_selector_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-live-template-stream") => match run_vm_live_template_stream(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-live-template-stream] {} template fixtures, {} patch event classes, {} rejected stream paths checked; report written to {}.",
                    summary.template_fixture_count,
                    summary.patch_event_count,
                    summary.rejected_stream_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-live-template-client-protocol") => {
            match run_vm_live_template_client_protocol(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-live-template-client-protocol] {} protocol events, {} payload validation cases, {} compatibility cases, {} rejected protocol paths checked; report written to {}.",
                        summary.protocol_event_count,
                        summary.payload_validation_case_count,
                        summary.compatibility_case_count,
                        summary.rejected_protocol_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("typed-template-render-mode") => {
            match run_typed_template_render_mode(Path::new(".")) {
                Ok(summary) => {
                    println!(
                    "[typed-template-render-mode] {} render modes, {} implemented modes, {} escaping checks, {} rejected mode combinations checked; report written to {}.",
                    summary.render_mode_count,
                    summary.implemented_mode_count,
                    summary.escaping_check_count,
                    summary.rejected_mode_combination_count,
                    summary.report_path.display()
                );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("web-asset-pipeline") => match run_web_asset_pipeline(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[web-asset-pipeline] {} asset graph entries, {} content-type checks, {} cache-header checks, {} rejected asset paths checked; report written to {}.",
                    summary.asset_graph_entry_count,
                    summary.content_type_check_count,
                    summary.cache_header_check_count,
                    summary.rejected_asset_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-web-security-policy") => match run_vm_web_security_policy(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-web-security-policy] {} route policy rows, {} rejected request fixtures, {} rejected policy paths checked; report written to {}.",
                    summary.route_policy_count,
                    summary.rejected_request_fixture_count,
                    summary.rejected_policy_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-web-config-secret-boundary") => {
            match run_vm_web_config_secret_boundary(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-web-config-secret-boundary] {} config schemas, {} rejected configs, {} redaction checks, {} secret usage checks, {} rejected secret paths checked; report written to {}.",
                        summary.config_schema_count,
                        summary.rejected_config_count,
                        summary.redaction_check_count,
                        summary.secret_usage_check_count,
                        summary.rejected_secret_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-web-observability") => match run_vm_web_observability(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-web-observability] {} telemetry fields, {} route traces, {} stream traces, {} rejected observability paths checked; report written to {}.",
                    summary.telemetry_field_count,
                    summary.route_trace_count,
                    summary.stream_trace_count,
                    summary.rejected_observability_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-web-lifecycle-health") => match run_vm_web_lifecycle_health(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-web-lifecycle-health] {} lifecycle transitions, {} health endpoint fixtures, {} drain traces, {} rejected lifecycle paths checked; report written to {}.",
                    summary.lifecycle_state_transition_count,
                    summary.health_endpoint_fixture_count,
                    summary.drain_trace_count,
                    summary.rejected_lifecycle_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-web-deployment-profile") => match run_vm_web_deployment_profile(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-web-deployment-profile] {} deployment profiles, {} proxy fixtures, {} upgrade cases, {} rejected deployment paths checked; report written to {}.",
                    summary.profile_matrix_count,
                    summary.proxy_fixture_count,
                    summary.upgrade_case_count,
                    summary.rejected_deployment_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        Some("vm-web-route-schema-client") => {
            match run_vm_web_route_schema_client(Path::new(".")) {
                Ok(summary) => {
                    println!(
                        "[vm-web-route-schema-client] {} route manifest hash cases, {} schema output cases, {} client fixtures, {} rejected schema/client paths checked; report written to {}.",
                        summary.route_manifest_hash_case_count,
                        summary.schema_output_case_count,
                        summary.generated_client_fixture_count,
                        summary.rejected_schema_client_path_count,
                        summary.report_path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(message) => failure(message),
            }
        }
        Some("vm-model-sync-store") => match run_vm_model_sync_store(Path::new(".")) {
            Ok(summary) => {
                println!(
                    "[vm-model-sync-store] {} model fixtures, {} adapters, {} version/conflict cases, {} rejected model-sync paths checked; report written to {}.",
                    summary.model_fixture_count,
                    summary.adapter_matrix_count,
                    summary.version_conflict_case_count,
                    summary.rejected_model_sync_path_count,
                    summary.report_path.display()
                );
                ExitCode::SUCCESS
            }
            Err(message) => failure(message),
        },
        _ => return None,
    };
    Some(result)
}
