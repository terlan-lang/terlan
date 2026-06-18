# Terlan CLI compiler-path targets.
#
# This file is included by the root Makefile. Target names remain public from the
# repository root, but CLI-specific recipes live with the CLI crate.

TERLC := $(CARGO) run -p terlan_cli --
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
TERLC_EXACT_TEST := $(EXACT_CARGO_TEST) -p terlan_cli
BROWSER_PACKAGE_PREFLIGHT_DIR ?= /tmp/terlan-0-0-4-browser-preflight

.PHONY: cli-help cli-check cli-build cli-test cli-release-artifact-linux cli-clean typecheck-fixture emit-fixture smoke browser-package-preflight release-0-0-4-preflight formal-cli-phase-contract-gate formal-cli-build-gate formal-cli-js-gate formal-cli-rust-gate formal-cli-doc-gate formal-cli-a0-50-template-frontend-gate formal-cli-a0-54-constructor-contract-gate formal-cli-a0-55-function-clause-contract-gate formal-cli-a0-56-primary-expression-contract-gate formal-cli-a0-57-keyword-expression-contract-gate formal-cli-a0-58-calls-and-references-contract-gate formal-cli-a0-59-data-form-contract-gate formal-cli-a0-60-pattern-contract-gate formal-cli-a0-61-lexical-and-name-contract-gate formal-cli-a0-62-template-boundary-contract-gate formal-incremental-gate formal-phase-gate formal-directory-phase-gate

cli-help:
	@echo "  make typecheck-fixture - terlan check fixture"
	@echo "  make emit-fixture      - emit fixture .erl/.typi to $(OUT_DIR)"
	@echo "  make smoke             - emit + erlc + runtime smoke test"
	@echo "  make browser-package-preflight - build and validate a JS browser package"
	@echo "  make release-0-0-4-preflight - run current 0.0.4 JS target release gate"
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

browser-package-preflight:
	rm -rf $(BROWSER_PACKAGE_PREFLIGHT_DIR)
	mkdir -p $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets
	printf '%s\n' 'module app.' '' 'import css "./assets/app.css" as AppCss.' 'import file "./assets/logo.txt" as Logo.' '' 'pub value(): Int ->' '    1.' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/app.terl
	printf '%s\n' 'body { color: black; }' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/app.css
	printf '%s\n' 'terlan' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/logo.txt
	$(TERLC) --target-profile js.browser --out-dir $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build build $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src --target js.browser
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/modules/app.js
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/index.html
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/assets/js/modules/app.js
	$(PYTHON) -c "import json,pathlib,sys; root=pathlib.Path(sys.argv[1]); manifest=json.loads((root/'manifest.json').read_text()); assert manifest['schema']=='terlan-web-build-v1', manifest; assert manifest['target_profile']=='js.browser', manifest; assert manifest['source_js_manifest']=='../js/manifest.json', manifest; assert (root/manifest['index']).is_file(), manifest['index']; assets=manifest['assets']; assert assets, manifest; missing=[entry['web_relative_path'] for entry in assets if not (root/entry['web_relative_path']).is_file()]; assert not missing, missing; kinds=[entry['kind'] for entry in assets]; assert 'javascript-module' in kinds, kinds; assert 'asset-css' in kinds, kinds; assert 'asset-file' in kinds, kinds" $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web
	$(TERLC) serve $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web --check

release-0-0-4-preflight:
	$(CARGO) fmt --all -- --check
	$(MAKE) --no-print-directory release-boundary-check
	$(MAKE) --no-print-directory single-root-contract-check
	$(MAKE) --no-print-directory diff-whitespace-check
	$(MAKE) --no-print-directory workspace-version-check
	$(MAKE) --no-print-directory release-version-metadata-check
	$(MAKE) --no-print-directory source-extension-check
	$(MAKE) --no-print-directory rust-quality-check
	$(MAKE) --no-print-directory test-hierarchy-check
	$(MAKE) --no-print-directory cli-exact-selector-check
	$(MAKE) --no-print-directory shared-helper-check
	$(MAKE) --no-print-directory oxc-boundary-check
	$(MAKE) --no-print-directory web-capability-decision-check
	$(MAKE) --no-print-directory changelog-public-scope-check
	$(MAKE) --no-print-directory internal-docs-check
	$(MAKE) --no-print-directory module-readme-check
	$(MAKE) --no-print-directory rustdoc-check
	$(MAKE) --no-print-directory cli-check
	$(MAKE) --no-print-directory stdlib-check
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_source_and_beam_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_module_and_manifest_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_std_core_string_intrinsics -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_js_declarations_when_requested -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_browser_web_package_for_js_browser_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_manifest_declared_static_assets_for_js_browser_project -- --exact
	$(MAKE) --no-print-directory browser-package-preflight
	$(TERLC_EXACT_TEST) commands::serve::serve_test::run_serve_check_validates_without_binding_port -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_accepts_manifest_handler -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_unsafe_handler_route -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::manifest_handler_for_request_matches_get_and_head -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_ebin_dir_for_web_root_uses_build_root_sibling -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::render_beam_handler_eval_passes_request_map_and_target -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::parse_beam_handler_stdout_accepts_stable_response_protocol -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_handler_response_converts_from_native_http_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::beam_handler_response_rejects_invalid_native_status -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::parse_beam_handler_stdout_rejects_bad_status -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::execute_beam_handler_reports_missing_ebin_before_running_erl -- --exact
	$(TERLC_EXACT_TEST) commands::serve::handler::handler_test::execute_beam_handler_reports_missing_beam_before_running_erl -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_missing_manifest_asset -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::validate_web_package_rejects_unsafe_manifest_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::inject_reload_script_inserts_before_body_close -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::render_reload_sse_headers_preserves_live_reload_response_contract -- --exact
	$(TERLC_EXACT_TEST) commands::serve::watch::watch_test::web_package_snapshot_changes_when_asset_content_changes -- --exact
	$(TERLC_EXACT_TEST) commands::serve::watch::watch_test::broadcast_reload_removes_disconnected_subscribers -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::parse_serve_static_args_preserves_shared_server_settings -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::parse_init_args_accepts_web_profile_before_project -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::parse_init_args_accepts_web_profile_after_project -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::write_project_web_profile_creates_browser_and_http_modules -- --exact
	$(TERLC_EXACT_TEST) commands::init::init_test::next_steps_for_web_profile_build_both_targets_and_serve -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::bind::bind_test::generate_js_dom_bindings_writes_fixture_outputs -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::target_profile_test::parse_args_accepts_js_shared_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::target_profile_test::parse_args_accepts_js_target_profile_alias -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::target_profile_test::parse_args_accepts_js_browser_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::target_profile_test::parse_args_accepts_js_worker_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::parse_test_args_accepts_explicit_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::release_api_test_modules_have_embedded_runner_support -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::effective_js_test_profile_defaults_to_shared_js_profile -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::validation_pass_report_marks_all_tests_as_validated -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_command_test::run_js_tests_writes_validation_manifests -- --exact
	$(TERLC) test std/js/string_test.terl --target js
	$(TERLC) --target-profile js.browser test std/js --target js
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::embedded_std_interfaces_include_js_std_contracts -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::compile_syntax_module_with_js_profile_resolves_js_string_summary -- --exact
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::compile_syntax_module_with_browser_profile_resolves_generated_dom_summary -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::diagnostics_test::build_command_rejects_js_std_import_for_erlang_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::diagnostics_test::build_command_rejects_browser_dom_import_for_shared_js_target -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::std_bridge_test::rejects_js_std_module_for_non_js_profiles -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::std_bridge_test::rejects_browser_dom_js_std_module_for_shared_js_profile -- --exact

formal-cli-phase-contract-gate:
	$(TERLC_EXACT_TEST) main_test::tests::run_phase_contract_fixtures_backend_parity -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::run_phase_contract_fixtures_match_golden -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::interface_test::run_interface_success_and_error_paths -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact

formal-cli-build-gate:
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_package_name_with_default_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_explicit_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_missing_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_missing_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_invalid_package_name -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_invalid_package_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_artifact_kind -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_accepts_reserved_empty_dependency_sections -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_erlang_package_adapter_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_erlang_package_adapter -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_registry_dependency_in_local_scope -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_wrong_target_dependency_source -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_dependency_without_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_section -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_source_and_beam_for_single_file -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_sources_and_beams_for_directory -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_erlang_sources_and_beams_for_recursive_package_layout -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_compiles_recursive_type_and_value_import_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_manifest_before_silent_directory_scan -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_manifest_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_source_outside_package_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_receiver_method_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_preserves_erlang_package_adapter_metadata_without_rebar3_files -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_manifest_multiple_source_roots -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_compiles_project_with_local_path_dependency -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_without_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_cycle -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_hex_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_npm_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_cargo_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_imported_constructors_and_aliases -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_aliased_imported_alias_patterns -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_compiles_directory_with_aliased_imported_alias_constructor_chain -- --exact

formal-cli-a0-54-constructor-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-erlang-a0-15-gate
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_rejects_public_constructor_returning_private_type -- --exact

formal-cli-a0-55-function-clause-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(MAKE) --no-print-directory formal-syntax-a0-26-declaration-gate
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_refines_function_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_function_clause_summaries -- --exact

formal-cli-a0-56-primary-expression-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_macro_expr_parses_as_primary_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_raw_macro_expr_requires_immediate_raw_block -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_constructor_chain_expr_parses_with_record_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_sequence_primary_expr_trees -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_raw_macro_primary_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_index_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact

formal-cli-a0-57-keyword-expression-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-3-gate
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-syntax-a0-23-keyword-gate
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_allows_keyword_expressions_in_operator_chains -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_if_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_receive_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_receive_after_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_try_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_checks_if_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_checks_receive_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_checks_try_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_supports_try_after_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_supports_receive_after_timeout -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_if_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_receive_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_try_core_expr -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_receive_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_try_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_quote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_unquote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile -- --exact

formal-cli-a0-58-calls-and-references-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-16-gate
	$(MAKE) --no-print-directory formal-erlang-a0-17-gate
	$(MAKE) --no-print-directory formal-erlang-a0-19-gate
	$(MAKE) --no-print-directory formal-erlang-a0-20-gate
	$(MAKE) --no-print-directory formal-erlang-a0-21-diagnostic-gate
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_infers_local_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_infers_field_access_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_local_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_function_value_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_typechecks_pipe_into_function_value_call -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_index_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_field_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_marks_remote_call_proof_model_required -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_resolves_local_receiver_method_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_rejects_method_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_language_test::build_command_compiles_project_receiver_method_entrypoint -- --exact

formal-cli-a0-59-data-form-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-8-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-syntax-a0-24-collection-gate
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_preserves_binary_segment_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_record_suffix_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_binds_list_comprehension_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_rejects_list_comprehension_non_list_source_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_map_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_list_cons_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_fixed_array_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_list_comprehension_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_record_construct_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_record_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_record_update_expr -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_multi_generator_list_comprehension_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact

formal-cli-a0-60-pattern-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-4-gate
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-6-gate
	$(MAKE) --no-print-directory formal-erlang-a0-7-gate
	$(MAKE) --no-print-directory formal-erlang-a0-9-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) --no-print-directory formal-syntax-a0-25-pattern-gate
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_recursive_expression_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_case_guard_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_declared_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_list_cons_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_binds_case_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_record_pattern_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_float_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_map_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_map_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_record_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_float_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_error_manifest_test::run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_declared_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_accepts_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_constructor_identity_manifest_test::run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase -- --exact

formal-cli-a0-61-lexical-and-name-contract-gate:
	$(MAKE) --no-print-directory formal-erlang-a0-5-gate
	$(MAKE) --no-print-directory formal-erlang-a0-10-gate
	$(MAKE) --no-print-directory formal-erlang-a0-12-gate
	$(MAKE) --no-print-directory formal-erlang-a0-13-gate
	$(MAKE) -f crates/terlan_syntax/syntax.mk --no-print-directory parser-fixture-check
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_raw_atom_patterns_are_literal_patterns -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_nullary_constructor_pattern_call_is_rejected -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_normalizes_prefixed_integer_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_keeps_constructor_call_candidates_as_named_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_literal_aliases_compare_with_literal_values_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_imported_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_compound_core_type_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_records_type_decl_core_body_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_float_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_binary_literal -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_progression_test::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact

formal-cli-js-gate:
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_uses_core_function_exports -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_handles_integer_division -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_js_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_emits_core_surface -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_minimal_direct_oxc_ast_module_prints_export -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_arithmetic_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_integer_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_float_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_bool_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_total_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_partial_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_guarded_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_lambda_value -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_simple_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_destructuring_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_remote_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_rejects_remote_fun_ref_source_syntax -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_constructor_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_try_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_quote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_array_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_unary_negation -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_list_cons -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_index_trait_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_map_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_field_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_construct -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_record_update -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_template_instantiate -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_binary_operator_set -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_emits_named_call_private_helper -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_uses_direct_reachability_filter -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::emit_core_module_to_typescript_declarations_uses_core_surface -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::emit_js_test::run_emit_js_reports_errors -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::emit_js_test::run_emit_js_writes_js_and_declarations -- --exact

formal-cli-rust-gate:
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_uses_core_function_visibility -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_pipe_forward_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_handles_function_value_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_contains_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_starts_with_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_length_intrinsic_probe -- --exact

formal-cli-doc-gate:
	$(TERLC_EXACT_TEST) main_test::tests::doc_test::formal_doc_markdown_generates_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::doc_test::formal_doctest_compiles_terlan_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_html_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_markdown_html_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_syntax_output_discovers_entrypoints_and_routes -- --exact

formal-cli-a0-50-template-frontend-gate:
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact

formal-cli-a0-62-template-boundary-contract-gate:
	$(MAKE) --no-print-directory formal-syntax-a0-43-template-raw-gate
	$(MAKE) --no-print-directory formal-cli-a0-50-template-frontend-gate
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_includes_struct_constructor_trait_and_template_signatures -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_language_feature_rejection_test::run_check_single_file_rejects_unresolved_template_body_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_target_profile_gate_test::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::static_site_test::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact

formal-incremental-gate:
	$(TERLC_EXACT_TEST) main_test::tests::check_phase_test::run_check_dir_rejects_module_layout_mismatch -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_incremental_test::run_check_dir_incremental_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) main_test::tests::check_incremental_test::run_check_dir_incremental_with_trait_interfaces -- --exact

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
