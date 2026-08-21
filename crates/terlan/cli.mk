# Terlan CLI compiler-path targets.
#
# This file is included by the root Makefile. Target names remain public from the
# repository root, but CLI-specific recipes live with the CLI crate.

TERLAN_PREBUILT_BINARY := bash scripts/run_prebuilt_terlan_binary.sh
TERLC := $(TERLAN_PREBUILT_BINARY) terlc none --
TERLAN_QUALITY := $(TERLAN_PREBUILT_BINARY) terlan-quality quality-tools --
TERLAN_BENCHMARK := $(TERLAN_PREBUILT_BINARY) terlan-benchmark benchmark-tools --
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
TERLC_EXACT_TEST := $(EXACT_CARGO_TEST) -p terlan --lib
TERLAN_TEST_WORKSPACE_ROOT ?= target/test-workspaces
BROWSER_PACKAGE_PREFLIGHT_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/browser-package-preflight
STATIC_PROFILE_PREFLIGHT_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/static-profile-preflight
STATIC_DOCS_PREFLIGHT_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/static-docs-preflight
WEB_PROFILE_PREFLIGHT_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/web-profile-preflight
CLI_BUILD_EXECUTABLE_CHECK_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/terlc-build-executable
CLI_BUILD_EXECUTABLE_HANDOFF_DIR ?= $(TERLAN_TEST_WORKSPACE_ROOT)/terlc-build-executable-handoff

.PHONY: abi1-pre-freeze-check abi1-continuous-fuzz-check abi1-cross-target-conformance-check abi1-tail-latency-check abi1-zero-copy-conformance-check abi1-specialization-equivalence-check abi1-trusted-adapter-audit-check abi1-release-candidate-check abi1-compatibility-freeze-check

abi1-pre-freeze-check:
	$(TERLAN_QUALITY) abi1-pre-freeze

abi1-continuous-fuzz-check:
	test -n "$$TERLAN_ABI1_REVISION"
	TERLAN_ABI1_REVISION="$$TERLAN_ABI1_REVISION" $(EXACT_CARGO_TEST) --locked --release -p terlan --test abi1_evidence_producers abi1_continuous_fuzz_producer -- --exact
	$(TERLAN_QUALITY) abi1-continuous-fuzz

abi1-cross-target-conformance-check:
	bash scripts/produce_abi1_cross_target_evidence.sh
	$(TERLAN_QUALITY) abi1-cross-target-conformance

abi1-tail-latency-check:
	test -n "$$TERLAN_ABI1_REVISION"
	TERLAN_ABI1_REVISION="$$TERLAN_ABI1_REVISION" $(EXACT_CARGO_TEST) --locked --release -p terlan --test abi1_evidence_producers abi1_tail_latency_producer -- --exact
	$(TERLAN_QUALITY) abi1-tail-latency

abi1-zero-copy-conformance-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib managed_sequence_test
	$(TERLAN_QUALITY) abi1-zero-copy-conformance

abi1-specialization-equivalence-check:
	test -n "$$TERLAN_ABI1_REVISION"
	TERLAN_ABI1_REVISION="$$TERLAN_ABI1_REVISION" $(EXACT_CARGO_TEST) --locked --release -p terlan --test abi1_evidence_producers abi1_specialization_equivalence_producer -- --exact
	$(TERLAN_QUALITY) abi1-specialization-equivalence

abi1-trusted-adapter-audit-check:
	$(TERLAN_QUALITY) abi1-trusted-adapter-audit

abi1-release-candidate-check: abi1-continuous-fuzz-check abi1-cross-target-conformance-check abi1-tail-latency-check abi1-zero-copy-conformance-check abi1-specialization-equivalence-check abi1-trusted-adapter-audit-check
	$(TERLAN_QUALITY) abi1-release-candidate

abi1-compatibility-freeze-check: abi1-release-candidate-check
	$(TERLAN_QUALITY) abi1-compatibility-freeze

.PHONY: cli-help cli-check cli-build cli-test cli-test-fast cli-test-full cli-test-release cli-release-artifact-current cli-release-artifact-linux cli-clean vm-artifact-check cli-terlc-build-executable-check cli-terlan-vm-compiler-bridge-check browser-package-preflight js-stdlib-smoke-check static-profile-preflight static-docs-check web-profile-preflight serve-static-smoke serve-web-smoke static-command-check http-router-check http-observability-check http-tls-check http-acme-live-check web-compose-check template-contract-check artifact-template-check typed-template-interpolation-check typed-template-interpolation-vm-check typed-template-interpolation-js-check typed-template-interpolation-tooling-check typed-template-interpolation-backend-check function-language-surface-check repeated-let-syntax-check comprehension-guards-check flexible-shape-guards-check private-field-check db-command-check terlc-debugger-check terlc-debugger-selector-inventory native-boundary-postgres-check native-boundary-http-cookie-check native-boundary-postgres-docker-check repl-check sql-form-check sql-runtime-check api-schema-check runtime-release-dependency-check terlan-format-check formatter-pipe-canonicalization-check terlan-lint-style-check terlan-readability-canonicalization-check terlan-grouped-binding-check terlan-function-reference-check formal-cli-phase-contract-gate formal-cli-build-gate formal-cli-js-gate formal-cli-rust-gate formal-cli-doc-gate formal-cli-a0-50-template-frontend-gate formal-cli-a0-54-constructor-contract-gate formal-cli-a0-55-function-clause-contract-gate formal-cli-a0-56-primary-expression-contract-gate formal-cli-a0-57-keyword-expression-contract-gate formal-cli-a0-58-calls-and-references-contract-gate formal-cli-a0-59-data-form-contract-gate formal-cli-a0-60-pattern-contract-gate formal-cli-a0-61-lexical-and-name-contract-gate formal-cli-a0-62-template-boundary-contract-gate formal-incremental-gate formal-phase-gate formal-directory-phase-gate

cli-help:
	@echo "  make browser-package-preflight - build and validate a JS browser package"
	@echo "  make js-stdlib-smoke-check - run bounded generated std.js test coverage"
	@echo "  make static-profile-preflight - build and validate a static profile site"
	@echo "  make static-docs-check - build and validate a docs-shaped static site"
	@echo "  make web-profile-preflight - scaffold and validate a web profile package"
	@echo "  make serve-static-smoke - run static profile serve smoke"
	@echo "  make serve-web-smoke - run web profile serve smoke"
	@echo "  make static-command-check - run public static command wrapper regressions"
	@echo "  make http-router-check - run HTTP router matcher and route-validation regressions"
	@echo "  make http-observability-check - run HTTP log/error/header regressions"
	@echo "  make http-tls-check - run HTTP TLS manifest and serve guard regressions"
	@echo "  make http-acme-live-check - run manually enabled live ACME smoke"
	@echo "  make web-compose-check - run web-profile Docker Compose contract regressions"
	@echo "  make template-contract-check - run typed template metadata/render regressions"
	@echo "  make artifact-template-check - run artifact-template suffix and structure regressions"
	@echo "  make typed-template-interpolation-check - run 0.0.7 typed interpolation closure gates"
	@echo "  make typed-template-interpolation-vm-check - run VM/HTTP typed interpolation regressions"
	@echo "  make typed-template-interpolation-js-check - run JS/browser typed interpolation regressions"
	@echo "  make typed-template-interpolation-backend-check - compare VM, JS/browser, and artifact renderers"
	@echo "  make comprehension-guards-check - run list-comprehension parser, type, and VM regressions"
	@echo "  make flexible-shape-guards-check - run case/function guard parser, type, and VM regressions"
	@echo "  make private-field-check - run private struct field visibility regressions"
	@echo "  make db-command-check - run Postgres migration command regressions"
	@echo "  make terlc-debugger-check - run VM-owned debugger regressions"
	@echo "  make native-boundary-postgres-check - run native-boundary Postgres adapter regressions"
	@echo "  make native-boundary-http-cookie-check - run native-boundary HTTP cookie regressions"
	@echo "  make native-boundary-postgres-docker-check - run live Postgres native-boundary checks when configured"
	@echo "  make sql-form-check - run typed SQL form parser/typechecker regressions"
	@echo "  make sql-runtime-check - run typed SQL CoreIR regressions"
	@echo "  make api-schema-check - run API contract, OpenAPI emit, and client import regressions"
	@echo "  make runtime-release-dependency-check - require committed live Postgres/TLS runtime dependencies"
	@echo "  make terlan-format-check - require canonical rustfmt-style Terlan sources"
	@echo "  make formatter-pipe-canonicalization-check - run nested first-argument pipe formatter regressions"
	@echo "  make terlan-lint-style-check - run public lint command and style diagnostics regressions"
	@echo "  make release-artifact-current - build and smoke-test the current platform artifact"
	@echo "  make vm-artifact-check - build and smoke-test the standalone terlan-vm artifact"
	@echo "  make terlc-build-executable-check - prove terlc build emits a runnable VM executable"
	@echo "  make terlan-vm-compiler-bridge-check - compare compiler VM run and direct VM source execution"
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
	$(CARGO) check --workspace

cli-build:

cli-test: cli-test-fast

cli-test-fast:
	$(RUST_TEST) --workspace --bins --no-run
	$(MAKE) --no-print-directory vm-artifact-check
	$(TERLC_EXACT_TEST) tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_emits_terlan_vm_artifact_without_erlang_or_beam -- --exact

cli-test-full:
	PATH="$(CURDIR)/target/debug:$$PATH" $(RUST_TEST) --workspace

ifeq ($(TERLAN_RUST_SUITE_ALREADY_RUN),1)
cli-test-release:
	@echo "[cli-test-release] canonical Rust suite already passed."
else
cli-test-release: cli-test-full
endif

formatter-pipe-canonicalization-selector-inventory:
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_nested_module_calls_without_pipe_promotion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_nested_receiver_calls_without_pipe_promotion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_nested_selected_import_calls_without_pipe_promotion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_wraps_explicit_pipe_chains_to_one_stage_per_line -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_does_not_promote_named_argument_calls_to_pipe_chain -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_does_not_promote_function_value_calls_to_pipe_chain -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_does_not_promote_nested_call_arguments_to_pipe_chain -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_splits_short_function_body_semicolon_sequences -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_infers_pipe_forward_into_imported_module_member_call -- --exact

terlan-format-check:
	target/debug/terlc fmt --check \
		std \
		scripts/self_validation \
		benchmarks \
		crates/terlan/tests \
		tests/language \
		tests/template \
		tests/pattern \
		tests/binary \
		tests/operator \
		tests/rc \
		tests/test_model/positive \
		tests/test_model/runtime \
		tests/std

formatter-pipe-canonicalization-check:
	$(RUST_TEST) -p terlan --lib compiler::syntax::formatter::formatter_test::
	$(RUST_TEST) -p terlan --lib compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_infers_pipe_forward_into_imported_module_member_call

terlc-debugger-check: terlan-vm-run-command-check terlan-vm-test-command-check vm-diagnostics-quality-check
	$(TERLAN_QUALITY) source-map-debug-info

terlc-debugger-selector-inventory:
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_debug_command_to_native_admission -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_debug_command_after_json_diagnostic_flag -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_repl_debug_to_vm_surface -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_repl_debug_after_json_diagnostic_flag -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_invalid_breakpoint_spec -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_empty_breakpoint_condition -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_missing_script_file -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_debug_breakpoint_management_script_to_native_admission -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_script_invalid_breakpoint_selector -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_without_target_or_script -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_duplicate_script -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_rejects_debug_command_too_many_targets -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::debug_usage_documents_script_commands -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_help_debug_to_debug_usage -- --exact
	$(TERLC_EXACT_TEST) \
		tests::debug_cli_test::run_cli_routes_debug_help_to_debug_usage -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::debugger_control::debugger_control_test
	$(RUST_TEST) -p terlan --lib runtime::vm::debugger_transport::debugger_transport_test
	$(TERLC_EXACT_TEST) \
		commands::serve::handler_cache::invocation::invocation_test::debugger_pause_and_step_follow_owner_migration_without_duplicate_execution -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::native_debug_script_stops_steps_and_inspects_real_aot_actor -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::public_debug_command_executes_script_through_live_native_shard -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::live_debugger_rejects_missing_process_and_frame_eval_with_stable_errors -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::explicit_debug_intrinsic_stops_and_resumes_generated_continuation -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::failure_condition_exposes_typed_restart_and_skip_resumes_in_vm -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::native_debug_session_rejects_stale_source_map -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_requires_target_or_script -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_accepts_target_breakpoint_script_and_json_events -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_accepts_nested_module_and_file_line_breakpoints -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_accepts_conditional_breakpoints -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_invalid_breakpoint_spec -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_empty_breakpoint_condition -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_zero_line_breakpoint -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_unknown_option -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_missing_breakpoint_value -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_missing_script_value -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_duplicate_script -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_args_rejects_too_many_targets -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_accepts_commands_and_breakpoints -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_command_inventory_matches_parser -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_unknown_command -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_missing_command_argument -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_invalid_frame_selector -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_invalid_breakpoint_selector -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_unexpected_list_argument -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_unexpected_help_argument -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_script_rejects_empty_scripts -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_native_image_text_report_is_stable -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_native_image_text_report_includes_script_command_count -- --exact
	$(TERLC_EXACT_TEST) \
		commands::debug::debug_test::debug_error_renderers_are_stable -- --exact
	$(RUST_TEST) -p terlan --lib commands::debug::evaluation::test
	$(RUST_TEST) -p terlan --lib commands::debug::presentation::test
	$(RUST_TEST) -p terlan --lib commands::debug::tracing::test
	$(TERLC_EXACT_TEST) \
		commands::repl::repl_aot_test::repl_debug_mode_executes_generation_through_live_vm_debugger -- --exact
	$(TERLC_EXACT_TEST) \
		commands::build::build_test::tests::debug_info_artifact_test::debug_info_artifact_covers_public_and_private_functions -- --exact
	$(TERLC_EXACT_TEST) \
		commands::build::build_test::tests::debug_info_artifact_test::debug_info_artifact_covers_generated_continuation_functions -- --exact
	$(TERLC_EXACT_TEST) \
		runtime::vm::process::process_test::debugger_mailbox_snapshot_is_bounded_without_consuming_or_advancing_cursor -- --exact

terlan-lint-style-selector-inventory:
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_semicolon_chain_with_stable_diagnostic -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_fix_splits_simple_semicolon_chain -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_fix_preserves_comment_semicolon_lines -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_fix_splits_semicolon_chains_with_string_arguments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_directory_sources_in_sorted_order -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_command_fix_rewrites_safe_chain -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_command_reports_unfixed_chain -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_command_rejects_unknown_flag -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_deep_expression_tree -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_staged_expression_tree -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_short_callback_name_for_multi_expression_body -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_meaningful_callback_name_for_multi_expression_body -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_short_callback_name_for_single_expression_body -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_unused_destructured_let_binding -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_underscore_prefixed_destructured_let_binding -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_used_destructured_let_bindings -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_unused_destructured_case_binding -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_redundant_comment_restatement -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_explanatory_line_comment -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_doc_comment_before_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::branch_test::lint_reports_boolean_heavy_branch_condition -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::branch_test::lint_accepts_simple_boolean_branch_condition -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::branch_test::lint_accepts_named_predicate_branch_condition -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::branch_test::lint_accepts_boolean_heavy_branch_text_inside_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_public_declaration_without_docs -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_documented_public_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_undocumented_public_test_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_reports_doc_comment_missing_star_space -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_accepts_canonical_doc_comment_spacing -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::readability_test::lint_ignores_non_doc_block_comment_spacing -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_reports_function_clause_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_accepts_function_clause_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_reports_match_arm_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_accepts_match_arm_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_reports_ordinary_source_file_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_accepts_ordinary_source_file_at_size_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::complexity_test::lint_accepts_generated_source_file_size_with_manifest_policy -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::maintainability_test::lint_reports_unstructured_debug_call_in_production_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::maintainability_test::lint_accepts_debug_words_in_strings_and_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::maintainability_test::lint_accepts_structured_logger_debug_call -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::maintainability_test::lint_accepts_debug_call_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_reports_manual_message_tag_equality_in_actor_handler -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_pattern_matched_actor_message_handler -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_tag_equality_outside_actor_handler -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_message_tag_equality_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_reports_actor_lifecycle_state_parameter_without_state_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_actor_lifecycle_state_parameter_with_state_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_state_typed_parameter_outside_actor_lifecycle -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::actor_vm_test::lint_accepts_actor_lifecycle_state_parameter_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_import_before_module_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_docs_before_module_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_line_comments_before_module_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_import_after_function_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_imports_before_declarations -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_docs_between_imports_and_declarations -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_duplicate_module_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_module_mentions_inside_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_type_declaration_after_function_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_impl_declaration_after_function_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_type_impl_function_declaration_order -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_reports_std_module_declaration_path_mismatch -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_std_module_declaration_matching_path -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_non_std_module_declaration_path_mismatch -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_std_module_path_with_comment_module_mentions -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_comments_between_declaration_blocks -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::consistency_test::lint_accepts_multiple_impl_blocks_with_methods_before_functions -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_reports_generated_file_missing_source_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_generated_file_with_source_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_non_generated_file_mentioning_generated_values -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_reports_generated_file_inline_lint_suppression -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_generated_file_lint_suppression_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_non_generated_inline_lint_suppression_text -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_reports_generated_file_unstructured_skip_note -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_generated_file_skip_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::generated_test::lint_accepts_non_generated_unsupported_notes -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_duplicate_import_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_distinct_selected_imports -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_import_text_inside_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_duplicate_selected_import_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_distinct_selected_import_names -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_duplicate_selected_type_import_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_unsorted_import_declaration -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_sorted_import_declarations -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_import_text_inside_block_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_unsorted_selected_import_names -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_sorted_selected_import_names -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_sorted_type_selected_import_names -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_reports_split_selected_imports_from_same_module -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_grouped_selected_imports_from_same_module -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::lint_accepts_separate_value_and_type_selected_imports -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_reports_unused_selected_value_import -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_selected_value_imports -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_selected_type_import -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_constructor_shaped_selected_import_without_textual_use -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_selected_import_alias -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_reports_unused_module_import -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_module_import_by_visible_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_module_import_by_qualified_path -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_used_module_import_alias -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_selected_imports_for_module_unused_rule -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::unused_test::lint_accepts_type_and_asset_imports_for_module_unused_rule -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_reports_redundant_module_qualified_selected_import_call -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_reports_redundant_full_qualified_selected_import_call -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_accepts_direct_selected_import_call -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_reports_redundant_qualified_call_for_aliased_selected_import -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_accepts_type_and_constructor_selected_imports_for_redundant_qualifier_rule -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::redundant_test::lint_accepts_qualified_selected_import_text_inside_comments_and_strings -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_reports_default_import_that_could_be_selected -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_reports_full_qualified_default_import_that_could_be_selected -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_accepts_default_import_used_for_multiple_members -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_accepts_aliased_default_import -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_accepts_constructor_shaped_member_use -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_accepts_default_import_used_for_constructor_and_member -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::imports_test::selected_default_test::lint_accepts_default_import_member_text_inside_comments_and_strings -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_camel_case_function_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_snake_case_function_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_camel_case_method_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_upper_camel_type_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_noncanonical_type_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_noncanonical_struct_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_upper_camel_struct_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_case_underscore_function_name_collision -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_case_underscore_type_name_collision -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_distinct_declaration_names -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_camel_case_function_parameter_binding -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_reports_camel_case_let_pattern_binding -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_snake_case_value_bindings -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::naming_test::lint_accepts_underscore_prefixed_value_bindings -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::targets_test::lint_reports_incompatible_target_std_imports -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::targets_test::lint_accepts_vm_and_native_std_imports_together -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::targets_test::lint_accepts_target_std_imports_inside_comments -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_literal_true_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_literal_true_body_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_unannotated_helper_identity_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_non_literal_assertion_condition_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_qualified_literal_true_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_identity_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_non_identity_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_declaration_only_surface_test_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_generated_declaration_only_surface_test_name -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_unannotated_declaration_only_helper_name_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_oversized_test_assertion_volume -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_focused_test_assertion_volume_threshold -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_unannotated_helper_assertion_volume_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_repeated_assert_equal_table_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_two_assert_equal_rows_without_table_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_accepts_mixed_assertions_without_table_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_rejects_identity_assertion_rule_outside_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::lint_reports_qualified_identity_assertion_in_test_source -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::test_rules_test::lint_reports_roundtrip_test_without_property_runner -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::test_rules_test::lint_accepts_roundtrip_test_with_property_runner -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::test_rules_test::lint_accepts_ordinary_ordering_example_without_property_runner -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::test_rules_test::lint_reports_ordering_law_without_property_runner -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_reports_safe_nested_module_call_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_command_fix_rewrites_safe_nested_module_call_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_reports_safe_receiver_inner_call_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_command_fix_rewrites_safe_receiver_inner_call_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_command_fix_rewrites_declared_local_helper_pipe_stage -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_command_fix_rewrites_selected_import_pipe_stage -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_named_argument_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_named_receiver_call_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_function_value_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_unproven_local_variable_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_nested_argument_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) tests::help_test::top_level_usage_hides_internal_scratch_commands -- --exact
	$(TERLC_EXACT_TEST) tests::help_test::run_cli_accepts_command_local_help_for_known_commands -- --exact

terlan-lint-style-check: terlan-readability-canonicalization-check
	$(RUST_TEST) -p terlan --lib commands::lint::lint_test::
	$(RUST_TEST) -p terlan --lib tests::help_test::
	$(TERLC) lint std/collections

TERLAN_GROUPED_BINDING_STDLIB_ROOTS := $(filter-out std/js/ std/summaries/,$(wildcard std/*/))

terlan-readability-canonicalization-check:
	$(TERLC) lint --only TL0009 --only TL0010 benchmarks crates proofs scripts tests $(TERLAN_GROUPED_BINDING_STDLIB_ROOTS)

terlan-grouped-binding-check terlan-function-reference-check: terlan-readability-canonicalization-check

cli-release-artifact-current: | terlan-tvm-platform-matrix-bootstrap
	$(CARGO) build --release --features editor-lsp --bin terlc --bin terlan-vm --bin terlan-native-worker --bin terlan-lsp
	mkdir -p dist
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-package

cli-release-artifact-linux: export TERLAN_RELEASE_OS = Linux
cli-release-artifact-linux: export TERLAN_RELEASE_ARCH = x86_64
cli-release-artifact-linux: cli-release-artifact-current

cli-clean:
	$(CARGO) clean
	bash scripts/clean_build_outputs.sh

vm-artifact-check:
	$(CARGO) build --bin terlan-vm
	target/debug/terlc test scripts/self_validation/VmCliBridgeTest.terl \
		--name standalone_vm_runs_source_artifact

cli-terlc-build-executable-check:
	rm -rf $(CLI_BUILD_EXECUTABLE_CHECK_DIR) $(CLI_BUILD_EXECUTABLE_HANDOFF_DIR)
	mkdir -p $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/src/app
	printf '%s\n' '[package]' 'name = "app"' 'version = "0.0.1"' '' '[build]' 'source_roots = ["src"]' 'artifact = "terlan-vm"' > $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/terlan.toml
	printf '%s\n' 'module app.Main.' '' 'import std.io.Console.{println}.' '' 'pub main(): Unit ->' '    println("hello from terlc build executable").' > $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/src/app/Main.terl
	target/debug/terlc --out-dir $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/_build build $(CLI_BUILD_EXECUTABLE_CHECK_DIR)
	test -x $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/_build/bin/app
	test -x $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/_build/bin/terlan-vm
	cp -a $(CLI_BUILD_EXECUTABLE_CHECK_DIR)/_build $(CLI_BUILD_EXECUTABLE_HANDOFF_DIR)
	build_output="$$(env -u TERLAN_VM_RUNNER PATH=/usr/bin:/bin $(CLI_BUILD_EXECUTABLE_HANDOFF_DIR)/bin/app)"; \
	if [ "$$build_output" != "hello from terlc build executable" ]; then \
		printf '%s\n' "terlc build executable check failed"; \
		printf '%s\n' "$$build_output"; \
		exit 1; \
	fi

cli-terlan-vm-compiler-bridge-check:
	target/debug/terlc test scripts/self_validation/VmCliBridgeTest.terl \
		--name compiler_bridge_covers_output_intrinsics_and_runtime

browser-package-preflight: | terlan-web-manifest-preflight-bootstrap
	rm -rf $(BROWSER_PACKAGE_PREFLIGHT_DIR)
	mkdir -p $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets
	printf '%s\n' 'module app.' '' 'import css "./assets/app.css" as AppCss.' 'import file "./assets/logo.txt" as Logo.' '' 'pub value(): Int ->' '    1.' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/app.terl
	printf '%s\n' 'body { color: black; }' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/app.css
	printf '%s\n' 'terlan' > $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src/assets/logo.txt
	$(TERLAN_BOOTSTRAP_COMPILER) --target-profile js.browser --out-dir $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build build $(BROWSER_PACKAGE_PREFLIGHT_DIR)/src --target js.browser
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/js/modules/app.js
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/index.html
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/manifest.json
	test -f $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web/assets/js/modules/app.js
	$(TERLAN_WEB_MANIFEST_PREFLIGHT) browser $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web
	$(TERLAN_BOOTSTRAP_COMPILER) serve $(BROWSER_PACKAGE_PREFLIGHT_DIR)/_build/web --check

js-stdlib-smoke-check:
	$(TERLC) test std/js/StringTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/ArrayTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/MapTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/SetTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/dom/DocumentTest.terl --target js
	$(TERLC) --target-profile js.browser test std/js/dom/HTMLElementTest.terl --target js

static-profile-preflight:
	rm -rf $(STATIC_PROFILE_PREFLIGHT_DIR)
	$(TERLC) init $(STATIC_PROFILE_PREFLIGHT_DIR) --profile static
	$(TERLC) static emit $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --validate-output --base-path /static-preflight
	test -f $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web/index.html
	grep -F '<base href="/static-preflight/">' $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web/index.html
	$(TERLC) static check $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --base-path /static-preflight
	$(TERLC) static serve $(STATIC_PROFILE_PREFLIGHT_DIR)/src/terlan_static_preflight/Site.terl --out-dir $(STATIC_PROFILE_PREFLIGHT_DIR)/_build/web --validate-output --base-path /static-preflight --check

static-docs-check:
	rm -rf $(STATIC_DOCS_PREFLIGHT_DIR)
	$(TERLC) init $(STATIC_DOCS_PREFLIGHT_DIR) --profile static
	mkdir -p $(STATIC_DOCS_PREFLIGHT_DIR)/content/guides $(STATIC_DOCS_PREFLIGHT_DIR)/content/api
	printf '%s\n' 'module terlan_static_docs_preflight.Site.' '' 'import css "../../assets/site.css" as SiteCss.' 'import file "../../assets/logo.txt" as Logo.' 'import file "../../assets/site.terl.json" as SiteJson.' 'import file "../../assets/deploy.terl.yaml" as DeployYaml.' 'import file "../../assets/config.terl.toml" as ConfigToml.' 'import markdown "../../content/index.terl.md" as HomeContent.' 'import markdown "../../content/guides/install.terl.md" as InstallContent.' 'import markdown "../../content/api/router.terl.md" as RouterContent.' '' 'template Layout from "../../templates/layout.terl.html" {' '    title: String' '}.' > $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl
	printf '%s\n' 'main { max-width: 72rem; }' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/site.css
	printf '%s\n' 'terlan docs' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/logo.txt
	printf '%s\n' '{"name": "terlan", "version": $${version}}' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/site.terl.json
	printf '%s\n' 'site:' '  name: $${name}' '  deploy: github-pages' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/deploy.terl.yaml
	printf '%s\n' 'name = $${name}' 'target = "github-pages"' > $(STATIC_DOCS_PREFLIGHT_DIR)/assets/config.terl.toml
	printf '%s\n' '@page { title = "Install", layout = "Layout" }' '' '# Install' '' 'Run `terlc init docs --profile static`.' > $(STATIC_DOCS_PREFLIGHT_DIR)/content/guides/install.terl.md
	printf '%s\n' '@page { title = "Router", layout = "Layout" }' '' '# Router' '' 'Static docs can describe typed routes.' > $(STATIC_DOCS_PREFLIGHT_DIR)/content/api/router.terl.md
	$(TERLC) static emit $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl --out-dir $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web --validate-output --base-path /terlan
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/guides/install/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/api/router/index.html
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.css
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/logo.txt
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.terl.json
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/deploy.terl.yaml
	test -f $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/config.terl.toml
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/index.html
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/guides/install/index.html
	grep -F '<base href="/terlan/">' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/api/router/index.html
	grep -F 'main { max-width: 72rem; }' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.css
	grep -F 'terlan docs' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/logo.txt
	grep -F '"version": $${version}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/site.terl.json
	grep -F 'name: $${name}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/deploy.terl.yaml
	grep -F 'name = $${name}' $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web/config.terl.toml
	$(TERLC) static check $(STATIC_DOCS_PREFLIGHT_DIR)/src/terlan_static_docs_preflight/Site.terl --out-dir $(STATIC_DOCS_PREFLIGHT_DIR)/_build/web --base-path /terlan

static-command-check: static-route-boundary-check
	$(TERLC_EXACT_TEST) commands::static_site::mod_test::static_check_args_adds_check_and_validation_flags -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::mod_test::static_check_args_preserves_existing_flags -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_nested_content_path -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_index_content_paths -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_infer_generated_relative_content_imports -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_default_title_from_first_heading -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_prefer_explicit_title_over_heading -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_use_page_route_override -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_reject_duplicate_paths -- --exact
	$(TERLC_EXACT_TEST) commands::static_site::routes::routes_test::markdown_static_routes_reject_parent_directory_segments -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::run_cli_static_emit_accepts_out_dir_after_source_path -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::run_cli_static_check_accepts_out_dir_after_source_path -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::parse_static_routes_text_accepts_compact_singular_route -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::parse_static_routes_text_accepts_compact_route_block -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_injects_base_path_when_requested -- --exact

web-profile-preflight: | terlan-web-manifest-preflight-bootstrap
	rm -rf $(WEB_PROFILE_PREFLIGHT_DIR)
	$(TERLAN_BOOTSTRAP_COMPILER) init $(WEB_PROFILE_PREFLIGHT_DIR) --profile web
	$(TERLAN_BOOTSTRAP_COMPILER) --target-profile js.browser --out-dir $(WEB_PROFILE_PREFLIGHT_DIR)/_build build $(WEB_PROFILE_PREFLIGHT_DIR) --target js.browser
	test -f $(WEB_PROFILE_PREFLIGHT_DIR)/_build/js/manifest.json
	test -f $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web/manifest.json
	$(TERLAN_WEB_MANIFEST_PREFLIGHT) web $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web/manifest.json
	$(TERLAN_BOOTSTRAP_COMPILER) serve $(WEB_PROFILE_PREFLIGHT_DIR)/_build/web --check

serve-static-smoke: static-profile-preflight

serve-web-smoke: web-profile-preflight

http-router-check:
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers_from_modules_extracts_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers_from_modules_extracts_receiver_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers_from_modules_extracts_grouped_router_builder_calls -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::route_fixtures::write_browser_package_serializes_discovered_router_handlers -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::route_fixtures::discover_web_error_handler_from_modules_extracts_router_error_handler -- --exact
	$(TERLC_EXACT_TEST) commands::build::js_browser::js_browser_test::asset_and_response_manifests::write_browser_package_serializes_router_error_handler -- --exact

http-observability-check:
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_handler_log_line_includes_handler_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_handler_log_line_includes_optional_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_static_log_line_includes_asset_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_static_route_log_line_includes_route_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_file_route_log_line_includes_route_and_file_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_dev_error_page_includes_escaped_handler_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::render_dev_error_page_omits_absent_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::build_http_response_preserves_server_response_contract -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::build_http_response_appends_validated_dynamic_headers -- --exact

http-tls-check:
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_accepts_absent_server_tls_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_auto_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_internal_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_parses_server_tls_manual_config -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_auto_without_domains -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_auto_manual_or_internal_fields -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_internal_with_public_fields -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_manual_acme_provider -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_manual_without_key -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_server_tls_without_mode -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_accepts_adjacent_project_manifest_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_invalid_adjacent_project_manifest_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_missing_manual_tls_files -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_missing_manual_tls_ca_file -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_manual_tls_paths_outside_project -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_returns_none_for_plain_http_package -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_rejects_invalid_manual_tls_files -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_accepts_manual_certificate_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_accepts_internal_local_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_runtime_plan_defaults_to_lets_encrypt_production -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_runtime_plan_preserves_zerossl_fallback_provider -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_domain_identifiers_preserve_dns_names -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_domain_identifiers_reject_empty_domains -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_contact_strings_wrap_optional_email -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::serve_live_acme_issuance_starts_vm_worker_lane -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::pending_http01_challenges_select_pending_http_challenges -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::pending_http01_challenges_skip_valid_authorizations -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::pending_http01_challenges_reject_missing_http01 -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::generate_acme_csr_returns_der_and_private_key_pem -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::issue_acme_certificate_cache_rejects_zerossl_before_network -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_rejects_zerossl_primary_before_cache_loading -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_account_credentials_round_trip_through_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_account_credentials_cache_reports_invalid_json -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_http01_challenge_cache_writes_valid_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::acme_http01_challenge_cache_rejects_invalid_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::acme_certificate_cache_write_feeds_runtime_tls_config -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_accepts_auto_tls_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::acme_runtime_tls_config_accepts_local_mock_issuer_cache_handoff -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::acme_runtime_tls_config_rejects_local_mock_issuer_without_cache_handoff -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_rejects_auto_tls_cache_without_renewal_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_rejects_malformed_auto_tls_certificate_cache_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_rejects_future_dated_auto_tls_certificate_cache_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_rejects_stale_auto_tls_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::cache_custody::runtime_tls_config_rejects_auto_tls_without_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_serves_acme_http01_challenge_from_auto_tls_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_serves_acme_http01_head_without_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_returns_404_for_missing_acme_http01_challenge -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_invalid_acme_http01_token -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_keeps_acme_like_static_files_for_plain_http_package -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::run_live_serve_rejects_auto_tls_without_certificate_cache -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::serve_web_package_rejects_auto_tls_without_certificate_cache -- --exact

http-acme-live-check:
	target/debug/terlc test scripts/self_validation/HttpAcmeLiveTest.terl

web-compose-check:
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_accepts_postgres_dev_service -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_accepts_long_loopback_postgres_port -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_accepts_list_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_empty_map_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_empty_list_form_postgres_environment -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_malformed_yaml -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_missing_postgres_service -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_disabled_postgres_healthcheck -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_postgres_healthcheck_without_test -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_postgres_healthcheck_none_test -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_postgres_without_healthcheck -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::validate_project_compose_rejects_public_postgres_port_binding -- --exact
	$(TERLC_EXACT_TEST) commands::dev_dependencies::compose_test::docker_compose_up_command_targets_postgres_service_only -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_accepts_adjacent_postgres_compose -- --exact
	$(TERLC_EXACT_TEST) commands::serve::manifest::manifest_test::validate_web_package_rejects_invalid_adjacent_postgres_compose -- --exact

template-contract-check: html-boundary-check
	$(RUST_TEST) -p terlan --lib commands::artifacts::artifacts_test::collect_syntax_
	$(RUST_TEST) -p terlan --lib validation::template_contract::
	$(RUST_TEST) -p terlan --lib commands::static_site::render::render_test::
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_accepts_template_html_route_return_type -- --exact

artifact-template-check:
	$(RUST_TEST) -p terlan --lib html::artifact::artifact_test::
	$(RUST_TEST) -p terlan --lib html::structured::structured_test::
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_copies_valid_json_artifact_template_asset -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_rejects_invalid_json_artifact_template_asset -- --exact

typed-template-interpolation-check: typed-template-interpolation-vm-check typed-template-interpolation-js-check

typed-template-interpolation-vm-check: compiler-purity-metadata-check template-contract-check
	$(TERLC) test \
		std/template/TemplateTest.terl \
		tests/fixtures/purity_template/PurityTemplateTest.terl \
		tests/template/TypedTemplateRuntimeTest.terl
	$(TERLC_EXACT_TEST) compiler::typeck::core_expr_test::syntax_output_lowering_to_core_template_call_expr -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_template_backed_http_handler -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::http::template_response_target_test::
	$(RUST_TEST) -p terlan --lib runtime::vm::http_session::http_session_live_template_response_test::
	$(RUST_TEST) -p terlan --lib runtime::vm::live_template_protocol::live_template_protocol_test::

typed-template-interpolation-js-check: template-contract-check
	$(TERLC_EXACT_TEST) commands::emit_js::template_runtime::template_runtime_test::generated_js_template_runtime_renders_and_rejects_typed_slots -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::template_runtime::template_runtime_test::generated_js_template_runtime_matches_vm_shared_fixture_corpus -- --exact
	$(TERLC_EXACT_TEST) commands::fmt::fmt_test::fmt_command_formats_nested_template_interpolation -- --exact
	$(RUST_TEST) -p terlan --features editor-lsp --lib template_completion_test::
	node editors/vscode/test/textmate_bridge_test.js

typed-template-interpolation-tooling-check: typed-template-interpolation-check

typed-template-interpolation-backend-check: artifact-template-check template-contract-check
	$(TERLC) test std/template/TemplateInterpolationBackendTest.terl
	$(TERLC_EXACT_TEST) commands::emit_js::template_runtime::template_runtime_test::generated_js_template_runtime_matches_vm_shared_fixture_corpus -- --exact
	$(RUST_TEST) -p terlan --lib html::structured_render::structured_render_test::
	$(RUST_TEST) -p terlan --lib commands::static_site::render::render_attribute_test::

function-language-surface-check: typed-template-interpolation-tooling-check repeated-let-syntax-check

repeated-let-syntax-check: tree-sitter-cli-check
	$(TERLC) test tests/language/RepeatedLetSyntaxTest.terl

comprehension-guards-check: compiler-purity-metadata-check tree-sitter-cli-check
	$(TERLC) test \
		std/core/GuardResultTest.terl \
		tests/language/ComprehensionGuardsTest.terl
	$(TERLC_EXACT_TEST) formal_pipeline::formal_pipeline_test::persistence_and_effect_interfaces::embedded_std_interfaces_include_core_guard_result_contract -- --exact
	$(RUST_TEST) -p terlan --lib comprehension
	$(RUST_TEST) -p terlan --lib --features editor-lsp comprehension
	@if output=$$($(TERLC) test tests/language/EffectfulComprehensionFailureTest.terl --name propagates_typed_guard_failure 2>&1); then \
		echo "expected typed guard failure" >&2; exit 1; \
	else echo "$$output" | grep -F 'error[vm_comprehension_guard_failed]'; fi
	@if output=$$($(TERLC) test tests/language/EffectfulComprehensionFailureTest.terl --name propagates_guard_cancellation 2>&1); then \
		echo "expected guard cancellation" >&2; exit 1; \
	else echo "$$output" | grep -F 'error[vm_comprehension_guard_cancelled]'; fi

flexible-shape-guards-check: compiler-purity-metadata-check pattern-matching-support-check
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::literals_and_comprehensions::formal_keyword_exprs_preserve_clause_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::literals_and_comprehensions::formal_keyword_exprs_reject_when_clause_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::formal_expr_parses_range_membership_with_range_precedence -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_case_guard_trees -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_function_clause_guard_trees -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_accepts_function_clause_where_guard_trees -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_preserves_case_guards_with_where -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_canonicalizes_function_head_guards_to_where -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_infers_range_membership_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_rejects_non_integer_range_bounds -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_rejects_non_range_membership_target -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_function_guards_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_where_guards_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_non_bool_case_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_non_bool_function_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_impure_case_guard_assignment -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_impure_case_guard_template_call -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_impure_function_guard_assignment -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_impure_function_guard_template_call -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::core_control_flow_test::syntax_output_lowering_to_core_records_case_core_expr_with_guard -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_handles_guarded_case_expr -- --exact

private-field-check:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_accepts_local_private_struct_field_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_accepts_local_private_struct_field_update -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_accepts_local_private_struct_field_pattern -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_rejects_bare_access_to_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_rejects_bare_update_to_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_rejects_bare_pattern_for_private_struct_field -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::import_test::visibility_and_function_values::syntax_output_rejects_imported_private_struct_field_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::import_test::visibility_and_function_values::syntax_output_rejects_imported_private_struct_field_update -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::import_test::visibility_and_function_values::syntax_output_rejects_imported_private_struct_field_pattern -- --exact

db-command-check:
	target/debug/terlc test scripts/self_validation/DbCommandBoundaryTest.terl
	$(RUST_TEST) -p terlan --lib commands::db::
	$(RUST_TEST) -p terlan --lib runtime::vm::postgres
	$(RUST_TEST) -p terlan --lib source_postgres

native-boundary-postgres-check:
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::config_defaults_are_stable -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::config_builders_set_pool_limits_and_timeouts -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::validate_config_rejects_invalid_pool_settings -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::query_operations_reject_empty_sql_before_adapter_dispatch -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::row_accessors_decode_matching_values -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::row_accessors_keep_dynamic_column_and_enum_text_as_strings -- --exact
	$(TERLC_EXACT_TEST) runtime::native::postgres_test::row_accessors_report_missing_and_type_errors -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::dispatch_postgres_connect_preserves_adapter_error_codes -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::dispatch_postgres_query_operations_are_known_driver_operations -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::dispatch_postgres_transaction_requires_runtime_bridge -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::dispatch_postgres_row_accessors_decode_values -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::bridge_dispatch_postgres_row_handles_decode_values -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::bridge_dispatch_postgres_row_keeps_atom_like_columns_as_text -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::dispatch::dispatch_test::typed_handle_operations::bridge_dispatch_postgres_pool_handles_reach_query_adapter -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_disposes_postgres_pool_and_row_resources -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_returns_postgres_typed_errors_without_live_database -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_rejects_postgres_empty_sql_before_driver_dispatch -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_dispatches_postgres_sql_operations_to_adapter -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_decodes_postgres_row_columns_through_handles -- --exact

native-boundary-http-cookie-check:
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::request_cookies_returns_mutable_cookie_jar -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::request_cookie_header_parser_splits_request_cookie_pairs -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::request_cookie_header_parser_ignores_malformed_segments -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::request_cookie_header_parser_preserves_duplicates_and_quoted_values -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::cookie_set_header_serializes_supported_attributes -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::cookie_set_header_with_options_serializes_full_option_surface -- --exact
	$(TERLC_EXACT_TEST) runtime::native::http::http_test::cookie_delete_header_serializes_expiring_cookie -- --exact
	$(TERLC_EXACT_TEST) runtime::native_boundary::runtime::runtime_test::runtime_executes_http_cookie_jar_operations_through_terms -- --exact

native-boundary-postgres-docker-check:
	$(CARGO) test -p terlan-libpq --all-targets
	$(RUST_TEST) -p terlan --lib \
		commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::
	$(TERLC_EXACT_TEST) \
		runtime::vm::postgres::libpq_worker::libpq_docker_gate_test::libpq_docker_gate_validates_success_failure_cancellation_and_cleanup \
		-- --ignored --exact
	$(TERLC_EXACT_TEST) \
		runtime::vm::source_postgres::source_postgres_docker_gate_test::source_postgres_docker_gate_validates_transaction_callbacks \
		-- --ignored --exact
	$(TERLC_EXACT_TEST) \
		commands::db::live_test::run_db_migration_and_snapshot_lifecycle_against_docker_postgres \
		-- --ignored --exact

repl-check:

sql-form-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/SqlFormBoundaryTest.terl
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_typed_sql_raw_macro_expr_parses_result_type -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_typed_sql_raw_macro_expr_parses_interpolation_expressions -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_typed_sql_raw_macro_expr_rejects_bad_interpolation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_typed_sql_raw_macro_expr_ignores_comment_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_adversarial_test::tests::formal_typed_sql_raw_macro_expr_ignores_dollar_quoted_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_adversarial_test::tests::formal_typed_sql_raw_macro_expr_ignores_nested_comment_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_typed_sql_raw_macro_expr_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_typed_sql_interpolation_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_ignores_typed_sql_comment_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_ignores_typed_sql_dollar_quoted_interpolation_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::infers_select_limit_one_as_optional_one -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::infers_cte_select_from_postgres_ast -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::infers_fetch_first_one_as_optional_one -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::keeps_dynamic_limit_and_fetch_with_ties_as_many_rows -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::classifies_postgres_statement_kinds_from_ast -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::infers_transaction_requirements_from_postgres_ast -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::builds_locking_query_with_active_transaction_requirement -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::rejects_vm_owned_transaction_control_from_wrapper_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::derives_aliased_expression_and_cte_projection_fields_from_ast -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::rejects_duplicate_projection_output_names -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::folds_unquoted_projection_names_before_duplicate_validation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::rejects_duplicate_unqualified_names_from_compound_columns -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::infers_mutating_statement_without_returning_as_affected_rows -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::rewrites_interpolations_to_postgres_placeholders_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::leaves_parameter_text_inside_postgres_dollar_quotes_untouched -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::analyzes_dollar_quoted_sql_without_parameter_drift -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::rejects_explicit_postgres_placeholders -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::reports_ready_sql_wrapper_lowering_front_door -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::builds_ready_sql_wrapper_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::sql_macro_validation_rejects_malformed_postgres_syntax -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::sql_macro_validation_rejects_multiple_statements -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::sql_macro_validation_rejects_comment_only_forms -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_forms_test::sql_macro_validation_keeps_injection_shaped_values_parameterized -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_parameter_type_test::sql_parameter_types_accept_runtime_scalar_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_parameter_type_test::sql_parameter_types_reject_structured_dynamic_and_nullable_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_parameter_type_test::sql_parameter_type_diagnostic_preserves_interpolation_index_and_span -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_accepts_scalar_tuple_and_infers_result_type -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_accepts_transparent_scalar_tuple_alias -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_rejects_tuple_projection_arity_mismatch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_rejects_non_decodable_structural_fields -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_accepts_nullable_scalar_tuple_and_struct_fields -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::sql_row_descriptor_test::sql_row_descriptor_rejects_nullable_structured_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::sql_macro_validation_reports_duplicate_projection_name -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::sql_macro_validation_rejects_explicit_postgres_placeholders -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_rejects_raw_macro_expr_without_macro_resolution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::sql_macro_validation_reports_stable_malformed_sql_diagnostic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::sql_macro_validation_rejects_vm_owned_transaction_control -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_rejects_sql_projection_field_not_on_row_struct -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_uses_sql_wrapper_result_type_for_return_checking -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_records_sql_query_payload -- --exact

sql-runtime-check:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_records_sql_query_payload -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::decode_projection_preserves_order -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::decode_params_accepts_arrays_and_rejects_non_arrays -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::decode_params_rejects_malformed_json -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::encode_decoded_value_serializes_supported_scalar_values -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::encode_decoded_value_serializes_json_and_null_values -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::run_inner_rejects_unsupported_operation_before_database_lookup -- --exact
	$(TERLC_EXACT_TEST) commands::sql_runtime::sql_runtime_test::malformed_invocation_returns_error_protocol -- --exact

api-schema-check:
	$(TERLC_EXACT_TEST) compiler::api_contract::api_contract_test::router_source_contract_extracts_routes -- --exact
	$(TERLC_EXACT_TEST) compiler::api_contract::api_contract_test::router_source_contract_projects_to_openapi_paths -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_emit_from_source_writes_route_openapi_paths -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_import_generates_client_module_and_skip_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::api::mod_test::api_import_records_unsupported_operation_skips -- --exact

runtime-release-dependency-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) runtime-release-dependency-check

formal-cli-phase-contract-gate:
	$(TERLC_EXACT_TEST) tests::run_phase_contract_fixtures_match_golden -- --exact
	$(TERLC_EXACT_TEST) tests::interface_test::run_interface_success_and_error_paths -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact

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
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_legacy_target_package_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_legacy_beam_thin_artifact_kind -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_registry_dependency_in_local_scope -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_wrong_target_dependency_source -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_dependency_without_version -- --exact
	$(TERLC_EXACT_TEST) commands::build::project_manifest::project_manifest_test::project_manifest_rejects_unsupported_section -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_emits_terlan_vm_artifact_without_erlang_or_beam -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_defaults_to_terlan_vm_artifact_without_erlang_or_beam -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_defaults_project_directory_to_terlan_vm_artifacts -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_manifest_before_silent_directory_scan -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_compiles_project_manifest_source_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_rejects_project_source_outside_package_root -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_vm_artifact_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_vm_artifact_test::build_command_compiles_project_receiver_method_entrypoint -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::project_layout_test::build_command_accepts_project_manifest_multiple_source_roots_vm_import_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_accepts_project_with_local_path_dependency_vm_import_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_without_manifest -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_local_path_dependency_cycle -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_legacy_target_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_npm_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::dependency_test::build_command_rejects_cargo_dependency_metadata_before_emission -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_accepts_directory_with_imported_constructors_and_aliases_vm_import_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_accepts_directory_with_aliased_imported_alias_patterns_vm_import_closure -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::import_constructor_test::build_command_accepts_directory_with_aliased_imported_alias_constructor_chain_vm_import_closure -- --exact

formal-cli-a0-54-constructor-contract-gate:
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_public_constructor_private_return_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_vm_artifact_test::build_command_compiles_project_explicit_constructor_entrypoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::variance_and_visibility::syntax_output_rejects_public_constructor_returning_private_type -- --exact

formal-cli-a0-55-function-clause-contract-gate:
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_function_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_records_function_clause_summaries -- --exact

formal-cli-a0-56-primary-expression-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_macro_expr_parses_as_primary_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_raw_macro_expr_requires_immediate_raw_block -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_expr_test::macros_and_constructors::formal_constructor_chain_expr_parses_with_record_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_sequence_primary_expr_trees -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_raw_macro_primary_before_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact

formal-cli-a0-57-keyword-expression-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_allows_keyword_expressions_in_operator_chains -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_if_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_try_expression_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_checks_if_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_checks_try_expr_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_supports_try_after_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_control_flow_test::syntax_output_lowering_to_core_records_if_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_control_flow_test::syntax_output_lowering_to_core_records_try_core_expr -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_receive_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_try_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_quote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_unquote_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile -- --exact

formal-cli-a0-58-calls-and-references-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_infers_local_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::assignment_templates_and_html::syntax_output_infers_field_access_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_records_local_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_records_function_value_call_core_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_typechecks_pipe_into_function_value_call -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_index_expr_uses_index_get_call -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_field_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_marks_remote_call_proof_model_required -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_rejects_remote_fun_ref_source_syntax -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::receiver_method_test::resolution_and_defaults::syntax_output_resolves_local_receiver_method_calls_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::receiver_method_test::dispatch_and_identity::syntax_output_rejects_duplicate_receiver_method_identity_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::receiver_method_test::dispatch_and_identity::syntax_output_rejects_receiver_methods_for_imported_owner_on_formal_path -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_accepts_fun_call_for_a0_16_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_keeps_fun_call_out_of_a0_15_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_accepts_qualified_calls_for_a0_20_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_keeps_qualified_calls_out_of_a0_19_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_rejects_method_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::executable_vm_artifact_test::build_command_compiles_project_receiver_method_entrypoint -- --exact

formal-cli-a0-59-data-form-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_rejects_erlang_binary_segment_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_record_suffix_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_binds_list_comprehension_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_rejects_list_comprehension_non_list_source_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_binary_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_map_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_list_cons_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_fixed_array_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_list_comprehension_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_record_construct_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_record_access_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_record_update_expr -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_accepts_lists_for_a0_7_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_keeps_lists_out_of_a0_6_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_accepts_binary_for_a0_8_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_keeps_binary_out_of_a0_7_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_accepts_list_cons_for_a0_9_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_keeps_list_cons_out_of_a0_8_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_accepts_multi_generator_list_comprehension_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact

formal-cli-a0-60-pattern-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_recursive_expression_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::expression_trees::syntax_output_includes_case_guard_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_includes_list_cons_expr_and_pattern_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::chain_identity::syntax_output_declared_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_list_cons_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_binds_case_constructor_patterns_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_records_record_pattern_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_pattern_coverage_includes_float_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_pattern_coverage_includes_map_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_includes_list_cons_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_requires_covered_tuple_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_requires_covered_list_children -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_requires_covered_constructor_args -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_requires_map_field_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_pattern_coverage_includes_compat_wildcards -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::chain_identity::syntax_output_lowering_to_core_resolves_declared_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::chain_identity::syntax_output_lowering_to_core_resolves_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::chain_identity::syntax_output_lowering_to_core_resolves_aliased_imported_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::call_and_pattern_identity::syntax_output_lowering_to_core_resolves_local_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::call_and_pattern_identity::syntax_output_lowering_to_core_resolves_direct_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::call_and_pattern_identity::syntax_output_lowering_to_core_resolves_imported_alias_constructor_pattern_identity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_control_flow_test::syntax_output_lowering_to_core_case_with_record_pattern_requires_proof_model -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_accepts_constructor_pattern_for_a0_13_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_map_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_record_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_float_pattern_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::profile_inference_and_collections::run_check_single_file_rejects_guarded_case_for_core_v0_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::constructor_errors::run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::alias_chain_errors::run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_error_manifest_test::alias_chain_errors::run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_declared_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_identity::run_check_single_file_accepts_alias_constructor_pattern_in_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_constructor_identity_manifest_test::constructor_chains::run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase -- --exact

formal-cli-a0-61-lexical-and-name-contract-gate:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_pattern_test::tests::formal_atom_literal_patterns_are_literal_patterns -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_pattern_test::tests::parses_nullary_constructor_pattern_call -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_quoted_atom_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_normalizes_prefixed_integer_literals -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_marks_constructor_pattern_candidates -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_keeps_constructor_call_candidates_as_named_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::import_test::alias_imports::syntax_output_imported_literal_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_literal_aliases_compare_with_literal_values_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::import_test::alias_imports::syntax_output_imported_literal_alias_constructor_calls_are_rejected_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::constructor_test::formal_path_aliases::syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_records_compound_core_type_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_lowering_test::pattern_coverage::syntax_output_lowering_to_core_records_type_decl_core_body_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_float_literal -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_expr_test::syntax_output_lowering_to_core_binary_literal -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_accepts_raw_atoms_for_a0_5_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_keeps_raw_atoms_out_of_a0_4_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_accepts_named_call_for_a0_10_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::early_profiles::run_check_single_file_keeps_named_call_out_of_a0_9_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_accepts_constructor_call_for_a0_12_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_keeps_constructor_call_out_of_a0_11_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_accepts_constructor_pattern_for_a0_13_vm_target_profile -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_progression_test::constructor_and_lambda_profiles::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_vm_target_profile -- --exact

formal-cli-js-gate:
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_uses_core_function_exports -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_integer_division -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::core_lowering_test::emit_core_module_to_js_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_js_with_oxc_codegen_reprints_module_source -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_emits_core_surface -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_minimal_direct_oxc_ast_module_prints_export -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_arithmetic_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_integer_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_float_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_string_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_bool_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_total_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_falls_back_for_partial_if_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_integer_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_float_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_bool_literal_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_falls_back_for_partial_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_handles_guarded_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_handles_destructuring_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_lambda_value -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_simple_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_direct_oxc_ast_handles_destructuring_list_comprehension -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_falls_back_for_remote_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::direct_core_lowering::emit_core_module_with_oxc_codegen_rejects_remote_fun_ref_source_syntax -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_constructor_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_try_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_quote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_array_like_literals -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_unary_negation -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_list_cons -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_oxc_codegen_falls_back_for_index_trait_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_map_literal -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_field_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_record_construct -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_record_access -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::record_template_emit_test::emit_core_module_with_direct_oxc_ast_handles_record_update -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::record_template_emit_test::emit_core_module_with_direct_oxc_ast_handles_template_instantiate -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_binary_operator_set -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_direct_oxc_ast_handles_pipe_forward_to_named_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::fallback_lowering::emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_direct_oxc_ast_handles_string_starts_with_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_direct_oxc_ast_handles_string_length_intrinsic -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_oxc_codegen_emits_named_call_private_helper -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_direct_oxc_ast_ignores_unreachable_private_function -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_oxc_codegen_uses_direct_reachability_filter -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::emit_core_module_with_oxc_codegen_falls_back_for_binding_case_expr -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::emit_js_test::intrinsics_and_declarations::declarations_test::emit_core_module_to_typescript_declarations_uses_core_surface -- --exact
	$(TERLC_EXACT_TEST) tests::emit_js_test::run_emit_js_reports_errors -- --exact
	$(TERLC_EXACT_TEST) tests::emit_js_test::run_emit_js_writes_js_and_declarations -- --exact

formal-cli-rust-gate:
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_uses_core_function_visibility -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_pipe_forward_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_handles_function_value_call -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_contains_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_starts_with_intrinsic_probe -- --exact
	$(TERLC_EXACT_TEST) commands::emit_rust::emit_rust_test::emit_core_module_to_rust_compiles_string_length_intrinsic_probe -- --exact

formal-cli-doc-gate:
	$(TERLC_EXACT_TEST) tests::doc_test::formal_doc_markdown_generates_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::doc_test::formal_doctest_compiles_terlan_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_external_template_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_html_blocks_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_inline_template_components_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_emit_renders_markdown_html_from_syntax_output -- --exact
	$(TERLC_EXACT_TEST) tests::static_site_test::formal_static_syntax_output_discovers_entrypoints_and_routes -- --exact

formal-cli-a0-50-template-frontend-gate:
	$(TERLC_EXACT_TEST) commands::artifacts::artifacts_test::collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_unresolved_template_body_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact

formal-cli-a0-62-template-boundary-contract-gate: formal-cli-a0-50-template-frontend-gate
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_map_constructor_record_and_template_field_trees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::syntax_output::syntax_output_decl_test::annotation_validation_and_methods::syntax_output_includes_struct_constructor_trait_and_template_signatures -- --exact
	$(TERLC_EXACT_TEST) tests::check_language_feature_rejection_test::run_check_single_file_rejects_unresolved_template_body_before_core_phase -- --exact
	$(TERLC_EXACT_TEST) tests::check_target_profile_gate_test::core_shape_rejections::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact

formal-incremental-gate:
	$(TERLC_EXACT_TEST) tests::check_phase_test::run_check_dir_rejects_module_layout_mismatch -- --exact
	$(TERLC_EXACT_TEST) tests::check_incremental_test::run_check_dir_incremental_dependency_closure -- --exact
	$(TERLC_EXACT_TEST) tests::check_incremental_test::run_check_dir_incremental_with_trait_interfaces -- --exact

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
	$(TERLC) build tests/fixtures/mathx.terl --out-dir "$${out1}"; \
	$(TERLC) build tests/fixtures/mathx.terl --out-dir "$${out2}"; \
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
