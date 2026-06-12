# Terlan standard-library validation targets.
#
# This file is included by the root Makefile. Public target names remain
# callable from the repository root while stdlib recipes live with stdlib
# sources and policy documents.

.PHONY: stdlib-help stdlib-check formal-0-0-1-std-negative-api-tests-check formal-0-0-1-std-core-backend-primitive-calls-check formal-0-0-1-std-receiver-methods-check formal-0-0-1-primitive-operation-classification-check formal-0-0-1-core-primitive-intrinsics-check

stdlib-help:
	@echo "  make stdlib-check      - verify 0.0.1 stdlib release API coverage and release tests"
	@echo "  make formal-0-0-1-std-negative-api-tests-check - verify constrained stdlib API diagnostics"
	@echo "  make formal-0-0-1-std-core-backend-primitive-calls-check - verify reviewed std.core backend primitive call inventory"
	@echo "  make formal-0-0-1-std-receiver-methods-check - verify receiver-shaped primitive APIs use receiver methods"
	@echo "  make formal-0-0-1-primitive-operation-classification-check - verify primitive operation classification docs cover release APIs"
	@echo "  make formal-0-0-1-core-primitive-intrinsics-check - verify CoreIR primitive contracts cover backend cleanup obligations"

stdlib-check: formal-0-0-1-primitive-operation-classification-check formal-0-0-1-core-primitive-intrinsics-check formal-0-0-1-std-core-backend-primitive-calls-check formal-0-0-1-std-receiver-methods-check formal-0-0-1-std-api-tests-check formal-0-0-1-std-negative-api-tests-check formal-0-0-1-std-release-tests

formal-0-0-1-std-negative-api-tests-check:
	@bash scripts/check_0_0_1_std_negative_api_tests.sh

formal-0-0-1-std-core-backend-primitive-calls-check:
	@bash scripts/check_0_0_1_std_core_backend_primitive_calls.sh

formal-0-0-1-std-receiver-methods-check:
	@bash scripts/check_0_0_1_std_receiver_methods.sh

formal-0-0-1-primitive-operation-classification-check:
	@bash scripts/check_0_0_1_primitive_operation_classification.sh

formal-0-0-1-core-primitive-intrinsics-check:
	@bash scripts/check_0_0_1_core_primitive_intrinsics.sh
