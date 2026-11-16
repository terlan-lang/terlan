# ---- Language-feature completeness and migration checks ----

# Validates pattern-matching feature support.
pattern-matching-support-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools pattern_matching_support_test
	$(TERLAN_QUALITY) pattern-matching-support
	$(TERLC) test tests/pattern/PatternMatchingTest.terl


# Validates string capture pattern parsing, typing, and lowering.
string-pattern-matching-check:
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_string_capture_pattern_in_case_clause -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_string_capture_pattern_in_let_binding -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_string_capture_pattern_in_function_clause -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_string_capture_pattern_in_lambda_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_elixir_style_string_capture_pattern -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_adjacent_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_unterminated_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_empty_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_string_capture_pattern_in_nested_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_marks_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_binds_explicit_type -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_defaults_untyped_capture_to_string -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_rejects_duplicate_capture_names -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_rejects_invalid_capture_annotation -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::core_lowering_test::interface_and_effects::syntax_output_lowering_to_core_pattern_coverage_includes_string_capture_payload -- --exact


# Validates long-tail string pattern cases including tree-sitter grammar.
string-pattern-long-tail-check:
	$(TERLAN_QUALITY) pattern-matching-support
	@tmp_home="$$(mktemp -d)"; trap 'rm -rf "$$tmp_home"' EXIT; HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check && HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check:cli
	$(TERLC) test tests/pattern/StringPatternLongTailTest.terl


# Aggregates binary/bitstring processing checks.
binary-bitstring-processing-check: binary-descriptor-check binary-syntax-scaffold-check binary-error-taxonomy-check binary-protocol-helper-check binary-protocol-benchmark-check vm-tcp-framing-check


# Validates binary layout syntax parsing, typing, and lowering.
binary-syntax-scaffold-check: tree-sitter-package-check tree-sitter-cli-check
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_binary_layout_expression_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::expands_binary_layout_shape_captures_without_rewriting_descriptors -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_structural_arguments_for_binary_layout_shape_captures -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_shape_import_test::project_vm_tests_execute_imported_binary_layout_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_binary_layout_function_head_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_binary_layout_case_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_binary_layout_lambda_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_binary_layout_unknown_endian_policy -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_binary_layout_duplicate_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_binary_layout_non_terminal_rest -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_binary_layout_multiple_rest_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_empty_binary_layout_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_binary_layout_unknown_descriptor -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::parses_binary_layout_unicode_scalar_descriptors -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::literals_and_comprehensions::formal_binary_segments_are_rejected_as_erlang_source_syntax -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_preserves_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_splits_long_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_expr_test::literals_and_control_flow::syntax_output_includes_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_accepts_fixed_integer_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_accepts_exact_bytes_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_accepts_exact_bits_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_accepts_terminal_rest_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_accepts_unicode_binary_layout_constructors -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_non_integer_unicode_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_non_integer_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_non_bytes_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_non_bitstring_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_non_bytes_terminal_rest_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_oversized_binary_layout_integer_width -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_oversized_binary_layout_byte_width -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_rejects_unbound_binary_layout_field_value -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_fixed_integer_binary_layout -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_exact_bytes_binary_layout -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_exact_bits_binary_layout -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_terminal_rest_binary_layout -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_unicode_binary_layouts -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_accepts_typed_binary_layout_case_pattern -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_accepts_typed_binary_layout_function_head_pattern -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::binary_pattern_test::target_profile_allows_binary_pattern_for_vm_profile -- --exact
	$(TERLC_EXACT_TEST) validation::target_profile::target_profile_test::tests::binary_pattern_test::target_profile_rejects_binary_pattern_for_js_profiles -- --exact
	$(TERLC) test \
		tests/binary/BinaryConstructionTest.terl \
		tests/binary/BinaryPatternTest.terl \
		tests/binary/BinaryPropertyTest.terl

	target/debug/terlc check std/binary/Binary.terl
	target/debug/terlc check std/binary/BinaryTest.terl
	target/debug/terlc test \
		std/binary \
		tests/binary/BinaryDynamicSizeTest.terl \
		std/vm/BitStringTest.terl
	target/debug/terlc check std/vm/BitString.terl
	$(TERLC_EXACT_TEST) compiler::typeck::core_intrinsic_test::vm_primitive_registry::vm_bitstring_intrinsics_have_closed_ids_and_return_types -- --exact
	$(TERLC_EXACT_TEST) runtime::vm::bitstring::bitstring_test -- --nocapture
	$(TERLC_EXACT_TEST) runtime::vm::memory::memory_test::limits_and_accounting::memory_logical_value_size_accounts_nested_structural_values_exactly -- --exact
	$(TERLC_EXACT_TEST) runtime::vm::term_format::term_format_runtime_test::tetf_encodes_bitstrings_with_exact_logical_length -- --exact


# Aggregates binary descriptor contract and runtime checks.
binary-descriptor-check: binary-descriptor-contract-check binary-runtime-suite-check


# Validates the binary descriptor contract.
binary-descriptor-contract-check:
	$(TERLAN_QUALITY) binary-descriptor-contract


# Validates the binary error taxonomy via the runtime suite.
binary-error-taxonomy-check: binary-runtime-suite-check


# Validates binary protocol helper functions.
binary-protocol-helper-check: binary-runtime-suite-check
	target/debug/terlc test tests/binary/BinaryProtocolHelperTest.terl


# Benchmarks binary protocol framing performance.
binary-protocol-benchmark-check:
	$(EXACT_CARGO_TEST) -p terlan --lib vm::framing_benchmark::tests::truncated_framing_benchmark_reports_expected_typed_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib vm::framing_benchmark::tests::adversarial_framing_matrix_reports_typed_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib vm::framing_benchmark::tests::framing_workload_parser_rejects_unknown_names -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib vm::framing_benchmark::tests::framing_percentiles_use_nearest_rank_for_tail_samples -- --exact
	$(RUST_TEST) -p terlan --lib --features benchmark-tools binary_protocol::
	TERLAN_PROTOCOL_BENCHMARK_AUTORUN=1 \
	TERLAN_PROTOCOL_BENCHMARK_ALREADY_RUN="$(TERLAN_PROTOCOL_BENCHMARK_ALREADY_RUN)" \
		target/debug/terlc test scripts/benchmarks/protocol/ProtocolBenchmarkTest.terl \
			--bench --warmup 0 --samples 1 --name repository_protocol_benchmark_gate


# Checks the function-head migration diagnostic policy via the quality tool
function-head-migration-diagnostic-policy-check:
	$(TERLAN_QUALITY) function-head-migration-diagnostic-policy


# Lints function-head migration usage via the quality tool
function-head-migration-lint-check: function-head-migration-diagnostic-policy-check
	$(TERLAN_QUALITY) function-head-migration-lint


# Checks the function-head pattern migration assist tooling
function-head-pattern-migration-assist-check: function-head-migration-lint-check
	$(TERLAN_QUALITY) function-head-pattern-migration-assist


# Benchmarks the function-head pattern migration tooling
function-head-pattern-migration-benchmark-check: function-head-pattern-migration-assist-check
	$(TERLAN_QUALITY) function-head-pattern-migration-benchmark


# Checks the function-head pattern migration documentation
function-head-pattern-migration-docs-check: function-head-migration-diagnostic-policy-check function-head-pattern-parameters-check
	$(TERLAN_QUALITY) function-head-pattern-migration-docs


# Validates the function-head pattern 0.0.7 handoff across all migration checks
function-head-pattern-0-0-7-handoff-check: function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-parameters-check function-head-pattern-migration-docs-check
	$(TERLAN_QUALITY) function-head-pattern-handoff


# Hardens function-head pattern parameters on top of the 0.0.7 handoff check
function-head-pattern-parameters-hardening-check: function-head-pattern-0-0-7-handoff-check


# Checks function-head pattern parameter observability and grammar docs
function-head-pattern-parameters-check:
	$(TERLAN_QUALITY) function-head-observability
	grep -F 'pub add({left, right}: {Int, Int}): Int ->' docs/grammar/README.md
	grep -F 'pub describe({status, body}: Dynamic): String.' docs/grammar/README.md
	grep -F 'pub full_name({name, family_name} = user: User): String ->' docs/grammar/README.md


# Checks shape synonym parsing, formatting, expansion, and overlap diagnostics
shape-synonyms-check: shape-implications-check tree-sitter-package-check tree-sitter-cli-check
	$(TERLAN_QUALITY) pattern-matching-support
	grep -F 'shape OkResponse(body)' docs/grammar/README.md
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_pattern_test::tests::parses_nullary_constructor_pattern_call -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_public_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::rejects_lowercase_shape_synonym_declaration -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_interface_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_preserves_shape_synonym_raw_declarations -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::expands_local_shape_calls_in_case_and_function_head_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_local_shape_arity_mismatch -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_local_shape_called_as_runtime_value -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_recursive_local_shape_expansion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_local_shape_parameters_and_names -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_in_shape_bodies -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_created_by_shape_expansion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_distinct_shapes_with_equivalent_case_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_distinct_shapes_with_equivalent_function_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_guarded_or_structurally_distinct_shape_clauses -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_shape_subsumed_by_earlier_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_shape_subsumed_by_earlier_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_specific_shape_before_broad_fallback_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_map_shape_with_stricter_required_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_partially_overlapping_shapes_when_both_are_useful -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_guarded_later_case_shape_shadowed_by_unguarded_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_guarded_later_function_shape_shadowed_by_unguarded_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_guarded_broad_shape_before_unguarded_fallback -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_alpha_equivalent_guarded_case_shapes -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_alpha_equivalent_guarded_function_shapes -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_equivalent_shape_patterns_with_distinct_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_shape_with_contained_integer_range -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_shape_with_stricter_integer_bound -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_equality_guard_contained_by_reversed_integer_bound -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_disjunction_when_every_branch_is_contained -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_range_contained_by_disjunction_of_conjunctions -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_disjunctive_guard_when_later_range_crosses_a_gap -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_guard_implication_beyond_branch_budget_conservatively -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_guard_with_implied_variable_relation -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_guard_with_implied_variable_equality -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_distinct_variable_relations_on_equivalent_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_guard_with_transitive_strict_relation -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_function_guard_with_transitive_equality -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_contradictory_later_relation_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_guard_with_equality_inequality_conflict -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::accepts_non_strict_chain_when_earlier_guard_requires_strict_order -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_case_guard_repeating_predicate_with_stronger_constraint -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_function_guard_implying_predicate_disjunction -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_distinct_local_predicates_with_equivalent_visible_bodies -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_distinct_local_predicate_body_that_implies_earlier_body -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_distinct_predicates_with_call_bearing_bodies_conservatively -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::does_not_use_non_bool_function_bodies_as_predicate_proofs -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_later_predicate_when_earlier_guard_requires_extra_evidence -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_same_predicate_with_distinct_arguments -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_guard_repeating_negated_predicate -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_contradictory_positive_and_negated_predicate_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_double_negation_as_positive_predicate_evidence -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_opposing_predicate_polarities_as_distinct_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_compound_negation_equivalent_to_explicit_de_morgan_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_conjunction_equivalent_to_negative_disjunction -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_compound_negation_contradicted_by_positive_predicate -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_partial_negative_evidence_for_earlier_negative_conjunction -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_comparison_equivalent_to_inverted_operator -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_integer_equality_equivalent_to_inequality -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_integer_inequality_equivalent_to_equality -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_variable_relation_equivalent_to_inverse_relation -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_variable_equality_equivalent_to_inequality -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_reversed_comparison_after_operator_inversion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_inverted_comparison_that_does_not_imply_earlier_range -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shape_overlap_test::accepts_narrow_guard_before_broader_guard_fallback -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::composes_guarded_shape_with_explicit_clause_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::expands_nested_shape_guards_and_substitutes_parameters -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_guard_parameter_substitution_from_non_value_pattern -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::composes_guarded_shape_with_comprehension_filter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::expands_guarded_shape_in_let_pattern_as_case_assertion -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::carries_guarded_shapes_into_grouped_let_success_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::let_else_test::guarded_shape_grouped_let_typechecks_and_lowers_success_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::let_else_test::guarded_shape_grouped_let_keeps_fallback_outside_success_scope -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::gives_each_private_shape_binding_a_distinct_compiler_name -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::substitutes_string_capture_parameters_in_text_and_binding_metadata -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::gives_private_string_captures_compiler_names -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::shapes_test::tests::rejects_non_binding_string_capture_arguments -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_binds_explicit_type -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_rejects_duplicate_capture_names -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::pattern_test::binary_and_collection_patterns::syntax_output_string_capture_pattern_rejects_invalid_capture_annotation -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_adjacent_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_unterminated_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::precedence_and_patterns::rejects_empty_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::shapes_test::imported_shape_expansion_normalizes_selected_alias_and_nested_guard -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::shapes_test::imported_shape_expansion_supports_wildcard_imports -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::shapes_test::imported_shape_expansion_rejects_alias_called_as_runtime_value -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::shapes_test::imported_shape_expansion_rejects_ambiguous_local_aliases -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::shapes_test::imported_shape_expansion_rejects_recursive_provider_shapes -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_shape_import_test::project_vm_tests_execute_selected_imported_shape_alias -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_record_case -- --exact
	$(TERLC_EXACT_TEST) commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_constructor_case -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_literal_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_tuple_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_nested_literal_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_map_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_record_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_constructor_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_imported_zero_arity_constructor_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_emits_guarded_imported_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_accepts_local_unguarded_shape_synonym_after_expansion -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_typechecks_composed_shape_guards -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::diagnostic_test::macros_sql_and_shapes::syntax_output_rejects_non_bool_shape_guard_after_expansion -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::shape_purity_test::syntax_output_rejects_effectful_helper_in_shape_guard_after_expansion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::lib_test::documents_and_shapes::lsp_document_accepts_shape_synonym_declarations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::lib_test::documents_and_shapes::document_symbols_include_raw_shape_declarations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::lib_test::completion_inventory::completion_items_include_local_and_imported_shapes_and_functions -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::hover::hover_test::hover_returns_same_document_raw_shape_docs -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::hover::hover_test::hover_returns_imported_shape_docs_from_interface -- --exact
	$(TERLC_EXACT_TEST) commands::doc::render::render_test::renders_public_shape_declarations -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::interface_conversion::lib_test::interface_rendering::interface_rendering_preserves_public_shape_declarations -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_ordinary_function_named_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::bindings_and_functions::parses_local_binding_named_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::bindings_and_functions::parses_pattern_binding_named_shape -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::bindings_and_functions::old_shape_fat_arrow_spelling_is_not_shape_synonym_surface -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::bindings_and_functions::old_shape_runtime_arrow_spelling_is_not_shape_synonym_surface -- --exact
	$(TERLC) test tests/pattern/ShapeSynonymTest.terl


# Checks structural implication parsing, typechecking, and interface rendering
shape-implications-check: syntax-contract-check
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_structural_implication_in_function_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_structural_implication_in_struct_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_structural_implication_in_type_alias_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_output::syntax_output_decl_test::annotations_and_metadata::syntax_output_preserves_structural_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::rejects_non_structural_generic_implication_target -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::rejects_empty_structural_generic_implication_target -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::rejects_negative_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::positive_structural_implication_remains_supported -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::rejects_duplicate_structural_implication_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::rejects_duplicate_nested_structural_implication_fields -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::rejects_duplicate_structural_implication_fields_inside_generic_types -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::nested_structural_implication_fields_use_independent_scopes -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::rejects_duplicate_record_fields_in_implication_evidence_aliases -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::type_params_test::record_type_alias_fields_use_independent_nested_scopes -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_structural_generic_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_structural_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_preserves_structural_generic_type_alias_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::structural_layout::formatter_preserves_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_preserves_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_imported_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::interface_conversion::lib_test::interface_rendering::interface_rendering_preserves_generic_trait_impl_implications -- --exact
	$(TERLC_EXACT_TEST) commands::doc::render::render_test::renders_generic_trait_impl_implications -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::type_model_test::type_aliases_preserve_structural_implication_bounds -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_forwarded_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_imported_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_in_struct_field -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_forwarded_type_alias_in_generic_struct_field -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_struct_field -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_alias_body -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_in_constructor_signature -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_constructor_signature -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_constructor_return -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_in_template_prop -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_template_prop -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_in_trait_signature -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_trait_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_trait_return -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_forwarded_type_alias_in_generic_trait_method -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_generic_trait_method -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_proven_type_alias_in_explicit_impl_signature -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_explicit_impl_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_type_alias_in_explicit_impl_return -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_nested_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_forwarded_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_accepts_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_unproven_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::accepted_evidence::syntax_output_rejects_missing_structural_implication_field -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_wrong_structural_implication_field_type -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_dynamic_structural_implication_evidence -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_open_map_structural_implication_evidence -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_unproven_forwarded_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_field_outside_structural_implication_scope -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_private_structural_implication_field -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_accepts_imported_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::hir::interface_conversion::lib_test::interface_rendering::interface_rendering_preserves_generic_struct_implications -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_accepts_imported_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_unproven_imported_generic_struct_projection -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_accepts_imported_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::typeck::implication_test::rejected_evidence::syntax_output_rejects_unproven_imported_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_implication_arrow_as_runtime_expression_operator -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_implication_arrow_in_lambda_body -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_implication_arrow_in_case_branch_body -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::rejects_implication_arrow_on_struct_field_declaration -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::rejects_implication_arrow_in_ordinary_type_alias_body -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_shape_import_test::project_vm_tests_execute_imported_receiver_method_implication -- --exact
	$(TERLC_EXACT_TEST) commands::test::test_shape_import_test::project_vm_tests_execute_imported_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::shape_js_test::build_command_executes_structural_implication_for_js_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::hover::hover_test::hover_preserves_local_structural_implication_signature -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --lib lsp::hover::hover_test::editor_surfaces_preserve_imported_structural_implication -- --exact
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=Terlan/Type/ShapeImplication.lean \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl
	$(TERLC) test tests/language/ShapeImplicationTest.terl
	$(TERLC) test std/binary/BinaryTest.terl --name protocol_name_uses_structural_evidence_across_metadata_types


