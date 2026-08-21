# Terlan standard-library validation targets.
#
# This file is included by the root Makefile. Public target names remain
# callable from the repository root while stdlib recipes live with stdlib
# sources and policy documents.

.PHONY: stdlib-help stdlib-check stdlib-release-check stdlib-release-runtime-check stdlib-release-runtime-owned-by-check stdlib-build-interfaces stdlib-doc-format-check stdlib-summary-inventory-check stdlib-summary-drift-check stdlib-embedded-interface-contract-check stdlib-js-bindings-drift-check stdlib-js-review-surface-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-validation-self-test stdlib-io-negative-api-tests-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-release-tests-vm-default-check stdlib-data-check stdlib-db-check stdlib-http-check stdlib-log-check stdlib-sync-check stdlib-release-contracts-check stdlib-release-tests

stdlib-help:
	@echo "  make stdlib-check      - verify fast stdlib drift, manifest, and API coverage checks"
	@echo "  make stdlib-release-check - run stdlib-check plus release-scale stdlib tests"
	@echo "  make stdlib-build-interfaces - regenerate stdlib .typi summaries"
	@echo "  make stdlib-doc-format-check - verify stdlib TypeDoc block marker spacing"
	@echo "  make stdlib-summary-inventory-check - verify stdlib sources have checked-in summaries"
	@echo "  make stdlib-summary-drift-check - verify regenerated stdlib summaries match committed artifacts"
	@echo "  make stdlib-embedded-interface-contract-check - verify canonical embedded std summaries"
	@echo "  make stdlib-js-bindings-drift-check - verify generated std.js bindings match pinned TypeScript inputs"
	@echo "  make stdlib-js-review-surface-check - verify generated std.js manifests and provenance headers"
	@echo "  make stdlib-release-manifest-check - verify stdlib source/summary/test/docs release manifest"
	@echo "  make stdlib-rust-backed-manifest-check - verify Rust-backed std native operation inventory"
	@echo "  make stdlib-native-artifacts-check - verify Rust-backed std NativeBoundary artifacts match generated output"
	@echo "  make stdlib-io-negative-api-tests-check - verify std.io misuse diagnostics"
	@echo "  make stdlib-release-api-tests-check - verify stdlib release API examples"
	@echo "  make stdlib-negative-api-tests-check - verify constrained stdlib API diagnostics"
	@echo "  make stdlib-core-backend-primitive-calls-check - verify reviewed std.core backend primitive call inventory"
	@echo "  make stdlib-receiver-methods-check - verify receiver-shaped primitive APIs use receiver methods"
	@echo "  make stdlib-release-tests-vm-default-check - verify stdlib release tests use bare terlc test on VM default lane"
	@echo "  make stdlib-data-check - verify portable std.data API tests"
	@echo "  make stdlib-db-check - verify portable std.db API tests"
	@echo "  make stdlib-http-check - verify portable std.http API tests"
	@echo "  make stdlib-log-check - verify portable std.log API and backend lowering"
	@echo "  make stdlib-sync-check - verify portable std.sync API tests"
	@echo "  make stdlib-release-contracts-check - run release-scale stdlib typecheck sweeps"
	@echo "  make stdlib-release-tests - verify stdlib release tests"

stdlib-check: stdlib-doc-format-check stdlib-summary-inventory-check stdlib-summary-drift-check stdlib-js-bindings-drift-check stdlib-js-review-surface-check stdlib-release-manifest-check stdlib-rust-backed-manifest-check stdlib-native-artifacts-check stdlib-core-backend-primitive-calls-check stdlib-receiver-methods-check stdlib-release-tests-vm-default-check stdlib-release-api-tests-check stdlib-negative-api-tests-check stdlib-io-negative-api-tests-check

stdlib-release-check: stdlib-check stdlib-release-runtime-check

stdlib-release-runtime-check: stdlib-release-contracts-check stdlib-release-tests

stdlib-release-runtime-owned-by-check:
	@test "$(TERLAN_CHECK_ALREADY_RUN)" = "1" || { \
		echo "stdlib release runtime ownership requires TERLAN_CHECK_ALREADY_RUN=1" >&2; \
		exit 1; \
	}
	@echo "[stdlib-release-runtime] canonical check already passed contracts and VM-default release tests."

stdlib-build-interfaces:
	@TERLAN_BUILD_INTERFACES_ROOT="$(CURDIR)" \
	TERLAN_BUILD_INTERFACES_OUT_DIR="$(CURDIR)/std/summaries" \
	TERLAN_BUILD_INTERFACES_TERLC="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc test scripts/self_validation/BuildInterfacesTest.terl

stdlib-doc-format-check:
	@target/debug/terlc test scripts/self_validation/DocCommentFormatTest.terl

stdlib-summary-inventory-check:
	@target/debug/terlc test scripts/self_validation/SummaryInventoryTest.terl

stdlib-summary-drift-check:
	@TERLAN_SUMMARY_DRIFT_ROOT="$(CURDIR)" \
	TERLAN_SUMMARY_DRIFT_TERLC="$(CURDIR)/target/debug/terlc" \
	TERLAN_SUMMARY_DRIFT_GENERATOR="$(CURDIR)/scripts/self_validation/BuildInterfacesTest.terl" \
		target/debug/terlc test scripts/self_validation/SummaryDriftTest.terl

stdlib-embedded-interface-contract-check:
	@target/debug/terlc test scripts/self_validation/EmbeddedInterfaceContractsTest.terl

stdlib-js-bindings-drift-check:
	@TERLAN_JS_BINDINGS_ROOT="$(CURDIR)" \
	TERLAN_JS_BINDINGS_TERLC="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc test scripts/self_validation/JsBindingsDriftTest.terl

stdlib-js-review-surface-check:
	@TERLAN_JS_REVIEW_ROOT="$(CURDIR)" \
		target/debug/terlc test scripts/self_validation/JsGeneratedReviewSurfaceTest.terl

stdlib-release-manifest-check:
	@TERLAN_RELEASE_MANIFEST_ROOT="$(CURDIR)" \
	TERLAN_RELEASE_MANIFEST_TERLC="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc test std/scripts/ReleaseManifestTest.terl

stdlib-rust-backed-manifest-check: stdlib-native-artifacts-check
	@TERLAN_RUST_BACKED_MANIFEST_ROOT="$(CURDIR)" \
		target/debug/terlc test \
			scripts/self_validation/RustBackedManifestTest.terl \
			scripts/self_validation/RustBackedAdapterTest.terl

stdlib-native-artifacts-check:
	@TERLAN_NATIVE_ARTIFACTS_ROOT="$(CURDIR)" \
	TERLAN_NATIVE_ARTIFACTS_TERLC="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc test scripts/self_validation/NativeArtifactsTest.terl

stdlib-validation-self-test: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_STDLIB_VALIDATION) self-test

stdlib-io-negative-api-tests-check: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
	TERLAN_STDLIB_VALIDATION_TERLC="$(CURDIR)/target/debug/terlc" \
		$(TERLAN_STDLIB_VALIDATION) io-negative-api

stdlib-release-api-tests-check: stdlib-release-manifest-check
	@echo "stdlib release API coverage is owned by the typed release manifest validator."

stdlib-negative-api-tests-check: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
	TERLAN_STDLIB_VALIDATION_TERLC="$(CURDIR)/target/debug/terlc" \
		$(TERLAN_STDLIB_VALIDATION) negative-api

stdlib-core-backend-primitive-calls-check: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_STDLIB_VALIDATION) core-backend

stdlib-receiver-methods-check: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_STDLIB_VALIDATION) receiver-methods

stdlib-release-tests-vm-default-check: stdlib-validation-self-test
	@echo "stdlib release VM-default routing is owned by the typed runner."

stdlib-data-check:
	@$(TERLC) test std/data

stdlib-db-check:
	@$(TERLC) test std/db

stdlib-http-check:
	@$(TERLC) test std/http

stdlib-log-check:
	@$(TERLC) test std/log/LogTest.terl
	@$(EXACT_CARGO_TEST) -p terlan compiler::typeck::core_intrinsic_test::syntax_output_lowering_to_core_maps_all_std_log_levels_to_runtime_capability -- --exact

stdlib-sync-check:
	@$(TERLC) test std/sync

stdlib-release-contracts-check:
	@$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts -- --ignored --exact

stdlib-release-tests: | terlan-stdlib-validation-bootstrap
	@TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
	TERLAN_STDLIB_VALIDATION_TERLC="$(CURDIR)/target/debug/terlc" \
		$(TERLAN_STDLIB_VALIDATION) release-tests
