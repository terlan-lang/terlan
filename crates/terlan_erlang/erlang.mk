# Terlan Erlang backend compiler-path validation targets.
#
# Kept with the Erlang backend crate so backend checks stay localized.

CARGO ?= cargo
TERLC ?= $(CARGO) run -p terlan_cli --
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh

.PHONY: erlang-help formal-erlang-core-gate formal-erlang-syntax-bridge-gate formal-erlang-a0-artifact-gate formal-erlang-a0-profile-gate formal-erlang-a0-1-profile-gate formal-erlang-a0-1-artifact-gate formal-erlang-a0-1-release-gate formal-erlang-a0-1-gate formal-erlang-a0-1-status formal-erlang-a0-2-profile-gate formal-erlang-a0-2-artifact-gate formal-erlang-a0-2-release-gate formal-erlang-a0-2-gate formal-erlang-a0-2-status formal-erlang-a0-3-profile-gate formal-erlang-a0-3-artifact-gate formal-erlang-a0-3-release-gate formal-erlang-a0-3-gate formal-erlang-a0-3-status formal-erlang-a0-4-profile-gate formal-erlang-a0-4-artifact-gate formal-erlang-a0-4-release-gate formal-erlang-a0-4-gate formal-erlang-a0-4-status formal-erlang-a0-5-profile-gate formal-erlang-a0-5-artifact-gate formal-erlang-a0-5-release-gate formal-erlang-a0-5-gate formal-erlang-a0-5-status formal-erlang-a0-6-profile-gate formal-erlang-a0-6-artifact-gate formal-erlang-a0-6-release-gate formal-erlang-a0-6-gate formal-erlang-a0-6-status formal-erlang-a0-7-profile-gate formal-erlang-a0-7-artifact-gate formal-erlang-a0-7-release-gate formal-erlang-a0-7-gate formal-erlang-a0-7-status formal-erlang-a0-8-profile-gate formal-erlang-a0-8-artifact-gate formal-erlang-a0-8-release-gate formal-erlang-a0-8-gate formal-erlang-a0-8-status formal-erlang-a0-9-profile-gate formal-erlang-a0-9-artifact-gate formal-erlang-a0-9-release-gate formal-erlang-a0-9-gate formal-erlang-a0-9-status formal-erlang-a0-10-profile-gate formal-erlang-a0-10-artifact-gate formal-erlang-a0-10-release-gate formal-erlang-a0-10-gate formal-erlang-a0-10-status formal-erlang-a0-11-profile-gate formal-erlang-a0-11-artifact-gate formal-erlang-a0-11-release-gate formal-erlang-a0-11-gate formal-erlang-a0-11-status formal-erlang-a0-12-profile-gate formal-erlang-a0-12-artifact-gate formal-erlang-a0-12-release-gate formal-erlang-a0-12-gate formal-erlang-a0-12-status formal-erlang-a0-13-profile-gate formal-erlang-a0-13-artifact-gate formal-erlang-a0-13-release-gate formal-erlang-a0-13-gate formal-erlang-a0-13-status formal-erlang-a0-14-profile-gate formal-erlang-a0-14-artifact-gate formal-erlang-a0-14-release-gate formal-erlang-a0-14-gate formal-erlang-a0-14-status formal-erlang-a0-release-gate formal-erlang-gate
.PHONY: formal-erlang-a0-15-profile-gate formal-erlang-a0-15-artifact-gate formal-erlang-a0-15-release-gate formal-erlang-a0-15-gate formal-erlang-a0-15-status
.PHONY: formal-erlang-a0-16-profile-gate formal-erlang-a0-16-artifact-gate formal-erlang-a0-16-release-gate formal-erlang-a0-16-gate formal-erlang-a0-16-status
.PHONY: formal-erlang-a0-17-profile-gate formal-erlang-a0-17-artifact-gate formal-erlang-a0-17-release-gate formal-erlang-a0-17-gate formal-erlang-a0-17-status
.PHONY: formal-erlang-a0-18-profile-gate formal-erlang-a0-18-artifact-gate formal-erlang-a0-18-release-gate formal-erlang-a0-18-gate formal-erlang-a0-18-status
.PHONY: formal-erlang-a0-19-profile-gate formal-erlang-a0-19-artifact-gate formal-erlang-a0-19-release-gate formal-erlang-a0-19-gate formal-erlang-a0-19-status
.PHONY: formal-erlang-a0-20-profile-gate formal-erlang-a0-20-artifact-gate formal-erlang-a0-20-release-gate formal-erlang-a0-20-gate formal-erlang-a0-20-status
.PHONY: formal-erlang-a0-21-diagnostic-gate formal-erlang-a0-21-status

erlang-help:
	@echo "  make formal-erlang-core-gate - run CoreIR-gated Erlang emission smoke tests"
	@echo "  make formal-erlang-syntax-bridge-gate - run opt-in syntax-output Erlang bridge regressions"
	@echo "  make formal-erlang-gate - run Erlang backend RC gate"

# Focused RC smoke: protects the CoreIR-gated Erlang backend entry point.
formal-erlang-core-gate:
	$(EXACT_CARGO_TEST) -p terlan_erlang emit::tests::core_module_syntax_bridge_emit_delegates_after_identity_validation -- --exact
	$(EXACT_CARGO_TEST) -p terlan_erlang emit::tests::core_module_syntax_bridge_emit_rejects_stale_core_identity -- --exact

# Intentionally broad and opt-in: this tracks direct syntax-output Erlang bridge regressions.
formal-erlang-syntax-bridge-gate:
	$(CARGO) test -p terlan_erlang formal_syntax_output_direct_emit

formal-erlang-a0-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0/mathx.terl --out-dir "$$out"; \
	erlc "$$out/mathx.erl"

formal-erlang-a0-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_mathx_for_a0_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_rejects_binary_for_a0_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_mathx_for_a0_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_rejects_binary_for_a0_erlang_target_profile -- --exact

formal-erlang-a0-1-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_1_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_arithmetic_for_a0_1_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_subtraction_out_of_a0_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_arithmetic_for_a0_1_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_subtraction_out_of_a0_erlang_target_profile -- --exact

formal-erlang-a0-1-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-1-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_1/arithmetic.terl --target-profile a0.1-erlang --out-dir "$$out"; \
	erlc "$$out/a0_1_arithmetic.erl"

formal-erlang-a0-1-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-1-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_1/arithmetic.terl --target-profile a0.1-erlang; \
	target/release/terlc emit tests/fixtures/a0_1/arithmetic.terl --target-profile a0.1-erlang --out-dir "$$out"; \
	erlc "$$out/a0_1_arithmetic.erl"

formal-erlang-a0-1-gate: formal-erlang-a0-1-profile-gate formal-erlang-a0-1-artifact-gate formal-erlang-a0-1-release-gate

formal-erlang-a0-1-status:
	@echo "A0.1 successor matrix: A0.1 Erlang"
	@echo "A0.1 successor fixture: tests/fixtures/a0_1/arithmetic.terl"
	@echo "A0.1 successor target profile: a0.1-erlang"
	@echo "A0.1 successor gate: make formal-erlang-a0-1-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-2-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_2_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_bool_ops_for_a0_2_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_bool_ops_out_of_a0_1_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_bool_ops_for_a0_2_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_bool_ops_out_of_a0_1_erlang_target_profile -- --exact

formal-erlang-a0-2-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-2-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_2/bool_ops.terl --target-profile a0.2-erlang --out-dir "$$out"; \
	erlc "$$out/a0_2_bool_ops.erl"

formal-erlang-a0-2-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-2-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_2/bool_ops.terl --target-profile a0.2-erlang; \
	target/release/terlc emit tests/fixtures/a0_2/bool_ops.terl --target-profile a0.2-erlang --out-dir "$$out"; \
	erlc "$$out/a0_2_bool_ops.erl"

formal-erlang-a0-2-gate: formal-erlang-a0-2-profile-gate formal-erlang-a0-2-artifact-gate formal-erlang-a0-2-release-gate

formal-erlang-a0-2-status:
	@echo "A0.2 successor matrix: A0.2 Erlang"
	@echo "A0.2 successor fixture: tests/fixtures/a0_2/bool_ops.terl"
	@echo "A0.2 successor target profile: a0.2-erlang"
	@echo "A0.2 successor gate: make formal-erlang-a0-2-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-3-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_3_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_if_expr_for_a0_3_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_if_expr_out_of_a0_2_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_if_expr_for_a0_3_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_if_expr_out_of_a0_2_erlang_target_profile -- --exact

formal-erlang-a0-3-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-3-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_3/if_expr.terl --target-profile a0.3-erlang --out-dir "$$out"; \
	erlc "$$out/a0_3_if_expr.erl"

formal-erlang-a0-3-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-3-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_3/if_expr.terl --target-profile a0.3-erlang; \
	target/release/terlc emit tests/fixtures/a0_3/if_expr.terl --target-profile a0.3-erlang --out-dir "$$out"; \
	erlc "$$out/a0_3_if_expr.erl"

formal-erlang-a0-3-gate: formal-erlang-a0-3-profile-gate formal-erlang-a0-3-artifact-gate formal-erlang-a0-3-release-gate

formal-erlang-a0-3-status:
	@echo "A0.3 successor matrix: A0.3 Erlang"
	@echo "A0.3 successor fixture: tests/fixtures/a0_3/if_expr.terl"
	@echo "A0.3 successor target profile: a0.3-erlang"
	@echo "A0.3 successor gate: make formal-erlang-a0-3-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-4-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_4_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_case_expr_for_a0_4_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_case_expr_out_of_a0_3_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_case_expr_for_a0_4_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_case_expr_out_of_a0_3_erlang_target_profile -- --exact

formal-erlang-a0-4-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-4-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_4/case_expr.terl --target-profile a0.4-erlang --out-dir "$$out"; \
	erlc "$$out/a0_4_case_expr.erl"

formal-erlang-a0-4-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-4-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_4/case_expr.terl --target-profile a0.4-erlang; \
	target/release/terlc emit tests/fixtures/a0_4/case_expr.terl --target-profile a0.4-erlang --out-dir "$$out"; \
	erlc "$$out/a0_4_case_expr.erl"

formal-erlang-a0-4-gate: formal-erlang-a0-4-profile-gate formal-erlang-a0-4-artifact-gate formal-erlang-a0-4-release-gate

formal-erlang-a0-4-status:
	@echo "A0.4 successor matrix: A0.4 Erlang"
	@echo "A0.4 successor fixture: tests/fixtures/a0_4/case_expr.terl"
	@echo "A0.4 successor target profile: a0.4-erlang"
	@echo "A0.4 successor gate: make formal-erlang-a0-4-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-5-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_5_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_raw_atoms_for_a0_5_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_raw_atoms_out_of_a0_4_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile -- --exact

formal-erlang-a0-5-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-5-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_5/raw_atoms.terl --target-profile a0.5-erlang --out-dir "$$out"; \
	erlc "$$out/a0_5_raw_atoms.erl"

formal-erlang-a0-5-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-5-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_5/raw_atoms.terl --target-profile a0.5-erlang; \
	target/release/terlc emit tests/fixtures/a0_5/raw_atoms.terl --target-profile a0.5-erlang --out-dir "$$out"; \
	erlc "$$out/a0_5_raw_atoms.erl"

formal-erlang-a0-5-gate: formal-erlang-a0-5-profile-gate formal-erlang-a0-5-artifact-gate formal-erlang-a0-5-release-gate

formal-erlang-a0-5-status:
	@echo "A0.5 successor matrix: A0.5 Erlang"
	@echo "A0.5 successor fixture: tests/fixtures/a0_5/raw_atoms.terl"
	@echo "A0.5 successor target profile: a0.5-erlang"
	@echo "A0.5 successor gate: make formal-erlang-a0-5-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-6-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_6_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_tuples_for_a0_6_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_tuples_out_of_a0_5_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_tuples_for_a0_6_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_tuples_out_of_a0_5_erlang_target_profile -- --exact

formal-erlang-a0-6-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-6-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_6/tuples.terl --target-profile a0.6-erlang --out-dir "$$out"; \
	erlc "$$out/a0_6_tuples.erl"

formal-erlang-a0-6-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-6-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_6/tuples.terl --target-profile a0.6-erlang; \
	target/release/terlc emit tests/fixtures/a0_6/tuples.terl --target-profile a0.6-erlang --out-dir "$$out"; \
	erlc "$$out/a0_6_tuples.erl"

formal-erlang-a0-6-gate: formal-erlang-a0-6-profile-gate formal-erlang-a0-6-artifact-gate formal-erlang-a0-6-release-gate

formal-erlang-a0-6-status:
	@echo "A0.6 successor matrix: A0.6 Erlang"
	@echo "A0.6 successor fixture: tests/fixtures/a0_6/tuples.terl"
	@echo "A0.6 successor target profile: a0.6-erlang"
	@echo "A0.6 successor gate: make formal-erlang-a0-6-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-7-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_7_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_lists_for_a0_7_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_lists_out_of_a0_6_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile -- --exact

formal-erlang-a0-7-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-7-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_7/lists.terl --target-profile a0.7-erlang --out-dir "$$out"; \
	erlc "$$out/a0_7_lists.erl"

formal-erlang-a0-7-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-7-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_7/lists.terl --target-profile a0.7-erlang; \
	target/release/terlc emit tests/fixtures/a0_7/lists.terl --target-profile a0.7-erlang --out-dir "$$out"; \
	erlc "$$out/a0_7_lists.erl"

formal-erlang-a0-7-gate: formal-erlang-a0-7-profile-gate formal-erlang-a0-7-artifact-gate formal-erlang-a0-7-release-gate

formal-erlang-a0-7-status:
	@echo "A0.7 successor matrix: A0.7 Erlang"
	@echo "A0.7 successor fixture: tests/fixtures/a0_7/lists.terl"
	@echo "A0.7 successor target profile: a0.7-erlang"
	@echo "A0.7 successor gate: make formal-erlang-a0-7-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-8-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_8_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_binary_for_a0_8_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_binary_out_of_a0_7_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile -- --exact

formal-erlang-a0-8-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-8-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_8/binary_literal.terl --target-profile a0.8-erlang --out-dir "$$out"; \
	erlc "$$out/a0_8_binary_literal.erl"

formal-erlang-a0-8-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-8-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_8/binary_literal.terl --target-profile a0.8-erlang; \
	target/release/terlc emit tests/fixtures/a0_8/binary_literal.terl --target-profile a0.8-erlang --out-dir "$$out"; \
	erlc "$$out/a0_8_binary_literal.erl"

formal-erlang-a0-8-gate: formal-erlang-a0-8-profile-gate formal-erlang-a0-8-artifact-gate formal-erlang-a0-8-release-gate

formal-erlang-a0-8-status:
	@echo "A0.8 successor matrix: A0.8 Erlang"
	@echo "A0.8 successor fixture: tests/fixtures/a0_8/binary_literal.terl"
	@echo "A0.8 successor target profile: a0.8-erlang"
	@echo "A0.8 successor gate: make formal-erlang-a0-8-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-9-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_9_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_list_cons_for_a0_9_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_list_cons_out_of_a0_8_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile -- --exact

formal-erlang-a0-9-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-9-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_9/list_cons.terl --target-profile a0.9-erlang --out-dir "$$out"; \
	erlc "$$out/a0_9_list_cons.erl"

formal-erlang-a0-9-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-9-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_9/list_cons.terl --target-profile a0.9-erlang; \
	target/release/terlc emit tests/fixtures/a0_9/list_cons.terl --target-profile a0.9-erlang --out-dir "$$out"; \
	erlc "$$out/a0_9_list_cons.erl"

formal-erlang-a0-9-gate: formal-erlang-a0-9-profile-gate formal-erlang-a0-9-artifact-gate formal-erlang-a0-9-release-gate

formal-erlang-a0-9-status:
	@echo "A0.9 successor matrix: A0.9 Erlang"
	@echo "A0.9 successor fixture: tests/fixtures/a0_9/list_cons.terl"
	@echo "A0.9 successor target profile: a0.9-erlang"
	@echo "A0.9 successor gate: make formal-erlang-a0-9-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-10-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_10_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_named_call_for_a0_10_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_named_call_out_of_a0_9_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile -- --exact

formal-erlang-a0-10-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-10-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_10/named_call.terl --target-profile a0.10-erlang --out-dir "$$out"; \
	erlc "$$out/a0_10_named_call.erl"

formal-erlang-a0-10-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-10-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_10/named_call.terl --target-profile a0.10-erlang; \
	target/release/terlc emit tests/fixtures/a0_10/named_call.terl --target-profile a0.10-erlang --out-dir "$$out"; \
	erlc "$$out/a0_10_named_call.erl"

formal-erlang-a0-10-gate: formal-erlang-a0-10-profile-gate formal-erlang-a0-10-artifact-gate formal-erlang-a0-10-release-gate

formal-erlang-a0-10-status:
	@echo "A0.10 successor matrix: A0.10 Erlang"
	@echo "A0.10 successor fixture: tests/fixtures/a0_10/named_call.terl"
	@echo "A0.10 successor target profile: a0.10-erlang"
	@echo "A0.10 successor gate: make formal-erlang-a0-10-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-11-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_11_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_unary_neg_for_a0_11_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_unary_neg_out_of_a0_10_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_unary_neg_for_a0_11_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_unary_neg_out_of_a0_10_erlang_target_profile -- --exact

formal-erlang-a0-11-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-11-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_11/unary_neg.terl --target-profile a0.11-erlang --out-dir "$$out"; \
	erlc "$$out/a0_11_unary_neg.erl"

formal-erlang-a0-11-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-11-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_11/unary_neg.terl --target-profile a0.11-erlang; \
	target/release/terlc emit tests/fixtures/a0_11/unary_neg.terl --target-profile a0.11-erlang --out-dir "$$out"; \
	erlc "$$out/a0_11_unary_neg.erl"

formal-erlang-a0-11-gate: formal-erlang-a0-11-profile-gate formal-erlang-a0-11-artifact-gate formal-erlang-a0-11-release-gate

formal-erlang-a0-11-status:
	@echo "A0.11 successor matrix: A0.11 Erlang"
	@echo "A0.11 successor fixture: tests/fixtures/a0_11/unary_neg.terl"
	@echo "A0.11 successor target profile: a0.11-erlang"
	@echo "A0.11 successor gate: make formal-erlang-a0-11-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-12-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_12_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_constructor_call_for_a0_12_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_constructor_call_out_of_a0_11_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile -- --exact

formal-erlang-a0-12-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-12-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_12/constructor_call.terl --target-profile a0.12-erlang --out-dir "$$out"; \
	erlc "$$out/a0_12_constructor_call.erl"

formal-erlang-a0-12-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-12-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_12/constructor_call.terl --target-profile a0.12-erlang; \
	target/release/terlc emit tests/fixtures/a0_12/constructor_call.terl --target-profile a0.12-erlang --out-dir "$$out"; \
	erlc "$$out/a0_12_constructor_call.erl"

formal-erlang-a0-12-gate: formal-erlang-a0-12-profile-gate formal-erlang-a0-12-artifact-gate formal-erlang-a0-12-release-gate

formal-erlang-a0-12-status:
	@echo "A0.12 successor matrix: A0.12 Erlang"
	@echo "A0.12 successor fixture: tests/fixtures/a0_12/constructor_call.terl"
	@echo "A0.12 successor target profile: a0.12-erlang"
	@echo "A0.12 successor gate: make formal-erlang-a0-12-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-13-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_13_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_constructor_pattern_for_a0_13_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_constructor_pattern_out_of_a0_12_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile -- --exact

formal-erlang-a0-13-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-13-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_13/constructor_pattern.terl --target-profile a0.13-erlang --out-dir "$$out"; \
	erlc "$$out/a0_13_constructor_pattern.erl"

formal-erlang-a0-13-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-13-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_13/constructor_pattern.terl --target-profile a0.13-erlang; \
	target/release/terlc emit tests/fixtures/a0_13/constructor_pattern.terl --target-profile a0.13-erlang --out-dir "$$out"; \
	erlc "$$out/a0_13_constructor_pattern.erl"

formal-erlang-a0-13-gate: formal-erlang-a0-13-profile-gate formal-erlang-a0-13-artifact-gate formal-erlang-a0-13-release-gate

formal-erlang-a0-13-status:
	@echo "A0.13 successor matrix: A0.13 Erlang"
	@echo "A0.13 successor fixture: tests/fixtures/a0_13/constructor_pattern.terl"
	@echo "A0.13 successor target profile: a0.13-erlang"
	@echo "A0.13 successor gate: make formal-erlang-a0-13-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-14-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_14_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_lambda_for_a0_14_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_lambda_out_of_a0_13_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_lambda_for_a0_14_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_lambda_out_of_a0_13_erlang_target_profile -- --exact

formal-erlang-a0-14-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-14-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_14/lambda.terl --target-profile a0.14-erlang --out-dir "$$out"; \
	erlc "$$out/a0_14_lambda.erl"

formal-erlang-a0-14-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-14-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_14/lambda.terl --target-profile a0.14-erlang; \
	target/release/terlc emit tests/fixtures/a0_14/lambda.terl --target-profile a0.14-erlang --out-dir "$$out"; \
	erlc "$$out/a0_14_lambda.erl"

formal-erlang-a0-14-gate: formal-erlang-a0-14-profile-gate formal-erlang-a0-14-artifact-gate formal-erlang-a0-14-release-gate

formal-erlang-a0-14-status:
	@echo "A0.14 successor matrix: A0.14 Erlang"
	@echo "A0.14 successor fixture: tests/fixtures/a0_14/lambda.terl"
	@echo "A0.14 successor target profile: a0.14-erlang"
	@echo "A0.14 successor gate: make formal-erlang-a0-14-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-15-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_15_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_constructor_extension_for_a0_15_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_constructor_extension_out_of_a0_14_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_constructor_extension_for_a0_15_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_constructor_extension_out_of_a0_14_erlang_target_profile -- --exact

formal-erlang-a0-15-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-15-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_15/constructor_extension.terl --target-profile a0.15-erlang --out-dir "$$out"; \
	erlc "$$out/a0_15_constructor_extension.erl"

formal-erlang-a0-15-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-15-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_15/constructor_extension.terl --target-profile a0.15-erlang; \
	target/release/terlc emit tests/fixtures/a0_15/constructor_extension.terl --target-profile a0.15-erlang --out-dir "$$out"; \
	erlc "$$out/a0_15_constructor_extension.erl"

formal-erlang-a0-15-gate: formal-erlang-a0-15-profile-gate formal-erlang-a0-15-artifact-gate formal-erlang-a0-15-release-gate

formal-erlang-a0-15-status:
	@echo "A0.15 successor matrix: A0.15 Erlang"
	@echo "A0.15 successor fixture: tests/fixtures/a0_15/constructor_extension.terl"
	@echo "A0.15 successor target profile: a0.15-erlang"
	@echo "A0.15 successor gate: make formal-erlang-a0-15-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-16-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_16_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_fun_call_for_a0_16_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_fun_call_out_of_a0_15_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile -- --exact

formal-erlang-a0-16-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-16-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_16/fun_call.terl --target-profile a0.16-erlang --out-dir "$$out"; \
	erlc "$$out/a0_16_fun_call.erl"

formal-erlang-a0-16-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-16-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_16/fun_call.terl --target-profile a0.16-erlang; \
	target/release/terlc emit tests/fixtures/a0_16/fun_call.terl --target-profile a0.16-erlang --out-dir "$$out"; \
	erlc "$$out/a0_16_fun_call.erl"

formal-erlang-a0-16-gate: formal-erlang-a0-16-profile-gate formal-erlang-a0-16-artifact-gate formal-erlang-a0-16-release-gate

formal-erlang-a0-16-status:
	@echo "A0.16 successor matrix: A0.16 Erlang"
	@echo "A0.16 successor fixture: tests/fixtures/a0_16/fun_call.terl"
	@echo "A0.16 successor target profile: a0.16-erlang"
	@echo "A0.16 successor gate: make formal-erlang-a0-16-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-17-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_17_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_field_access_for_a0_17_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_field_access_out_of_a0_16_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_field_access_for_a0_17_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_field_access_out_of_a0_16_erlang_target_profile -- --exact

formal-erlang-a0-17-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-17-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_17/field_access.terl --target-profile a0.17-erlang --out-dir "$$out"; \
	erlc "$$out/a0_17_field_access.erl"

formal-erlang-a0-17-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-17-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_17/field_access.terl --target-profile a0.17-erlang; \
	target/release/terlc emit tests/fixtures/a0_17/field_access.terl --target-profile a0.17-erlang --out-dir "$$out"; \
	erlc "$$out/a0_17_field_access.erl"

formal-erlang-a0-17-gate: formal-erlang-a0-17-profile-gate formal-erlang-a0-17-artifact-gate formal-erlang-a0-17-release-gate

formal-erlang-a0-17-status:
	@echo "A0.17 successor matrix: A0.17 Erlang"
	@echo "A0.17 successor fixture: tests/fixtures/a0_17/field_access.terl"
	@echo "A0.17 successor target profile: a0.17-erlang"
	@echo "A0.17 successor gate: make formal-erlang-a0-17-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-18-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_18_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_let_expr_for_a0_18_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_let_expr_out_of_a0_17_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_let_expr_for_a0_18_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_let_expr_out_of_a0_17_erlang_target_profile -- --exact

formal-erlang-a0-18-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-18-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_18/let_expr.terl --target-profile a0.18-erlang --out-dir "$$out"; \
	erlc "$$out/a0_18_let_expr.erl"

formal-erlang-a0-18-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-18-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_18/let_expr.terl --target-profile a0.18-erlang; \
	target/release/terlc emit tests/fixtures/a0_18/let_expr.terl --target-profile a0.18-erlang --out-dir "$$out"; \
	erlc "$$out/a0_18_let_expr.erl"

formal-erlang-a0-18-gate: formal-erlang-a0-18-profile-gate formal-erlang-a0-18-artifact-gate formal-erlang-a0-18-release-gate

formal-erlang-a0-18-status:
	@echo "A0.18 successor matrix: A0.18 Erlang"
	@echo "A0.18 successor fixture: tests/fixtures/a0_18/let_expr.terl"
	@echo "A0.18 successor target profile: a0.18-erlang"
	@echo "A0.18 successor gate: make formal-erlang-a0-18-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-19-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_19_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_index_access_for_a0_19_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_index_access_out_of_a0_18_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_index_access_for_a0_19_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_index_access_out_of_a0_18_erlang_target_profile -- --exact

formal-erlang-a0-19-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-19-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_19/index_access.terl --target-profile a0.19-erlang --out-dir "$$out"; \
	erlc "$$out/a0_19_index_access.erl"

formal-erlang-a0-19-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-19-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_19/index_access.terl --target-profile a0.19-erlang; \
	target/release/terlc emit tests/fixtures/a0_19/index_access.terl --target-profile a0.19-erlang --out-dir "$$out"; \
	erlc "$$out/a0_19_index_access.erl"

formal-erlang-a0-19-gate: formal-erlang-a0-19-profile-gate formal-erlang-a0-19-artifact-gate formal-erlang-a0-19-release-gate

formal-erlang-a0-19-status:
	@echo "A0.19 successor matrix: A0.19 Erlang"
	@echo "A0.19 successor fixture: tests/fixtures/a0_19/index_access.terl"
	@echo "A0.19 successor target profile: a0.19-erlang"
	@echo "A0.19 successor gate: make formal-erlang-a0-19-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-20-profile-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_20_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_accepts_qualified_calls_for_a0_20_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_keeps_qualified_calls_out_of_a0_19_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile -- --exact

formal-erlang-a0-20-artifact-gate:
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-20-artifact.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	$(TERLC) emit tests/fixtures/a0_20/qualified_calls.terl --target-profile a0.20-erlang --out-dir "$$out"; \
	erlc "$$out/a0_20_qualified_calls.erl"

formal-erlang-a0-20-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-20-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0_20/qualified_calls.terl --target-profile a0.20-erlang; \
	target/release/terlc emit tests/fixtures/a0_20/qualified_calls.terl --target-profile a0.20-erlang --out-dir "$$out"; \
	erlc "$$out/a0_20_qualified_calls.erl"

formal-erlang-a0-20-gate: formal-erlang-a0-20-profile-gate formal-erlang-a0-20-artifact-gate formal-erlang-a0-20-release-gate

formal-erlang-a0-20-status:
	@echo "A0.20 successor matrix: A0.20 Erlang"
	@echo "A0.20 successor fixture: tests/fixtures/a0_20/qualified_calls.terl"
	@echo "A0.20 successor target profile: a0.20-erlang"
	@echo "A0.20 successor gate: make formal-erlang-a0-20-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-21-diagnostic-gate:
	$(EXACT_CARGO_TEST) -p terlan_cli tests::parse_args_accepts_a0_21_erlang_target_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli validation::target_profile::tests::target_profile_rejects_remote_fun_ref_for_a0_21_erlang_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan_cli tests::run_check_single_file_rejects_remote_fun_ref_for_a0_21_erlang_target_profile -- --exact
	! $(TERLC) check tests/fixtures/a0_21/backend_specific_remote_fun_ref.terl --target-profile a0.21-erlang

formal-erlang-a0-21-status:
	@echo "A0.21 successor matrix: A0.21 Erlang diagnostics"
	@echo "A0.21 successor fixture: tests/fixtures/a0_21/backend_specific_remote_fun_ref.terl"
	@echo "A0.21 successor target profile: a0.21-erlang"
	@echo "A0.21 successor gate: make formal-erlang-a0-21-diagnostic-gate"
	@bash scripts/check_baseline_capabilities.sh

formal-erlang-a0-release-gate:
	$(CARGO) build -p terlan_cli --release
	@tmp_dir=$$(mktemp -d /tmp/terlan-a0-release.XXXXXX); \
	trap 'rm -rf "$$tmp_dir"' EXIT; \
	out="$$tmp_dir/out"; \
	mkdir -p "$$out"; \
	target/release/terlc check tests/fixtures/a0/mathx.terl --target-profile a0-erlang; \
	target/release/terlc emit tests/fixtures/a0/mathx.terl --target-profile a0-erlang --out-dir "$$out"; \
	erlc "$$out/mathx.erl"

formal-erlang-gate: formal-erlang-core-gate formal-erlang-a0-artifact-gate formal-erlang-a0-profile-gate formal-erlang-a0-release-gate
