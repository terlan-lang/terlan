# Terlan HIR compiler-path validation targets.
#
# Kept with the HIR crate so validation targets are owned by the owning module.

CARGO ?= cargo
EXACT_CARGO_TEST ?= bash scripts/run_exact_cargo_test.sh
T_HIR_EXACT_TEST := $(EXACT_CARGO_TEST) -p terlan_hir

.PHONY: hir-help formal-contract-gate formal-hir-gate

hir-help:
	@echo "  make formal-contract-gate - run canonical syntax-contract regressions"
	@echo "  make formal-hir-gate - run syntax-output HIR/interface regressions"

formal-contract-gate:
	$(T_HIR_EXACT_TEST) lib_test::hir_accepts_canonical_syntax_contract -- --exact
	$(T_HIR_EXACT_TEST) lib_test::hir_rejects_broken_syntax_contract -- --exact

formal-hir-gate:
	$(T_HIR_EXACT_TEST) lib_test::formal_hir_syntax_output_resolves_interface_surface -- --exact
