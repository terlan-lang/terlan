# Terlan CLI compiler-path targets.
#
# This file is included by the root Makefile. Target names remain public from the
# repository root, but CLI-specific recipes live with the CLI crate.

TERLC := $(CARGO) run -p terlan_cli --
TERLC_EXACT_TEST := $(CARGO) test -p terlan_cli

.PHONY: cli-help cli-check cli-build cli-test cli-release-artifact-linux cli-clean typecheck-fixture emit-fixture smoke release-0-0-3-preflight release-0-0-3-installed-smoke polars-blackbox-contract-check formal-cli-phase-contract-gate formal-cli-build-gate formal-cli-js-gate formal-cli-rust-gate formal-cli-doc-gate formal-cli-a0-50-template-frontend-gate formal-cli-a0-54-constructor-contract-gate formal-cli-a0-55-function-clause-contract-gate formal-cli-a0-56-primary-expression-contract-gate formal-cli-a0-57-keyword-expression-contract-gate formal-cli-a0-58-calls-and-references-contract-gate formal-cli-a0-59-data-form-contract-gate formal-cli-a0-60-pattern-contract-gate formal-cli-a0-61-lexical-and-name-contract-gate formal-cli-a0-62-template-boundary-contract-gate formal-incremental-gate formal-phase-gate formal-directory-phase-gate

cli-help:
	@echo "  make typecheck-fixture - terlan check fixture"
	@echo "  make emit-fixture      - emit fixture .erl/.typi to $(OUT_DIR)"
	@echo "  make smoke             - emit + erlc + runtime smoke test"
	@echo "  make release-0-0-3-preflight - run current 0.0.3 CLI/docs/REPL release gate"
	@echo "  make release-0-0-3-installed-smoke - smoke-test installed terlc 0.0.3"
	@echo "  make polars-blackbox-contract-check - run public-CLI native Polars package boundary checks"
	@echo "  make formal-cli-phase-contract-gate - run CLI phase-contract golden/parity regressions"
	@echo "  make formal-cli-build-gate - run CLI build artifact/debug-map regressions"
	@echo "  make formal-cli-js-gate - run CLI JavaScript/Oxc output regressions"
	@echo "  make formal-cli-rust-gate - run CLI Rust/native neutrality probe regressions"
	@echo "  make formal-cli-doc-gate - run CLI formal documentation regressions"
	@echo "  make formal-cli-a0-50-template-frontend-gate - run A0.50 normalized template frontend input regression"
	@echo "  make formal-cli-a0-54-constructor-contract-gate - run A0.54 constructor contract regressions"
	@echo "  make formal-cli-a0-55-function-clause-contract-gate - run A0.55 function/clause contract regressions"
	@echo "  make formal-cli-a0-56-primary-expression-contract-gate - run A0.56 primary-expression contract regressions"
	@echo "  make formal-cli-a0-57-keyword-expression-contract-gate - run A0.57 keyword-expression contract regressions"
	@echo "  make formal-cli-a0-58-calls-and-references-contract-gate - run A0.58 calls-and-references contract regressions"
	@echo "  make formal-cli-a0-59-data-form-contract-gate - run A0.59 data-form contract regressions"
	@echo "  make formal-cli-a0-60-pattern-contract-gate - run A0.60 pattern contract regressions"
	@echo "  make formal-cli-a0-61-lexical-and-name-contract-gate - run A0.61 lexical/name contract regressions"
	@echo "  make formal-cli-a0-62-template-boundary-contract-gate - run A0.62 template boundary contract regressions"
	@echo "  make formal-incremental-gate - run CLI incremental dependency-closure regression"
	@echo "  make formal-phase-gate - run formal phase determinism regression gate"
	@echo "  make formal-directory-phase-gate - run deterministic directory-mode phase-manifest gate"

cli-check:
	$(CARGO) check --locked --workspace

cli-build:
	$(CARGO) build --locked --bin terlc

cli-test:
	$(CARGO) test --locked --workspace

cli-release-artifact-linux:
	$(CARGO) build --release --locked --bin terlc
	mkdir -p dist
	cp target/release/terlc dist/terlc
	tar -C dist -czf dist/terlc-linux-x86_64.tar.gz terlc

cli-clean:
	$(CARGO) clean
	rm -rf dist

typecheck-fixture:
	$(TERLC) check $(FIXTURE)

emit-fixture:
	mkdir -p $(OUT_DIR)
	$(TERLC) emit $(FIXTURE) --out-dir $(OUT_DIR)

smoke: emit-fixture
	erlc $(OUT_DIR)/mathx.erl
	erl -noshell -pa $(OUT_DIR) -eval 'io:format("~p~n", [mathx:add(41)]), halt().'

polars-blackbox-contract-check:
	bash scripts/check_0_0_3_polars_blackbox_contract.sh

release-0-0-3-installed-smoke:
	bash scripts/check_0_0_3_installed_smoke.sh

release-0-0-3-preflight:
	$(CARGO) fmt --all -- --check
	$(MAKE) --no-print-directory workspace-version-check
	$(MAKE) --no-print-directory source-extension-check
	$(CARGO) check -p terlan_cli
	$(CARGO) test -p terlan_syntax syntax_output_includes_sequence_primary_expr_trees
	bash scripts/check_0_0_3_polars_blackbox_contract.sh
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_rust_stub_contains_actor_bridge_contract -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_rust_stub_satisfies_validator -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_rust_stub_compiles_as_library -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_erl_stub_uses_neutral_loader_env_var -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_erl_stub_contains_worker_transport_contract -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_erl_stub_compiles_as_module -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::safe_native_erl_stub_metadata_runs_without_native_library -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::compiler_native_metadata_extracts_std_json_operations -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::compiler_native_metadata_extracts_all_rust_backed_std_operations -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::native_metadata_rejects_native_core_module_without_compiler_native_annotations -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::emit_native_artifacts_writes_safe_native_filenames -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::artifacts::tests::emit_native_artifacts_writes_compiler_native_std_files -- --exact
	$(TERLC_EXACT_TEST) commands::emit_native_metadata::tests::run_emits_compiler_native_std_json_artifacts -- --exact
	$(TERLC_EXACT_TEST) commands::emit::tests::run_emit_writes_compiler_native_std_artifacts -- --exact
	$(TERLC_EXACT_TEST) validation::native_policy::tests::compiler_native_annotation_requires_native_policy -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::parse_init_args_accepts_named_project -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::parse_init_args_rejects_missing_project_name -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::parse_init_args_rejects_invalid_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::render_manifest_uses_release_project_contract -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::write_project_creates_manifest_and_main_module -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::write_project_normalizes_hyphenated_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::init::tests::write_project_refuses_existing_directory -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_long_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_help_command -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_help_command_long_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_help_command_short_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_help_command_for_known_commands -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_help_command_after_global_options -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_release_command_help_after_global_options -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_help_after_global_options -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_version_after_global_options -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_command_local_help_for_known_commands -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_generic_command_local_help_after_global_options -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_rejects_help_command_for_unknown_command -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_rejects_help_command_extra_operands -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_long_version -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_top_level_short_version -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_version_command_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_rejects_version_command_extra_arguments -- --exact
	$(TERLC_EXACT_TEST) tests::top_level_help_does_not_consume_command_local_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_release_command_long_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_release_command_short_help -- --exact
	$(TERLC_EXACT_TEST) tests::command_local_help_accepts_repl_help -- --exact
	$(TERLC_EXACT_TEST) tests::run_cli_accepts_repl_short_help -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::parse_build_args_defaults_to_erlang_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::parse_build_args_defaults_to_current_directory -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_semicolon_expression_sequence_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_task_done_result_call -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_std_equal_trait_dispatch -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::parse_test_args_accepts_default_erlang_target -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::parse_test_args_defaults_to_tests_directory -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::parse_test_args_accepts_explicit_erlang_target -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::collect_test_files_finds_only_test_sources -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::release_support_modules_are_embedded_for_installed_runner -- --exact
	$(TERLC_EXACT_TEST) commands::test::tests::render_eunit_wrapper_source_delegates_to_target_tests -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_maps_type_of_to_intrinsic -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_maps_is_type_to_intrinsic -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_lowercase_canonical_boolean_literals -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_undeclared_uppercase_boolean_spellings -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_declared_uppercase_boolean_alias_value -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_uppercase_unit_value -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_lowercase_unit_as_builtin_value -- --exact
	$(CARGO) test -p terlan_syntax parser::tests::formal_atom_literal_expr_syntax_parses_canonical_atom_values -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_canonical_atom_literal_expr_source -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_atom_aliases_compare_with_canonical_atom_literal_values -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_canonical_unit_named_atom_literal -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_legacy_conversion_helpers_from_implicit_prelude -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_legacy_predicate_helpers_from_implicit_prelude -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_release_core_collection_contracts -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_invalid_annotation_schema_usage_before_core_phase -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_release_core_task_contract_usage -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::tests::embedded_std_interfaces_include_core_task_contract -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_std_core_task_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_task_operation_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::tests::embedded_std_interfaces_include_data_json_contract -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_rust_backed_json_std_module_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::tests::embedded_std_interfaces_include_web_data_utility_contracts -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_rust_backed_web_data_std_modules_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::tests::embedded_std_interfaces_include_beam_bridge_contracts -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_beam_std_module_for_core_v0_profile -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_beam_native_bridge_contract_for_core_v0_profile -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::rejects_all_beam_bridge_contract_modules_for_core_v0_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_beam_process_contract_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_beam_native_bridge_contract_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::accepts_beam_native_bridge_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_native_bridge_operation_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::accepts_beam_supervisor_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_supervisor_operation_before_backend_emission -- --exact
	$(CARGO) test -p terlan_hir tests::interface_rendering_preserves_trait_default_method_markers -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_imported_trait_impl_default_methods_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_std_beam_gen_server_without_default_terminate -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::accepts_beam_gen_server_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::accepts_beam_agent_get_and_update_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_agent_get_and_update_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_gen_server_default_terminate_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_gen_server_operation_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::tests::accepts_beam_task_operation_for_erlang_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_beam_task_operation_before_backend_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_beam_agent_start_get_update_cast_call -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_beam_task_start_result_cancel_call -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_beam_gen_server_start_call_cast_stop -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_beam_supervisor_child_spec_start_stop -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_imported_beam_native_bridge_start_call_dispose_stop -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_aliased_core_and_beam_task_receiver_calls -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_infers_index_read_through_index_get_trait -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_index_expr_uses_index_get_call -- --exact
	$(CARGO) test -p terlan_erlang emit::tests::core_index_get_call_waits_for_trait_wrapper_backend_dispatch -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_index_read_through_index_get_trait_wrapper -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_infers_index_assignment_through_index_set_trait -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_index_assignment_uses_index_set_call -- --exact
	$(CARGO) test -p terlan_erlang emit::tests::core_index_set_call_waits_for_trait_wrapper_backend_dispatch -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_index_assignment_through_mutable_receiver_rebinding -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_result_producing_mutable_receiver_methods -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_accepts_receiver_returning_mutable_receiver_methods -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_mutable_receiver_method_with_receiver_return -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_native_vector_selection_sort_fixture -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::tests::embedded_std_interfaces_include_native_vector_contract -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_release_std_collection_receiver_mutators -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_source_traversal_iterator_next -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_source_traversal_list_each_receiver_callback -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::reject_erlang_native_package_source_rejects_native_vector_import -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_value_binding_parser_accepts_simple_binding -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_value_binding_parser_rejects_source_let_expression -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_help_args_accept_long_and_short_help -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_help_args_reject_non_help_invocations -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_expression_with_bindings_builds_source_let_expression -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_json_event_without_extra_fields_is_valid_json -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_json_event_with_extra_fields_is_valid_json -- --exact
	$(TERLC_EXACT_TEST) commands::repl::tests::repl_load_sources_uses_project_manifest_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::repl::evaluator::tests::evaluator_renders_simple_integer_expression -- --exact
	$(TERLC_EXACT_TEST) commands::repl::evaluator::tests::evaluator_returns_unit_for_console_println -- --exact
	$(TERLC_EXACT_TEST) commands::repl::evaluator::tests::evaluator_routes_console_println_through_output_sink -- --exact
	$(TERLC_EXACT_TEST) commands::repl::evaluator::tests::evaluator_supports_type_of_for_integer -- --exact
	$(TERLC_EXACT_TEST) commands::repl::evaluator::tests::evaluator_supports_is_type_for_implicit_type_value -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::extracts_repl_doc_examples_from_block_docs -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::extracts_fenced_repl_doc_example_until_next_tag -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::validates_runnable_repl_doc_example_output -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::rejects_runnable_repl_doc_example_output_mismatch -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::validates_expected_error_repl_doc_example -- --exact
	$(TERLC_EXACT_TEST) commands::doc::validation::tests::rejects_expected_error_repl_doc_example_mismatch -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_check_accepts_matching_repl_example -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_check_rejects_mismatched_repl_example -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_command_writes_json_model -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_command_defaults_to_html_index -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_check_accepts_std_reference -- --exact
	$(TERLC_EXACT_TEST) commands::doc::tests::doc_command_generates_std_html_reference -- --exact
	$(TERLC) doc std --check
	$(TERLC) --out-dir /tmp/terlan-std-docs doc std
	test -f /tmp/terlan-std-docs/index.html
	test -f /tmp/terlan-std-docs/std.core.String.html
	test -f /tmp/terlan-std-docs/std.collections.Map.html
	$(MAKE) --no-print-directory stdlib-release-manifest-check
	$(MAKE) --no-print-directory stdlib-rust-backed-manifest-check
	$(MAKE) --no-print-directory stdlib-native-artifacts-check

formal-cli-phase-contract-gate:
	$(TERLC_EXACT_TEST) tests::run_phase_contract_fixtures_backend_parity -- --exact
	$(TERLC_EXACT_TEST) tests::run_phase_contract_fixtures_match_golden -- --exact
	$(TERLC_EXACT_TEST) tests::run_interface_success_and_error_paths -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact

formal-cli-build-gate:
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_parses_package_name_with_default_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_parses_explicit_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_missing_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_missing_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_invalid_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_invalid_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_unsupported_artifact_kind -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_accepts_reserved_empty_dependency_sections -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_parses_dependency_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_parses_erlang_package_adapter_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_unsupported_erlang_package_adapter -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_registry_dependency_in_local_scope -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_wrong_target_dependency_source -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_dependency_without_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::tests::project_manifest_rejects_unsupported_section -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_emits_erlang_source_and_beam_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_emits_erlang_sources_and_beams_for_directory -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_emits_erlang_sources_and_beams_for_recursive_package_layout -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_recursive_type_and_value_import_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_project_manifest_before_silent_directory_scan -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_manifest_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_project_source_outside_package_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_receiver_method_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_preserves_erlang_package_adapter_metadata_without_rebar3_files -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_manifest_multiple_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_with_local_path_dependency -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_local_path_dependency_without_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_local_path_dependency_cycle -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_hex_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_npm_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_rejects_cargo_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_directory_with_imported_constructors_and_aliases -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_directory_with_aliased_imported_alias_patterns -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_directory_with_aliased_imported_alias_constructor_chain -- --exact

formal-cli-a0-54-constructor-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-erlang-a0-15-gate
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_public_constructor_returning_private_type -- --exact

formal-cli-a0-55-function-clause-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(MAKE) --no-print-directory formal-syntax-a0-26-declaration-gate
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_refines_function_guards_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_function_clause_summaries -- --exact

formal-cli-a0-56-primary-expression-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(CARGO) test -p terlan_syntax parser::tests::formal_macro_expr_parses_as_primary_expr -- --exact
	$(CARGO) test -p terlan_syntax parser::tests::formal_raw_macro_expr_requires_immediate_raw_block -- --exact
	$(CARGO) test -p terlan_syntax parser::tests::formal_constructor_chain_expr_parses_with_record_expr -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_sequence_primary_expr_trees -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_raw_macro_primary_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_index_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact

formal-cli-a0-57-keyword-expression-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-3-gate
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_allows_keyword_expressions_in_operator_chains -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_if_expression_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_receive_expression_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_receive_after_expression_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_try_expression_trees -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_checks_if_expr_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_checks_receive_expr_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_checks_try_expr_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_supports_try_after_cleanup -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_supports_receive_after_timeout -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_if_core_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_receive_core_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_try_core_expr -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_receive_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_try_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_quote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_unquote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile -- --exact

formal-cli-a0-58-calls-and-references-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-16-gate
	$(MAKE) --no-print-directory formal-erlang-a0-17-gate
	$(MAKE) --no-print-directory formal-erlang-a0-19-gate
	$(MAKE) --no-print-directory formal-erlang-a0-20-gate
	$(MAKE) --no-print-directory formal-erlang-a0-21-diagnostic-gate
	$(CARGO) test -p terlan_typeck tests::syntax_output_infers_local_calls_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_infers_field_access_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_local_call_core_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_function_value_call_core_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_typechecks_pipe_into_function_value_call -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_index_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_field_access_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_marks_remote_call_proof_model_required -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_resolves_local_receiver_method_calls_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_method_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::build_command_compiles_project_receiver_method_entrypoint -- --exact

formal-cli-a0-59-data-form-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-8-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_preserves_binary_segment_text -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_record_suffix_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_binds_list_comprehension_patterns_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_rejects_list_comprehension_non_list_source_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_map_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_list_cons_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_fixed_array_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_list_comprehension_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_record_construct_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_record_access_expr -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_record_update_expr -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_multi_generator_list_comprehension_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact

formal-cli-a0-60-pattern-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-6-gate
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-syntax-a0-25-pattern-gate
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_recursive_expression_and_pattern_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_case_guard_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_declared_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_list_cons_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_binds_case_constructor_patterns_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_record_pattern_payload -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_float_payload -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_map_payload -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_map_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_record_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_float_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_declared_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase -- --exact

formal-cli-a0-61-lexical-and-name-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory parser-fixture-check
	$(CARGO) test -p terlan_syntax parser::tests::formal_raw_atom_patterns_are_literal_patterns -- --exact
	$(CARGO) test -p terlan_syntax parser::tests::formal_nullary_constructor_pattern_call_is_rejected -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_normalizes_prefixed_integer_literals -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_keeps_constructor_call_candidates_as_named_calls -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_literal_aliases_compare_with_literal_values_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_imported_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_compound_core_type_payloads -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_records_type_decl_core_body_payloads -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_float_literal -- --exact
	$(CARGO) test -p terlan_typeck tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact

formal-cli-js-gate:
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_uses_core_function_exports -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_handles_integer_division -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_js_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_emits_core_surface -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_minimal_direct_oxc_ast_module_prints_export -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_arithmetic_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_integer_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_float_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_string_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_bool_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_total_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_partial_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_guarded_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_lambda_value -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_simple_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_remote_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_remote_fun_ref -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_constructor_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_receive_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_try_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_send_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_quote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_array_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_unary_negation -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_list_cons -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_index_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_map_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_field_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_record_construct -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_record_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_record_update -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_template_instantiate -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_binary_operator_set -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_emits_named_call_private_helper -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_uses_direct_reachability_filter -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::tests::emit_core_module_to_typescript_declarations_uses_core_surface -- --exact
	$(TERLC_EXACT_TEST) tests::run_emit_js_reports_errors -- --exact
	$(TERLC_EXACT_TEST) tests::run_emit_js_writes_js_and_declarations -- --exact

formal-cli-rust-gate:
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_uses_core_function_visibility -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_compiles_pipe_forward_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_handles_function_value_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_compiles_string_contains_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_compiles_string_starts_with_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::tests::emit_core_module_to_rust_compiles_string_length_intrinsic_probe -- --exact

formal-cli-doc-gate:
	$(TERLC_EXACT_TEST) tests::formal_doc_markdown_generates_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) doctest_compile_tests::formal_doctest_compiles_terlan_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_html_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_markdown_html_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_syntax_output_discovers_entrypoints_and_routes -- --exact

formal-cli-a0-50-template-frontend-gate:
	$(TERLC_EXACT_TEST) commands::artifacts::tests::collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact

formal-cli-a0-62-template-boundary-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-43-template-raw-gate
	$(MAKE) --no-print-directory formal-cli-a0-50-template-frontend-gate
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(CARGO) test -p terlan_syntax syntax_output::tests::syntax_output_includes_struct_constructor_trait_and_template_signatures -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_unresolved_template_body_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact

formal-incremental-gate:
	$(TERLC_EXACT_TEST) tests::run_check_dir_rejects_module_layout_mismatch -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_dir_incremental_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) tests::run_check_dir_incremental_with_trait_interfaces -- --exact

formal-phase-gate:
	@tmpdir=$$(mktemp -d); \
	tmp2=$$(mktemp -d); \
	manifest1=$${tmpdir}/phase-a.json; \
	manifest2=$${tmp2}/phase-b.json; \
	out1=$${tmpdir}/gen1; \
	out2=$${tmp2}/gen2; \
	mkdir -p "$${out1}" "$${out2}"; \
	$(TERLC) check tests/fixtures/mathx.terl --emit-phase-manifest "$${manifest1}"; \
	$(TERLC) check tests/fixtures/mathx.terl --emit-phase-manifest "$${manifest2}"; \
	cmp "$${manifest1}" "$${manifest2}" >/dev/null; \
	$(TERLC) emit tests/fixtures/mathx.terl --out-dir "$${out1}"; \
	$(TERLC) emit tests/fixtures/mathx.terl --out-dir "$${out2}"; \
	diff -qr "$${out1}" "$${out2}" >/dev/null; \
	rm -rf "$${tmpdir}" "$${tmp2}"

formal-directory-phase-gate:
	@tmpdir=$$(mktemp -d); \
	cache_a=$${tmpdir}/cache-a; \
	cache_b=$${tmpdir}/cache-b; \
	manifest_a=$${tmpdir}/manifests-a; \
	manifest_b=$${tmpdir}/manifests-b; \
	mkdir -p "$${cache_a}" "$${cache_b}" "$${manifest_a}" "$${manifest_b}"; \
	$(TERLC) check tests/fixtures/phase_contract --cache-dir "$${cache_a}" --emit-phase-manifest "$${manifest_a}"; \
	$(TERLC) check tests/fixtures/phase_contract --cache-dir "$${cache_b}" --emit-phase-manifest "$${manifest_b}"; \
	diff -qr "$${manifest_a}" "$${manifest_b}" >/dev/null; \
	rm -rf "$${tmpdir}"
