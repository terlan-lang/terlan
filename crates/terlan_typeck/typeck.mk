# Terlan typechecker compiler-path validation targets.
#
# Kept with the typechecker crate to keep typecheck checks near the owning module.

CARGO ?= cargo
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
T_TYPECK_CLI_EXACT_TEST := $(EXACT_CARGO_TEST) -p terlan_cli

.PHONY: typeck-help formal-typecheck-gate formal-typecheck-a0-29-implements-gate formal-typecheck-a0-30-cast-gate formal-typecheck-a0-53-struct-authority-gate formal-core-proof-gate

typeck-help:
	@echo "  make formal-typecheck-gate - run syntax-output typechecker regressions"
	@echo "  make formal-typecheck-a0-29-implements-gate - run A0.29 trait conformance semantic regressions"
	@echo "  make formal-typecheck-a0-30-cast-gate - run A0.30 cast conversion-boundary diagnostics"
	@echo "  make formal-typecheck-a0-53-struct-authority-gate - run A0.53 struct construction authority diagnostics"
	@echo "  make formal-core-proof-gate - run CoreIR proof/profile artifact regressions"

# Intentionally broad: this gate tracks every syntax-output typechecker regression.
formal-typecheck-gate:
	$(CARGO) test -p terlan_typeck syntax_output

formal-typecheck-a0-29-implements-gate:
	$(CARGO) test -p terlan_typeck declared_implements -- --nocapture
	$(CARGO) test -p terlan_typeck explicit_trait_impl -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_uses_generic_bounds_for_trait_method_dispatch_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_preserves_trait_conformance_facts -- --exact

formal-typecheck-a0-30-cast-gate:
	$(EXACT_CARGO_TEST) -p terlan_typeck diagnostic_test::syntax_output_rejects_cast_before_conversion_resolution_on_formal_path -- --exact

formal-typecheck-a0-53-struct-authority-gate:
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_lowering_to_core_record_construct_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_typeck tests::syntax_output_rejects_raw_imported_struct_construction_without_constructor -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact

# Intentionally broad: the suite filters in this gate track all CoreIR
# lowering, proof-manifest, proof-baseline, and target-profile regressions.
formal-core-proof-gate:
	$(CARGO) test -p terlan_typeck syntax_output_lowering_to_core
	$(CARGO) test -p terlan_cli phase_manifest_core_proof_coverage
	$(CARGO) test -p terlan_cli proof_baseline_
	$(CARGO) test -p terlan_cli target_profile_
	$(T_TYPECK_CLI_EXACT_TEST) tests::parse_args_accepts_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_success_emits_core_phase_ok -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_subtraction_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_map_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_map_pattern_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_record_pattern_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_float_pattern_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_float_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_fixed_array_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_index_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_receive_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_try_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_remote_call_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_record_construct_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_record_access_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_record_update_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_struct_only_emits_typed_struct_body_manifest -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_lambda_emits_runtime_binding_freshness_manifest -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_imported_constructor_call_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_aliased_imported_constructor_call_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_imported_constructor_pattern_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_imported_constructor_chain_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_aliased_imported_constructor_chain_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_single_file_accepts_declared_constructor_pattern_in_core_phase -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_phase_contract_lean_conformance_baselines_are_lean_covered -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_phase_contract_next_lean_model_candidates_are_pinned -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_phase_contract_lean_conformance_baselines_emit_manifest_evidence -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_check_phase_contract_next_lean_model_candidates_emit_manifest_evidence -- --exact
	$(T_TYPECK_CLI_EXACT_TEST) tests::run_phase_contract_fixtures_match_golden -- --exact
