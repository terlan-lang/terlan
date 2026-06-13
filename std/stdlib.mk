# Terlan standard-library validation targets.
#
# This file is included by the root Makefile. Public target names remain
# callable from the repository root while stdlib recipes live with stdlib
# sources and policy documents.

.PHONY: stdlib-help stdlib-check stdlib-build-interfaces stdlib-summary-inventory-check stdlib-summary-generated-check

stdlib-help:
	@echo "  make stdlib-check      - verify 0.0.1 stdlib release API coverage and release tests"
	@echo "  make stdlib-build-interfaces - regenerate stdlib .typi summaries"
	@echo "  make stdlib-summary-inventory-check - verify stdlib sources have checked-in summaries"
	@echo "  make stdlib-summary-generated-check - verify checked-in summaries match generated summaries"

stdlib-check: stdlib-summary-inventory-check stdlib-summary-generated-check

stdlib-build-interfaces:
	@python3 scripts/build_stdlib_interfaces.py

stdlib-summary-inventory-check:
	@python3 scripts/check_stdlib_summary_inventory.py

stdlib-summary-generated-check:
	@python3 scripts/check_stdlib_generated_summaries.py
