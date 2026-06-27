# Terlan standard-library validation targets.
#
# This file is included by the root Makefile. Public target names remain
# callable from the repository root while stdlib recipes live with stdlib
# sources and policy documents.

.PHONY: stdlib-help stdlib-check stdlib-release-check stdlib-build-interfaces stdlib-doc-format-check stdlib-summary-inventory-check stdlib-summary-drift-check stdlib-js-bindings-drift-check stdlib-js-review-surface-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-io-negative-api-tests-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-data-check stdlib-db-check stdlib-http-check stdlib-log-check stdlib-sync-check stdlib-release-contracts-check stdlib-release-tests

stdlib-help:
	@echo "  make stdlib-check      - verify fast stdlib drift, manifest, and API coverage checks"
	@echo "  make stdlib-release-check - run stdlib-check plus release-scale stdlib tests"
	@echo "  make stdlib-build-interfaces - regenerate stdlib .typi summaries"
	@echo "  make stdlib-doc-format-check - verify stdlib TypeDoc block marker spacing"
	@echo "  make stdlib-summary-inventory-check - verify stdlib sources have checked-in summaries"
	@echo "  make stdlib-summary-drift-check - verify regenerated stdlib summaries match committed artifacts"
	@echo "  make stdlib-js-bindings-drift-check - verify generated std.js bindings match pinned TypeScript inputs"
	@echo "  make stdlib-js-review-surface-check - verify generated std.js manifests and provenance headers"
	@echo "  make stdlib-release-manifest-check - verify stdlib source/summary/test/docs release manifest"
	@echo "  make stdlib-rust-backed-manifest-check - verify Rust-backed std native operation inventory"
	@echo "  make stdlib-native-artifacts-check - verify Rust-backed std SafeNative artifacts match generated output"
	@echo "  make stdlib-io-negative-api-tests-check - verify std.io misuse diagnostics"
	@echo "  make stdlib-release-api-tests-check - verify stdlib release API examples"
	@echo "  make stdlib-negative-api-tests-check - verify constrained stdlib API diagnostics"
	@echo "  make stdlib-core-backend-primitive-calls-check - verify reviewed std.core backend primitive call inventory"
	@echo "  make stdlib-receiver-methods-check - verify receiver-shaped primitive APIs use receiver methods"
	@echo "  make stdlib-data-check - verify portable std.data API tests"
	@echo "  make stdlib-db-check - verify portable std.db API tests"
	@echo "  make stdlib-http-check - verify portable std.http API tests"
	@echo "  make stdlib-log-check - verify portable std.log API and backend lowering"
	@echo "  make stdlib-sync-check - verify portable std.sync API tests"
	@echo "  make stdlib-release-contracts-check - run release-scale stdlib typecheck sweeps"
	@echo "  make stdlib-release-tests - verify stdlib release tests"

stdlib-check: stdlib-doc-format-check stdlib-summary-inventory-check stdlib-summary-drift-check stdlib-js-bindings-drift-check stdlib-js-review-surface-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-io-negative-api-tests-check

stdlib-release-check: stdlib-check stdlib-release-contracts-check stdlib-release-tests

stdlib-build-interfaces:
	@$(PYTHON) std/scripts/build_interfaces.py

stdlib-doc-format-check:
	@$(PYTHON) std/scripts/check_doc_comment_format.py

stdlib-summary-inventory-check:
	@$(PYTHON) std/scripts/check_summary_inventory.py

stdlib-summary-drift-check:
	@$(PYTHON) std/scripts/check_summary_drift.py

stdlib-js-bindings-drift-check:
	@$(PYTHON) std/scripts/check_js_bindings_drift.py

stdlib-js-review-surface-check:
	@$(PYTHON) std/scripts/check_js_generated_review_surface.py

stdlib-release-manifest-check:
	@docs_dir=$$(mktemp -d /tmp/terlan-std-docs.XXXXXX); \
	trap 'rm -rf "$$docs_dir"' EXIT; \
	$(PYTHON) std/scripts/check_release_manifest.py --docs-dir "$$docs_dir" --generate-docs

stdlib-rust-backed-manifest-check:
	@$(PYTHON) std/scripts/check_rust_backed_manifest.py

stdlib-native-artifacts-check:
	@$(PYTHON) std/scripts/check_native_artifacts.py

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

stdlib-data-check:
	@$(TERLC) test std/data

stdlib-db-check:
	@$(TERLC) test std/db

stdlib-http-check:
	@$(TERLC) test std/http

stdlib-log-check:
	@$(TERLC) test std/log/LogTest.terl
	@$(EXACT_CARGO_TEST) -p terlan terlan_typeck::core_intrinsic_test::syntax_output_lowering_to_core_maps_all_std_log_levels_to_runtime_capability -- --exact
	@$(EXACT_CARGO_TEST) -p terlan terlan_erlang::emit::syntax_emit_test::formal_syntax_output_direct_emit_lowers_all_std_log_levels_to_runtime_capability -- --exact

stdlib-sync-check:
	@$(TERLC) test std/sync

stdlib-release-contracts-check:
	@$(EXACT_CARGO_TEST) -p terlan terlan_typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts -- --ignored --exact

stdlib-release-tests:
	@bash std/scripts/run_release_tests.sh
