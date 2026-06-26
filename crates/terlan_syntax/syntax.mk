# Terlan syntax crate validation targets.
#
# Kept as a sub-makefile so Makefile modules stay grouped by ownership while
# still exposing public target names from the repository root.

CARGO ?= cargo
PYTHON ?= python3 -B
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
SYNTAX_MAKEFILE := crates/terlan_syntax/syntax.mk

.PHONY: syntax-help parser-fixture-check formal-syntax-output-gate formal-syntax-a0-22-precedence-gate formal-syntax-a0-22-status formal-syntax-a0-23-keyword-gate formal-syntax-a0-23-status formal-syntax-a0-24-collection-gate formal-syntax-a0-24-status formal-syntax-a0-25-pattern-gate formal-syntax-a0-25-status formal-syntax-a0-26-declaration-gate formal-syntax-a0-26-status formal-syntax-a0-27-type-gate formal-syntax-a0-27-status formal-syntax-a0-28-method-gate formal-syntax-a0-28-status formal-syntax-a0-29-trait-gate formal-syntax-a0-29-status formal-syntax-a0-30-cast-gate formal-syntax-a0-30-status formal-syntax-a0-43-template-raw-gate formal-syntax-a0-43-status formal-syntax-a0-44-config-fixture-gate formal-syntax-a0-44-status formal-syntax-a0-49-config-structure-gate formal-syntax-a0-49-status formal-syntax-a0-52-annotation-gate formal-syntax-a0-52-status

syntax-help:
	@echo "  make parser-fixture-check - validate the canonical EBNF grammar contract"
	@echo "  make formal-syntax-output-gate - run syntax-output parser regressions"
	@echo "  make formal-syntax-a0-22-precedence-gate - run A0.22 expression-precedence and unsupported-operator guards"
	@echo "  make formal-syntax-a0-22-status - print and verify A0.22 syntax baseline references"
	@echo "  make formal-syntax-a0-23-keyword-gate - run A0.23 keyword-expression and guarded-clause coverage"
	@echo "  make formal-syntax-a0-23-status - print and verify A0.23 syntax baseline references"
	@echo "  make formal-syntax-a0-24-collection-gate - run A0.24 collection and binary-form coverage"
	@echo "  make formal-syntax-a0-24-status - print and verify A0.24 syntax baseline references"
	@echo "  make formal-syntax-a0-25-pattern-gate - run A0.25 expanded-pattern coverage"
	@echo "  make formal-syntax-a0-25-status - print and verify A0.25 syntax baseline references"
	@echo "  make formal-syntax-a0-26-declaration-gate - run A0.26 declaration-inventory coverage"
	@echo "  make formal-syntax-a0-26-status - print and verify A0.26 syntax baseline references"
	@echo "  make formal-syntax-a0-27-type-gate - run A0.27 type-family coverage"
	@echo "  make formal-syntax-a0-27-status - print and verify A0.27 syntax baseline references"
	@echo "  make formal-syntax-a0-28-method-gate - run A0.28 method receiver coverage"
	@echo "  make formal-syntax-a0-28-status - print and verify A0.28 syntax baseline references"
	@echo "  make formal-syntax-a0-29-trait-gate - run A0.29 trait/conformance surface coverage"
	@echo "  make formal-syntax-a0-29-status - print and verify A0.29 syntax baseline references"
	@echo "  make formal-syntax-a0-30-cast-gate - run A0.30 cast/conversion syntax coverage"
	@echo "  make formal-syntax-a0-30-status - print and verify A0.30 syntax baseline references"
	@echo "  make formal-syntax-a0-43-template-raw-gate - run A0.43 template-adjacent raw-block fixture coverage"
	@echo "  make formal-syntax-a0-43-status - print and verify A0.43 syntax baseline references"
	@echo "  make formal-syntax-a0-44-config-fixture-gate - run A0.44 config declaration fixture-id coverage"
	@echo "  make formal-syntax-a0-44-status - print and verify A0.44 syntax baseline references"
	@echo "  make formal-syntax-a0-49-config-structure-gate - run A0.49 structured config syntax-output coverage"
	@echo "  make formal-syntax-a0-49-status - print and verify A0.49 syntax baseline references"
	@echo "  make formal-syntax-a0-52-annotation-gate - run A0.52 declaration annotation contract coverage"
	@echo "  make formal-syntax-a0-52-status - print and verify A0.52 syntax baseline references"

parser-fixture-check:
	$(PYTHON) tools/validate_ebnf.py --strict

# Intentionally broad: this gate tracks every syntax-output parser regression.
formal-syntax-output-gate:
	$(CARGO) test -p terlan_syntax syntax_output

formal-syntax-a0-22-precedence-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::parser_expr_test::tests::formal_expr_precedence_keeps_pipe_below_boolean_chain -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::parser_expr_test::tests::formal_boolean_operators_preserve_ebnf_precedence -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::parser_expr_test::tests::formal_keyword_expr_participates_in_pipe_expression -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::parser_expr_test::tests::formal_unary_expr_preserves_precedence -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::parser_expr_test::tests::formal_deprecated_equality_operators_are_rejected -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-22-status:
	@echo "A0.22 successor matrix: A0.22 syntax precedence"
	@echo "A0.22 successor fixture: docs/grammar/fixtures/core/17_expression_precedence.terl"
	@echo "A0.22 successor negative fixtures: docs/grammar/fixtures/negative/62_deprecated_eqeqeq_operator.terl docs/grammar/fixtures/negative/63_deprecated_slash_eq_operator.terl docs/grammar/fixtures/negative/64_deprecated_eq_slash_eq_operator.terl"
	@echo "A0.22 successor gate: make formal-syntax-a0-22-precedence-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-23-keyword-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_receive_expr_parses_as_keyword_expression -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_receive_expr_parses_after_clause -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_try_expr_parses_of_and_catch_clauses -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_try_expr_parses_after_clause -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_keyword_exprs_preserve_clause_guards -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_quote_unquote_exprs_parse_as_keyword_expressions -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-23-status:
	@echo "A0.23 successor matrix: A0.23 syntax keyword expressions"
	@echo "A0.23 successor fixture: docs/grammar/fixtures/core/18_keyword_exprs.terl"
	@echo "A0.23 successor gate: make formal-syntax-a0-23-keyword-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-24-collection-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_cons_list_expr_is_distinct_from_generator_expr -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_list_comprehension_rejects_unrepresented_extra_generators -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_collection_exprs_preserve_ast_shapes -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_binary_segments_are_rejected_as_erlang_source_syntax -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-24-status:
	@echo "A0.24 successor matrix: A0.24 syntax collection forms and Erlang binary rejection"
	@echo "A0.24 successor fixture: docs/grammar/fixtures/core/19_collection_binary_forms.terl"
	@echo "A0.24 successor negative fixture: docs/grammar/fixtures/negative/33_multi_generator_list_comprehension.terl"
	@echo "A0.24 successor gate: make formal-syntax-a0-24-collection-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-25-pattern-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_raw_atom_patterns_are_literal_patterns -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_keyword_exprs_preserve_clause_guards -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_pattern_expansion_preserves_ast_shapes -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-25-status:
	@echo "A0.25 successor matrix: A0.25 syntax pattern expansion"
	@echo "A0.25 successor fixture: docs/grammar/fixtures/core/20_pattern_expansion.terl"
	@echo "A0.25 successor gate: make formal-syntax-a0-25-pattern-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-26-declaration-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_declaration_inventory_covers_parser_decl_classes -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-26-status:
	@echo "A0.26 successor matrix: A0.26 syntax declaration inventory"
	@echo "A0.26 successor fixture: docs/grammar/fixtures/core/21_declaration_inventory.terl"
	@echo "A0.26 successor gate: make formal-syntax-a0-26-declaration-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-27-type-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_type_family_inventory_preserves_type_expr_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_type_family_rejects_runtime_expression_tokens -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-27-status:
	@echo "A0.27 successor matrix: A0.27 syntax type-family inventory"
	@echo "A0.27 successor fixture: docs/grammar/fixtures/core/22_type_family_inventory.terl"
	@echo "A0.27 successor negative fixture: docs/grammar/fixtures/negative/2_runtime_expr_in_type.terl"
	@echo "A0.27 successor gate: make formal-syntax-a0-27-type-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-28-method-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_method_receiver_inventory_preserves_validated_methods -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_method_receiver_diagnostics_reject_invalid_method_heads -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-28-status:
	@echo "A0.28 successor matrix: A0.28 syntax method receiver diagnostics"
	@echo "A0.28 successor fixture: docs/grammar/fixtures/core/23_methods_receiver_inventory.terl"
	@echo "A0.28 successor negative fixtures: docs/grammar/fixtures/negative/65_uppercase_method_receiver_name.terl docs/grammar/fixtures/negative/66_lowercase_method_receiver_type.terl docs/grammar/fixtures/negative/67_uppercase_method_name.terl"
	@echo "A0.28 successor gate: make formal-syntax-a0-28-method-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-29-trait-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_trait_conformance_inventory_preserves_trait_surface -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::parses_function_declaration_with_constraint_list -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::parses_method_trait_method_and_impl_method_constraint_lists -- --exact
	# Intentionally broad: this filter tracks the parser-facing trait conformance fixture family.
	$(CARGO) test -p terlan_syntax formal_trait_conformance_syntax -- --nocapture
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-29-status:
	@echo "A0.29 successor matrix: A0.29 syntax trait declarations, implements clauses, impl blocks, and native conformance contract"
	@echo "A0.29 successor fixture: docs/grammar/fixtures/core/24_trait_conformance_inventory.terl"
	@echo "A0.29 successor fixture: docs/grammar/fixtures/core/15_trait_conformance_forms.terl"
	@echo "A0.29 successor fixture: docs/grammar/fixtures/core/26_trait_impl_adapter.terl"
	@echo "A0.29 successor gate: make formal-syntax-a0-29-trait-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-30-cast-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_cast_expr_preserves_ebnf_precedence -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_preserves_cast_expression_shape -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-30-status:
	@echo "A0.30 successor matrix: A0.30 explicit casts and conversion-boundary diagnostics"
	@echo "A0.30 successor fixture: docs/grammar/fixtures/core/25_cast_conversion_inventory.terl"
	@echo "A0.30 successor gate: make formal-syntax-a0-30-cast-gate"
	@echo "A0.30 successor gate: make formal-typecheck-a0-30-cast-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-43-template-raw-gate:
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-43-status:
	@echo "A0.43 successor matrix: A0.43 template-adjacent raw-block scanning"
	@echo "A0.43 successor fixture: docs/grammar/fixtures/raw/2_template_raw_block_scanning.terl"
	@echo "A0.43 successor gate: make formal-syntax-a0-43-template-raw-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-44-config-fixture-gate:
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-44-status:
	@echo "A0.44 successor matrix: A0.44 config fixture ID cleanup"
	@echo "A0.44 successor fixture: docs/grammar/fixtures/core/6_config_declarations.terl"
	@echo "A0.44 successor fixture: docs/grammar/fixtures/raw/1_template_config_declarations.terl"
	@echo "A0.44 successor gate: make formal-syntax-a0-44-config-fixture-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-49-config-structure-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_normalizes_config_declarations -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-49-status:
	@echo "A0.49 successor matrix: A0.49 structured config payloads"
	@echo "A0.49 successor fixture: docs/grammar/fixtures/core/6_config_declarations.terl"
	@echo "A0.49 successor gate: make formal-syntax-a0-49-config-structure-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-syntax-a0-52-annotation-gate:
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_annotation_subjects_are_rejected_before_declaration_routing -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax parser::tests::formal_declaration_annotation_before_function_still_parses -- --exact
	$(EXACT_CARGO_TEST) -p terlan_syntax syntax_output::tests::syntax_output_preserves_declaration_annotations -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_rejects_annotation_subject_before_phase_manifest -- --exact
	$(MAKE) -f $(SYNTAX_MAKEFILE) --no-print-directory parser-fixture-check

formal-syntax-a0-52-status:
	@echo "A0.52 successor matrix: A0.52 declaration annotation contract"
	@echo "A0.52 successor fixture: docs/grammar/fixtures/core/11_annotations.terl"
	@echo "A0.52 successor gate: make formal-syntax-a0-52-annotation-gate"
	@bash scripts/check_baseline_capabilities.sh
