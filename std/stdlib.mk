# Terlan standard-library validation targets.
#
# This file is included by the root Makefile. Public target names remain
# callable from the repository root while stdlib recipes live with stdlib
# sources and policy documents.

.PHONY: stdlib-help stdlib-check stdlib-build-interfaces stdlib-summary-inventory-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-io-negative-api-tests-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-release-tests

stdlib-help:
	@echo "  make stdlib-check      - verify stdlib release API coverage and release tests"
	@echo "  make stdlib-build-interfaces - regenerate stdlib .typi summaries"
	@echo "  make stdlib-summary-inventory-check - verify stdlib sources have checked-in summaries"
	@echo "  make stdlib-release-manifest-check - verify stdlib source/summary/test/docs release manifest"
	@echo "  make stdlib-rust-backed-manifest-check - verify Rust-backed std native operation inventory"
	@echo "  make stdlib-native-artifacts-check - verify Rust-backed std SafeNative artifacts match generated output"
	@echo "  make stdlib-io-negative-api-tests-check - verify std.io misuse diagnostics"
	@echo "  make stdlib-release-api-tests-check - verify stdlib release API examples"
	@echo "  make stdlib-negative-api-tests-check - verify constrained stdlib API diagnostics"
	@echo "  make stdlib-core-backend-primitive-calls-check - verify reviewed std.core backend primitive call inventory"
	@echo "  make stdlib-receiver-methods-check - verify receiver-shaped primitive APIs use receiver methods"
	@echo "  make stdlib-release-tests - verify stdlib release tests"

stdlib-check: stdlib-summary-inventory-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-io-negative-api-tests-check stdlib-release-tests

stdlib-build-interfaces:
	@python3 std/scripts/build_interfaces.py

stdlib-summary-inventory-check:
	@python3 std/scripts/check_summary_inventory.py

stdlib-release-manifest-check:
	@$(TERLC) --out-dir /tmp/terlan-std-docs doc std
	@python3 std/scripts/check_release_manifest.py --docs-dir /tmp/terlan-std-docs

stdlib-rust-backed-manifest-check:
	@python3 std/scripts/check_rust_backed_manifest.py

stdlib-native-artifacts-check:
	@python3 std/scripts/check_native_artifacts.py

stdlib-io-negative-api-tests-check:
	@bash std/scripts/check_io_negative_api_tests.sh

stdlib-release-api-tests-check:
	@bash std/scripts/check_release_api_tests.sh

stdlib-negative-api-tests-check:
	@bash std/scripts/check_negative_api_tests.sh

stdlib-core-backend-primitive-calls-check:
	@bash std/scripts/check_core_backend_primitive_calls.sh

stdlib-receiver-methods-check:
	@bash std/scripts/check_receiver_methods.sh

stdlib-release-tests:
	@bash std/scripts/run_release_tests.sh
