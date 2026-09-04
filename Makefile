CARGO := cargo --locked
RUST_TEST := $(CARGO) test
RELEASE_VERSION ?= $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
EXACT_CARGO_TEST := bash scripts/run_exact_cargo_test.sh
TERLAN_BOOTSTRAP_COMPILER := target/debug/terlc
TERLAN_BOOTSTRAP_COMPILER_BUILD := $(TERLAN_BOOTSTRAP_COMPILER) build --incremental
TERLAN_BOOTSTRAP_VM := target/debug/terlan-vm
TERLAN_SERVE_RUNTIME_PROFILE := serve-runtime
TERLAN_SERVE_RUNTIME_BIN := $(CURDIR)/target/$(TERLAN_SERVE_RUNTIME_PROFILE)/terlan-serve-runtime
TERLAN_SERVE_RUNTIME_BUILD := $(CARGO) build --profile $(TERLAN_SERVE_RUNTIME_PROFILE) -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin
TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS = -p terlan --bin terlc --bin terlan-vm
TERLAN_TYPED_VALIDATOR_BUILD := bash scripts/build_typed_validator.sh
TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT := target/self-validation/typed-validator-common.inputs.sha256
TERLAN_TYPED_VALIDATOR_COMMON_INPUTS := $(TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT)
TERLAN_SELF_VALIDATION_IMAGE := _build/vm/scripts_self_validation_BuildArtifactBudgetTest.tvm
TERLAN_MAKE_RECIPE_DIR := target/self-validation/make-recipe-thinness
TERLAN_MAKE_RECIPE_IMAGE := $(TERLAN_MAKE_RECIPE_DIR)/vm/scripts_self_validation_MakeRecipeThinness.tvm
TERLAN_PROOF_RELEASE_DIR := target/self-validation/proof-release-evidence
TERLAN_PROOF_RELEASE_IMAGE := $(TERLAN_PROOF_RELEASE_DIR)/vm/scripts_ProofReleaseEvidence.tvm
TERLAN_SEMANTIC_KERNEL_DIR := target/self-validation/semantic-kernels
TERLAN_SEMANTIC_KERNEL_IMAGE := $(TERLAN_SEMANTIC_KERNEL_DIR)/vm/scripts_SemanticKernels.tvm
TERLAN_EBNF_VALIDATOR_DIR := target/self-validation/ebnf-validator
TERLAN_EBNF_VALIDATOR_IMAGE := $(TERLAN_EBNF_VALIDATOR_DIR)/vm/scripts_self_validation_EbnfValidator.tvm
TERLAN_SHARED_HELPER_DIR := target/self-validation/shared-helper-check
TERLAN_SHARED_HELPER_IMAGE := $(TERLAN_SHARED_HELPER_DIR)/vm/scripts_self_validation_SharedHelperCheck.tvm
TERLAN_EXTERNAL_PACKAGE_MATRIX_DIR := target/self-validation/external-package-matrix
TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE := $(TERLAN_EXTERNAL_PACKAGE_MATRIX_DIR)/vm/scripts_self_validation_ExternalPackageExecutionMatrix.tvm
TERLAN_EXTERNAL_PACKAGE_MATRIX := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE) --script-eval --
TERLAN_TVM_PACKAGE_CONSUMER_DIR := target/self-validation/tvm-package-install-consumer
TERLAN_TVM_PACKAGE_CONSUMER_IMAGE := $(TERLAN_TVM_PACKAGE_CONSUMER_DIR)/vm/scripts_self_validation_TvmPackageInstallConsumer.tvm
TERLAN_TVM_PACKAGE_CONSUMER := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_TVM_PACKAGE_CONSUMER_IMAGE) --script-eval --
TERLAN_TVM_PLATFORM_MATRIX_DIR := target/self-validation/tvm-aot-platform-matrix
TERLAN_TVM_PLATFORM_MATRIX_IMAGE := $(TERLAN_TVM_PLATFORM_MATRIX_DIR)/vm/scripts_TvmAotPlatformMatrix.tvm
TERLAN_TVM_PLATFORM_MATRIX := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE) --script-eval --
TERLAN_RUST_QUALITY_DIR := target/self-validation/rust-quality
TERLAN_RUST_QUALITY_IMAGE := $(TERLAN_RUST_QUALITY_DIR)/vm/scripts_RustQuality.tvm
TERLAN_RUST_QUALITY := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_RUST_QUALITY_IMAGE) --script-eval --
TERLAN_RELEASE_PROMOTION_DIR := target/self-validation/release-promotion
TERLAN_RELEASE_PROMOTION_IMAGE := $(TERLAN_RELEASE_PROMOTION_DIR)/vm/scripts_ReleasePromotion.tvm
TERLAN_RELEASE_PROMOTION := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_RELEASE_PROMOTION_IMAGE) --script-eval --
TERLAN_RELEASE_CLOSEOUT_DIR := target/self-validation/release-closeout
TERLAN_RELEASE_CLOSEOUT_IMAGE := $(TERLAN_RELEASE_CLOSEOUT_DIR)/vm/scripts_ReleaseCloseout.tvm
TERLAN_RELEASE_CLOSEOUT := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_RELEASE_CLOSEOUT_IMAGE) --script-eval --
TERLAN_WEB_MANIFEST_PREFLIGHT_DIR := target/self-validation/web-manifest-preflight
TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE := $(TERLAN_WEB_MANIFEST_PREFLIGHT_DIR)/vm/scripts_self_validation_WebManifestPreflight.tvm
TERLAN_WEB_MANIFEST_PREFLIGHT := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE) --script-eval --
TERLAN_SELF_VALIDATION_CHECKOUT_DIR := target/self-validation/clean-checkout
TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE := $(TERLAN_SELF_VALIDATION_CHECKOUT_DIR)/vm/scripts_self_validation_SelfValidationCheckout.tvm
TERLAN_SELF_VALIDATION_CHECKOUT := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE) --script-eval --
TERLAN_STDLIB_VALIDATION_DIR := target/self-validation/stdlib-validation
TERLAN_STDLIB_VALIDATION_IMAGE := $(TERLAN_STDLIB_VALIDATION_DIR)/vm/scripts_self_validation_StdlibValidation.tvm
TERLAN_STDLIB_VALIDATION := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_STDLIB_VALIDATION_IMAGE) --script-eval --
TERLAN_REPOSITORY_VALIDATION_DIR := target/self-validation/repository-validation
TERLAN_REPOSITORY_VALIDATION_IMAGE := $(TERLAN_REPOSITORY_VALIDATION_DIR)/vm/scripts_self_validation_RepositoryValidation.tvm
TERLAN_REPOSITORY_VALIDATION := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_REPOSITORY_VALIDATION_IMAGE) --script-eval --
TERLAN_RUST_QUALITY_TIMEOUT_SECONDS := 300
TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB := 131072
TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX := 1
TERLAN_TVM_PLATFORM_REPORT_ROOT ?= target/quality/tvm-aot-platform-input
TERLAN_SHARED_HELPER_TIMEOUT_SECONDS := 300
TERLAN_SHARED_HELPER_VIRTUAL_MEMORY_KIB := 131072
TERLAN_VALIDATOR_BUILD_JOBS ?= 2
ifeq ($(filter 1 2,$(TERLAN_VALIDATOR_BUILD_JOBS)),)
$(error TERLAN_VALIDATOR_BUILD_JOBS must be 1 or 2)
endif
DOCS_STATIC_RELEASE_PARITY_DIR := target/self-validation/docs-static-release-parity
DOCS_STATIC_RELEASE_PARITY_IMAGE := $(DOCS_STATIC_RELEASE_PARITY_DIR)/vm/docs_static_release_parity_Main.tvm
EDITOR_RELEASE_PARITY_DIR := $(DOCS_STATIC_RELEASE_PARITY_DIR)
EDITOR_RELEASE_PARITY_IMAGE := $(DOCS_STATIC_RELEASE_PARITY_IMAGE)
TERLAN_COMPILER_CONSUMER_GATES := \
	terlan-self-validation-inventory-check \
	terlan-self-validation-capabilities-check \
	terlan-format-check \
	editor-release-parity-check \
	docs-static-release-parity-check \
	terlan-make-recipe-thinness-check \
	terlan-benchmark-framework-check \
	single-root-contract-check \
	safe-rust-runtime-check \
	abi1-pre-freeze-check \
	lalrpop-parser-parity-check \
	lean-proof-parser-shape-check \
	tvm-aot-package-install-consumer-check \
	tvm-aot-multicore-migration-check \
	vm-multicore-fixed-placement-check \
	vm-multicore-mailbox-publication-check \
	vm-multicore-replay-observability-check \
	vm-multicore-runtime-cleanup-check \
	vm-multicore-work-stealing-policy-check \
	vm-actor-mutator-ownership-check \
	runtime-aot-only-check \
	terlan-vm-test-command-check \
	vm-coordination-docker-check \
	all-terlan-tests-vm-inventory-check \
	callable-syntax-cleanup-check \
	generated-package-contract-check \
	cpp-package-consumer-check \
	cuda-package-check \
	accelerator-hard-contract-check \
	accelerator-boundary-baseline-check \
	accelerator-package-metadata-check \
	accelerator-value-contract-check \
	accelerator-target-admission-check \
	accelerator-ir-check \
	accelerator-aot-backend-check \
	accelerator-placement-check \
	accelerator-vm-integration-check \
	accelerator-specialized-artifact-check \
	vm-sql-macro-validation-check \
	vm-in-memory-stream-check \
	static-route-boundary-check \
	html-boundary-check \
	http-runtime-stack-check \
	angular-ts-terlan-integration-check \
	angular-ts-namespace-generation-check \
	angular-ts-terlan-app-ownership-check \
	changelog-public-scope-check \
	rust-code-quality-adversarial-check \
	rust-artifact-retention-check \
	rust-artifact-retention-clean-shared-debug \
	release-version-metadata-check \
	release-version-bump \
	binary-syntax-scaffold-check \
	binary-protocol-benchmark-check \
	lean-proof-smoke-check \
	lean-proof-counterexample-check \
	lean-proof-feature-cull-check \
	proof-repro-check \
	shape-implications-check \
	tvm-aot-runtime-workload-benchmark-check \
	tvm-http-paired-performance-check \
	tvm-aot-compilation-benchmark-check \
	vm-float-native-arithmetic-check \
	vm-multicore-performance-check \
	std-test-table-check \
	std-test-property-check \
	std-range-check \
	std-random-check \
	std-regex-check \
	executable-docs-vm-check \
	vm-http-vs-axum-check \
	vm-semantics-vs-otp-check \
	cli-build \
	cli-test-full \
	cli-terlc-build-executable-check \
	cli-terlan-vm-compiler-bridge-check \
	db-command-check \
	sql-form-check \
	stdlib-build-interfaces \
	stdlib-doc-format-check \
	stdlib-summary-inventory-check \
	stdlib-summary-drift-check \
	stdlib-embedded-interface-contract-check \
	stdlib-js-bindings-drift-check \
	stdlib-js-review-surface-check \
	stdlib-release-manifest-check \
	stdlib-rust-backed-manifest-check \
	stdlib-native-artifacts-check \
	stdlib-release-tests-vm-default-check
TERLAN_NDARRAY_DIR ?= ../terlan-ndarray
TERLAN_CUDA_DIR ?= ../terlan-cuda
TERLAN_POLARS_DIR ?=
TERLAN_PYTORCH_DIR ?=
TERLAN_PYTORCH_LIBTORCH ?=
export TERLAN_CUDA_DIR
SHELL := bash
.SHELLFLAGS := -eo pipefail -c

.PHONY: tvm-native-image-format-check tvm-direct-aot-backend-check tvm-aot-application-closure-check tvm-aot-case-lowering-check tvm-aot-higher-order-specialization-check tvm-aot-lowering-coverage-check tvm-aot-managed-continuation-check tvm-aot-owned-closure-representation-check tvm-aot-static-callable-check tvm-aot-thread-neutral-continuation-check tvm-aot-typed-lifecycle-check tvm-aot-typed-mailbox-check tvm-managed-memory-check tvm-native-image-loader-check tvm-aot-consumer-check tvm-aot-test-consumer-check tvm-aot-repl-consumer-check tvm-aot-debugger-consumer-check tvm-aot-hot-reload-consumer-check tvm-aot-package-install-consumer-check tvm-aot-support-crash-metadata-check tvm-aot-platform-target-check tvm-aot-platform-matrix-check tvm-aot-http-managed-cycle-check tvm-aot-http-request-accessor-check tvm-aot-http-response-mutation-check tvm-aot-http-typed-metadata-check tvm-aot-http-router-callable-check tvm-aot-http-managed-error-check tvm-aot-http-template-check tvm-aot-http-template-render-plan-check tvm-aot-http-template-expression-check tvm-aot-http-body-json-check tvm-aot-http-session-check tvm-aot-http-managed-boundary-check tvm-aot-http-channel-plan-check tvm-aot-http-persistent-shard-check tvm-aot-http-native-invocation-check tvm-aot-http-websocket-invocation-check tvm-aot-http-sse-invocation-check tvm-aot-http-generation-lifetime-check tvm-aot-http-channel-transport-check tvm-aot-http-cleanup-check tvm-aot-http-lifecycle-inventory-check tvm-aot-http-checked-coreir-reference-record tvm-aot-http-performance-check tvm-single-image-artifact-check tvm-aot-runtime-transition-check tvm-aot-runtime-transition-focused-check tvm-aot-compilation-benchmark-check tvm-aot-compilation-time-check tvm-aot-capability-worker-check

.PHONY: docs-light-check rust-security-audit-check terlan-serve-runtime-bootstrap terlan-http-benchmark-release-bootstrap

terlan-serve-runtime-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	test -x $(TERLAN_SERVE_RUNTIME_BIN)
else
	$(TERLAN_SERVE_RUNTIME_BUILD)
endif

terlan-http-benchmark-release-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	@for binary in terlan-axum-baseline terlan-hyper-baseline terlan-http-framework-benchmark terlan-http-paired-benchmark; do \
		test -x "target/release/$$binary"; \
	done
else
	$(CARGO) build --release -p terlan --bin terlan-axum-baseline --bin terlan-hyper-baseline --bin terlan-http-framework-benchmark --bin terlan-http-paired-benchmark --features axum-baseline,benchmark-tools
endif
docs-light-check: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) docs-light-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) docs-light-check

rust-security-audit-check:
	@cargo-audit --version | grep -Fx 'cargo-audit 0.22.2'
	cargo-audit audit --deny warnings
.PHONY: tvm-aot-application-conformance-check tvm-aot-c-abi-boundary-check tvm-aot-closure-dispatch-check tvm-aot-crash-injection-check tvm-aot-image-lifetime-check tvm-aot-multicore-readiness-check tvm-aot-thread-sanitizer-check
.PHONY: std-vm-parity-matrix-check otp-stdlib-port-check vm-distribution-suite-parity-check vm-multicore-invariant-inventory-check terlan-self-validation-inventory-check terlan-self-validation-capabilities-check terlan-self-validation-clean-checkout-check terlan-self-validation-check editor-release-parity-check docs-static-release-parity-check
.PHONY: tvm-aot-managed-field-projection-check tvm-aot-platform-matrix-contract-check tvm-aot-thread-sanitizer-contract-check tvm-aot-release-closeout-contract-check tvm-aot-release-closeout-check tvm-aot-publish-evidence-check
.PHONY: tail-recursion-lowering-check termination-productivity-analysis-check binding-shadowing-safety-check
.PHONY: no-tvm-json-runtime-check no-vmir-interpreter-check runtime-aot-only-check
.PHONY: vm-debug-key-compatibility-check
.PHONY: vm-latin1-source-policy-check
.PHONY: vm-ignore-cores-parity-check
.PHONY: vm-iovec-suite-parity-check
.PHONY: vm-lcnt-suite-parity-check
.PHONY: vm-list-bif-suite-parity-check
.PHONY: vm-literal-area-collector-parity-check
.PHONY: vm-lttng-suite-parity-check
.PHONY: vm-map-suite-parity-check
.PHONY: vm-match-spec-suite-parity-check
.PHONY: vm-module-info-suite-parity-check
.PHONY: vm-mtx-suite-parity-check
.PHONY: vm-multi-load-suite-parity-check
.PHONY: vm-native-record-suite-parity-check
.PHONY: vm-nif-suite-parity-check
.PHONY: vm-compiler-transform-retirement-check
.PHONY: vm-source-column-ownership-check
.PHONY: vm-source-provenance-artifact-check
.PHONY: vm-call-dependency-artifact-check
.PHONY: vm-executable-source-span-artifact-check
.PHONY: accelerator-hard-contract-check accelerator-boundary-baseline-check accelerator-package-metadata-check accelerator-value-contract-check
.PHONY: terlan-ndarray-abi-check terlan-ndarray-operations-check terlan-ndarray-blas-check ndarray-dlpack-interop-check terlan-ndarray-release-check
.PHONY: vm-source-hot-reload-check
.PHONY: vm-distributed-scheduling-check
.PHONY: vm-distributed-state-check
.PHONY: vm-supervision-restart-check
.PHONY: vm-timer-deadline-check
.PHONY: vm-scheduler-fairness-check vm-actor-mutator-ownership-check vm-parallel-messages-suite-parity-check vm-efile-suite-parity-check vm-float-native-arithmetic-check vm-fun-suite-parity-check vm-gc-suite-parity-check vm-guard-suite-parity-check vm-guard-no-opt-suite-parity-check vm-hash-suite-parity-check vm-hello-suite-parity-check vm-hibernate-suite-parity-check vm-small-suite-parity-check vm-smoke-suite-parity-check vm-multicore-mailbox-publication-check vm-multicore-fixed-placement-check tvm-aot-multicore-migration-check vm-multicore-work-stealing-policy-check vm-multicore-work-stealing-check vm-multicore-runtime-cleanup-check tvm-aot-multicore-io-epoch-check vm-multicore-timer-epoch-check vm-multicore-timer-scheduler-check vm-multicore-protocol-reactor-check vm-multicore-capability-worker-check vm-multicore-capability-completion-check vm-multicore-capability-event-pump-check vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-multicore-runtime-integration-check vm-multicore-replay-observability-check vm-multicore-performance-record vm-multicore-performance-check vm-multicore-memory-model-check vm-multicore-thread-sanitizer-contract-check vm-multicore-thread-sanitizer-check vm-multicore-mc9-evidence-contract-check vm-multicore-mc9-evidence-check vm-multicore-mc9-local-evidence-check vm-multicore-release-contract-check vm-multicore-release-record vm-multicore-release-check vm-multicore-publish-evidence-refresh vm-multicore-publish-check
.PHONY: tvm-http-axum-performance-record tvm-http-paired-performance-check tvm-http-decisive-performance-check
.PHONY: vm-memory-heap-pressure-check
.PHONY: std-generated-metadata-check
.PHONY: std-test-honesty-check
.PHONY: std-test-table-check
.PHONY: std-test-property-check
.PHONY: std-range-check
.PHONY: std-random-check
.PHONY: std-regex-check
.PHONY: rust-build-feature-shipping-check
.PHONY: language-feature-coverage-100-check operator-coverage-100-check pattern-matching-support-check string-pattern-matching-check string-pattern-long-tail-check binary-bitstring-processing-check binary-syntax-scaffold-check binary-runtime-suite-check binary-descriptor-check binary-descriptor-contract-check binary-error-taxonomy-check binary-protocol-helper-check binary-protocol-benchmark-check
.PHONY: core-type-contracts-check
.PHONY: type-alias-shorthand-check
.PHONY: compiler-purity-metadata-check
.PHONY: comprehension-guards-check
.PHONY: lalrpop-parser-parity-check lean-proof-parser-shape-check lean-proof-native-boundary-check lean-proof-track-check lean-proof-smoke-check lean-proof-feature-binding-check lean-proof-change-impact-report lean-proof-feature-binding-review lean-proof-snapshot-consistency-check lean-proof-counterexample-check lean-proof-track-gap-hygiene-check lean-proof-feature-cull-check proof-repro-check proof_repro_check lean-proof-track-pr-gate lean-proof-track-regression-check lean-proof-track-runtime-check lean-proof-track-release-closeout-check lean-proof-templates-routes-check lean-proof-concurrency-check lean-proof-collections-check lean-proof-wasm-bridge-check lean-proof-db-sql-check lean-proof-std-package-check lean-proof-semantic-kernels-check release-artifacts-closeout-check proof-coverage-release-artifacts-smoke proof-readiness-release-mode-check release-evidence-compose release-evidence-refresh release-preflight release-check publish-evidence-refresh publish-evidence-check publish-evidence-plan-check publish-evidence-refresh-plan-check aot-developer-hot-reload-check
.PHONY: function-head-pattern-parameters-check function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-migration-docs-check function-head-pattern-0-0-7-handoff-check function-head-pattern-parameters-hardening-check
.PHONY: syntax-contract-check shape-implications-check
.PHONY: shape-synonyms-check
.PHONY: wasm-coreir-lowering-check wasm-runtime-exec-check wasm-contract-discovery-check
.PHONY: flexible-shape-guards-check
.PHONY: callable-syntax-cleanup-check
.PHONY: release-version-channel-check release-version-bump
.PHONY: editor-definition-navigation-check editor-code-action-auto-import-check
.PHONY: terlan-lint-style-profile-check terlan-lint-pipe-canonicalization-check
.PHONY: upgrade-local update-terlc
.PHONY: dormant-runtime-code-check rust-module-structure-check rust-structure-census-check rust-structure-census-record-timings rust-build-graph-boundary-check
.PHONY: rust-format-check rust-locked-binary-check rust-clippy-check rust-workspace-policy-check
.PHONY: rust-lint-allowance-check
.PHONY: rust-code-quality-adversarial-check rust-code-quality-preflight-check rust-api-boundary-quality-check
.PHONY: vm-http-stack-check vm-http-in-memory-transport-check vm-in-memory-stream-check vm-tcp-framing-check vm-http-static-streaming-check vm-http-concurrency-hot-reload-check
.PHONY: vm-http-router-middleware-check vm-http-sse-check vm-http-websocket-source-check vm-http-websocket-upgrade-check vm-http-websocket-queue-check vm-http-websocket-policy-check vm-http-websocket-tls-check vm-http-websocket-termination-check vm-http-live-channel-source-check
.PHONY: vm-native-worker-runtime-check vm-io-reactor-runtime-check
.PHONY: vm-http-handler-scheduler-fairness-check vm-http-stateful-actor-session-check vm-live-template-stream-check vm-live-template-client-protocol-check
.PHONY: cpp-binding-metadata-extractor-check cpp-binding-metadata-extractor-live-check cpp-binding-build-plan-check cpp-binding-value-record-check cpp-binding-copied-containers-check cpp-binding-enum-check cpp-binding-exception-check cpp-package-consumer-check
.PHONY: vm-http-benchmark-comparability-check vm-http-runtime-attribution-check vm-http-soak-stability-check
.PHONY: vm-http-acme-tls-base-check vm-http-acme-tls-production-check vm-http-protocol-readiness-check
.PHONY: vm-otp-abstractions-terlan-stdlib-check

ifeq ($(TERLAN_RUST_SUITE_ALREADY_RUN),1)
RUST_TEST := true
EXACT_CARGO_TEST := true
endif

include crates/terlan/cli.mk
include std/stdlib.mk
include editors/editor.mk
include mk/code-quality.mk

COVERAGE_MIN ?= 84.20
COVERAGE_VM_RUNNER ?= $(CURDIR)/target/debug/terlan-vm
COVERAGE_IGNORE_FILENAME_REGEX ?= crates/terlan/src/(lsp|vm)/\.\./

ifeq ($(TERLAN_CHECK_ALREADY_RUN),1)
TEST_RELEASE_STDLIB_TARGET := stdlib-release-runtime-owned-by-check
LEAN_PROOF_CLOSEOUT_DEPS :=
VM_HTTP_BENCHMARK_COMPARABILITY_DEPS :=
else
TEST_RELEASE_STDLIB_TARGET := stdlib-release-check
LEAN_PROOF_CLOSEOUT_DEPS := lean-proof-track-check
VM_HTTP_BENCHMARK_COMPARABILITY_DEPS := vm-http-concurrency-investigation-check
endif

HTTP_SOAK_PROFILE ?= short
HTTP_SOAK_REPORT = target/quality/vm-http-soak-$(if $(filter release,$(HTTP_SOAK_PROFILE)),release-,)stability-report.json

ifneq ($(filter publish publish-preflight,$(MAKECMDGOALS)),)
VERSION ?= $(RELEASE_VERSION)
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

CHECK_GATES := \
	terlan-self-validation-inventory-check \
	terlan-self-validation-capabilities-check \
	terlan-format-check \
	terlan-make-recipe-thinness-check \
	terlan-benchmark-framework-check \
	rust-structure-census-check \
	rust-code-quality-preflight-check \
	build-artifact-budget-check \
	aot-developer-hot-reload-check \
	terlan-lint-pipe-canonicalization-check \
	stdlib-check \
	language-feature-coverage-100-check \
	lean-proof-track-check \
	runtime-aot-only-check \
	terlan-vm-artifact-format-check \
	tvm-native-image-format-check \
	tvm-direct-aot-backend-check \
	tvm-aot-application-closure-check \
	binding-shadowing-safety-check \
	tvm-aot-managed-continuation-check \
	tvm-aot-thread-neutral-continuation-check \
	tvm-managed-memory-check \
	tvm-native-image-loader-check \
	tvm-aot-consumer-check \
	tvm-single-image-artifact-check \
	tvm-aot-runtime-transition-focused-check \
	tvm-aot-shard-ownership-check \
	tvm-aot-supervisor-lifecycle-check \
	tvm-aot-stale-epoch-check \
	tvm-aot-crash-injection-check \
	tvm-aot-capability-worker-check \
	tvm-aot-multicore-readiness-check \
	vm-multicore-mc9-evidence-contract-check \
	cpp-binding-generator-check \
	generated-package-contract-check \
	cuda-package-availability-check \
	external-package-execution-matrix-check \
	no-tvm-json-runtime-check \
	no-vmir-interpreter-check

# A validation cycle owns one compiler bootstrap and one build of each typed
# validation artifact. Every aggregate gate waits for this boundary, including
# under parallel Make execution, and later recipes execute the sealed image
# directly instead of recompiling its source through `terlc run`. Only `check`
# and aggregate targets whose parent owns this prerequisite set
# `TERLAN_VALIDATION_BOOTSTRAPPED=1`; nested Make processes therefore trust the
# already-verified boundary instead of repeating fingerprints and file probes.
.PHONY: terlan-compiler-bootstrap terlan-quality-tools-bootstrap terlan-quality-bootstrap terlan-benchmark-release-bootstrap terlan-native-worker-bootstrap terlan-typed-validator-fingerprint terlan-artifact-measurement-bootstrap terlan-make-recipe-bootstrap terlan-self-validation-bootstrap terlan-semantic-kernel-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap
terlan-compiler-bootstrap:
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
	test -x $(TERLAN_BOOTSTRAP_COMPILER)
	test -x $(TERLAN_BOOTSTRAP_VM)
else
	$(CARGO) build $(TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS)
endif
endif

# Seal the quality and structural-analysis CLIs in one Cargo invocation. The
# release aggregate orders this after the canonical suite, while a focused
# quality target can request the tools without running the entire test suite.
terlan-quality-tools-bootstrap:
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
	@for binary in \
		terlan-quality \
		terlan-native-target-feasibility \
		terlan-lean-proof-closeout \
		terlan-accelerator-value-contract \
		terlan-accelerator-target-admission \
		terlan-accelerator-ir \
		terlan-accelerator-aot-backend \
		terlan-accelerator-placement \
		terlan-accelerator-vm-integration \
		terlan-accelerator-specialized-artifact \
		terlan-rust-boundary-audit; do \
		test -x "target/debug/$$binary" || { \
			echo "error: missing prebuilt release Rust tool: target/debug/$$binary" >&2; \
			exit 1; \
		}; \
	done
else
	$(CARGO) build \
		-p terlan \
		-p terlan-rust-boundary-audit \
		--bin terlan-quality \
		--bin terlan-native-target-feasibility \
		--bin terlan-lean-proof-closeout \
		--bin terlan-accelerator-value-contract \
		--bin terlan-accelerator-target-admission \
		--bin terlan-accelerator-ir \
		--bin terlan-accelerator-aot-backend \
		--bin terlan-accelerator-placement \
		--bin terlan-accelerator-vm-integration \
		--bin terlan-accelerator-specialized-artifact \
		--bin terlan-rust-boundary-audit \
		--features terlan/quality-tools
endif
endif

terlan-benchmark-release-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	test -x target/release/terlc
	test -x target/release/terlan-vm
	test -x target/release/terlan-benchmark
else
	$(CARGO) build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark --features benchmark-tools
endif

# GNU Make 4.3 treats its global-serialization special target as process-wide.
# Keep this target's required ordering explicit without disabling safe
# parallelism elsewhere in the build graph.
terlan-quality-bootstrap: rust-test-suite
	$(MAKE) --no-print-directory --jobs=1 terlan-quality-tools-bootstrap

terlan-native-worker-bootstrap:
ifneq ($(filter 1,$(TERLAN_BUILD_ARTIFACTS_PREBUILT) $(TERLAN_RUST_SUITE_ALREADY_RUN)),)
	test -x target/debug/terlan-native-worker
else
	$(CARGO) build -p terlan --bin terlan-native-worker
endif

terlan-typed-validator-fingerprint: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) fingerprint \
		$(TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT) \
		$(TERLAN_BOOTSTRAP_COMPILER) std
endif

terlan-make-recipe-bootstrap terlan-semantic-kernel-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-release-promotion-bootstrap terlan-release-closeout-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap: terlan-typed-validator-fingerprint

terlan-artifact-measurement-bootstrap: terlan-compiler-bootstrap
ifeq ($(filter 1,$(TERLAN_VALIDATION_BOOTSTRAPPED) $(TERLAN_ARTIFACT_MEASUREMENT_ALREADY_BUILT)),)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SELF_VALIDATION_IMAGE) \
		$(TERLAN_BOOTSTRAP_COMPILER) std scripts/self_validation/BuildArtifactBudgetTest.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/BuildArtifactBudgetTest.terl
	test -s $(TERLAN_SELF_VALIDATION_IMAGE)
endif

terlan-make-recipe-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_MAKE_RECIPE_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/MakeRecipeThinness.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/MakeRecipeThinness.terls \
		--target terlan-vm --out-dir $(TERLAN_MAKE_RECIPE_DIR)
	test -s $(TERLAN_MAKE_RECIPE_IMAGE)
endif

terlan-semantic-kernel-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SEMANTIC_KERNEL_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/proof_release_evidence -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/proof_release_evidence/scripts/SemanticKernels.terls \
		--target terlan-vm --out-dir $(TERLAN_SEMANTIC_KERNEL_DIR)
	test -s $(TERLAN_SEMANTIC_KERNEL_IMAGE)
endif

terlan-ebnf-validator-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_EBNF_VALIDATOR_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/EbnfValidator.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/EbnfValidator.terls \
		--target terlan-vm --out-dir $(TERLAN_EBNF_VALIDATOR_DIR)
	test -s $(TERLAN_EBNF_VALIDATOR_IMAGE)
endif

terlan-shared-helper-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SHARED_HELPER_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/SharedHelperCheck.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/SharedHelperCheck.terls \
		--target terlan-vm --out-dir $(TERLAN_SHARED_HELPER_DIR)
	test -s $(TERLAN_SHARED_HELPER_IMAGE)
endif

terlan-external-package-matrix-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/ExternalPackageExecutionMatrix.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/ExternalPackageExecutionMatrix.terls \
		--target terlan-vm --out-dir $(TERLAN_EXTERNAL_PACKAGE_MATRIX_DIR)
	test -s $(TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE)
endif

terlan-tvm-package-consumer-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_TVM_PACKAGE_CONSUMER_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/TvmPackageInstallConsumer.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/TvmPackageInstallConsumer.terls \
		--target terlan-vm --out-dir $(TERLAN_TVM_PACKAGE_CONSUMER_DIR)
	test -s $(TERLAN_TVM_PACKAGE_CONSUMER_IMAGE)
endif

terlan-tvm-platform-matrix-bootstrap: terlan-compiler-bootstrap terlan-tvm-package-consumer-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/tvm_aot_platform_matrix -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/tvm_aot_platform_matrix/scripts/TvmAotPlatformMatrix.terls \
		--target terlan-vm --out-dir $(TERLAN_TVM_PLATFORM_MATRIX_DIR)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
endif

terlan-rust-quality-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RUST_QUALITY_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/rust_quality -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/rust_quality/scripts/RustQuality.terls \
		--target terlan-vm --out-dir $(TERLAN_RUST_QUALITY_DIR)
	test -s $(TERLAN_RUST_QUALITY_IMAGE)
endif

terlan-release-promotion-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RELEASE_PROMOTION_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/release_promotion -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/release_promotion/scripts/ReleasePromotion.terls \
		--target terlan-vm --out-dir $(TERLAN_RELEASE_PROMOTION_DIR)
	test -s $(TERLAN_RELEASE_PROMOTION_IMAGE)
endif

.PHONY: terlan-release-closeout-bootstrap release-example-projects-check release-project-upgrade-matrix-check release-reference-app-suite-check release-diagnostic-catalog-check release-compatibility-baseline-check release-notes-accuracy-check release-supply-chain-provenance-check release-security-hardening-check release-support-bundle-check release-performance-baseline-check release-adversarial-corpus-check release-mutation-check release-fault-injection-check release-readiness-attestation-check release-readiness-attestation-refresh release-staged-distribution-verification-check release-staged-distribution-verification-refresh
ifeq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
terlan-release-closeout-bootstrap:
	test -s $(TERLAN_RELEASE_CLOSEOUT_IMAGE)
else
terlan-release-closeout-bootstrap: terlan-compiler-bootstrap vm-release-artifact-matrix-check
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RELEASE_CLOSEOUT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/release_closeout -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/release_closeout/scripts/ReleaseCloseout.terls \
		--target terlan-vm --out-dir $(TERLAN_RELEASE_CLOSEOUT_DIR)
	test -s $(TERLAN_RELEASE_CLOSEOUT_IMAGE)
endif

release-example-projects-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) examples
	test -s target/quality/release-example-projects-report.json
	@rg -q '"decision": "pass"' target/quality/release-example-projects-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-example-projects-report.json

release-project-upgrade-matrix-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) upgrade
	test -s target/quality/release-project-upgrade-matrix-report.json
	@rg -q '"decision": "pass"' target/quality/release-project-upgrade-matrix-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-project-upgrade-matrix-report.json

release-reference-app-suite-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) reference-apps
	test -s target/quality/release-reference-app-suite-report.json
	@rg -q '"decision": "pass"' target/quality/release-reference-app-suite-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-reference-app-suite-report.json

release-diagnostic-catalog-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) diagnostic-catalog
	test -s target/quality/release-diagnostic-catalog-report.json
	@rg -q '"decision": "pass"' target/quality/release-diagnostic-catalog-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-diagnostic-catalog-report.json

release-compatibility-baseline-check: release-diagnostic-catalog-check
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) compatibility-baseline
	test -s target/quality/release-compatibility-baseline-report.json
	@rg -q '"decision": "pass"' target/quality/release-compatibility-baseline-report.json

release-notes-accuracy-check: release-compatibility-baseline-check
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) release-notes
	test -s target/quality/release-notes-accuracy-report.json
	@rg -q '"decision": "pass"' target/quality/release-notes-accuracy-report.json

release-supply-chain-provenance-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) supply-chain
	test -s target/quality/release-sbom.cargo-metadata.json
	test -s target/quality/release-unsafe-inventory.json
	test -s target/quality/release-supply-chain-provenance-report.json
	@rg -q '"decision": "pass"' target/quality/release-supply-chain-provenance-report.json
	@rg -q '"classification": "reviewed-subsystem-boundary:path"' target/quality/release-unsafe-inventory.json
	@! rg -q '":crates/' target/quality/release-unsafe-inventory.json

release-security-hardening-check: release-supply-chain-provenance-check
	$(RUST_TEST) -p terlan --lib --features quality-tools native_boundary_security_test
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_web_security_policy_test
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_web_config_secret_boundary_test
	$(RUST_TEST) -p terlan --lib --features quality-tools package_capability_contract_test
	$(RUST_TEST) -p terlan --lib --features quality-tools package_cache_integrity_test
	$(TERLAN_QUALITY) native-boundary-security
	$(TERLAN_QUALITY) vm-web-security-policy
	$(TERLAN_QUALITY) vm-web-config-secret-boundary
	$(TERLAN_QUALITY) package-capability-contract
	$(TERLAN_QUALITY) package-cache-integrity
	test -s target/quality/vm-release-artifact-matrix-report.json
	@rg -q '"decision": "pass"' target/quality/vm-release-artifact-matrix-report.json
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) security
	test -s target/quality/release-security-hardening-report.json
	@rg -q '"decision": "pass"' target/quality/release-security-hardening-report.json

release-support-bundle-check: release-security-hardening-check
	$(RUST_TEST) -p terlan --lib commands::support_bundle
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) support-bundle
	test -s target/quality/release-support-bundle-report.json
	test -s target/quality/release-support-bundles/installed-project.json
	@rg -q '"decision": "pass"' target/quality/release-support-bundle-report.json
	@! rg -q 'release-secret-must-not-leak|$(CURDIR)|/tmp/terlan-release-' target/quality/release-support-bundles/installed-project.json

release-performance-baseline-check: release-support-bundle-check
	test -s benchmarks/release_0_0_7.baseline.json
	test -s target/quality/http-paired-performance.json
	test -s target/quality/vm-multicore-performance.json
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) performance
	test -s target/quality/release-performance-baseline-report.json
	@rg -q '"decision": "pass"' target/quality/release-performance-baseline-report.json
	@rg -q '"correctness_assertion": "command-status-zero-and-output-contract"' target/quality/release-performance-baseline-report.json

release-adversarial-corpus-check: release-performance-baseline-check
	test -s tests/release/ADVERSARIAL_CORPUS.tsv
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) adversarial
	test -s target/quality/release-adversarial-corpus-report.json
	@rg -q '"decision": "pass"' target/quality/release-adversarial-corpus-report.json
	@rg -q '"stale_entries": \[\]' target/quality/release-adversarial-corpus-report.json

release-mutation-check: release-adversarial-corpus-check
	test -s tests/release/MUTATIONS.tsv
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) mutation
	test -s target/quality/release-mutation-check-report.json
	@rg -q '"decision": "pass"' target/quality/release-mutation-check-report.json
	@rg -q '"survived": \[\]' target/quality/release-mutation-check-report.json

release-fault-injection-check: release-mutation-check
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_web_lifecycle_health_test
	$(TERLAN_QUALITY) vm-web-lifecycle-health
	test -s target/quality/vm-multicore-release-closeout.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) fault
	test -s target/quality/release-fault-injection-report.json
	@rg -q '"decision": "pass"' target/quality/release-fault-injection-report.json
	@rg -q '"skip_reasons": \[\]' target/quality/release-fault-injection-report.json

release-readiness-attestation-check: release-example-projects-check release-project-upgrade-matrix-check release-reference-app-suite-check release-notes-accuracy-check release-fault-injection-check terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) readiness --version $(RELEASE_VERSION)
	test -s target/quality/release-readiness-attestation-report.json
	@rg -q '"decision": "pass"' target/quality/release-readiness-attestation-report.json
	@rg -q '"publication_required": false' target/quality/release-readiness-attestation-report.json

# Focused invalidation owner: reseal unchanged artifacts and already-passing
# reports without replaying the gates that produced them.
release-readiness-attestation-refresh:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_RELEASE_PROMOTION_IMAGE)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) readiness --version $(RELEASE_VERSION)
	test -s target/quality/release-readiness-attestation-report.json
	@rg -q '"decision": "pass"' target/quality/release-readiness-attestation-report.json
	@rg -q '"publication_required": false' target/quality/release-readiness-attestation-report.json

release-staged-distribution-verification-check: release-readiness-attestation-check | terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) verify --version $(RELEASE_VERSION)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_TVM_PLATFORM_MATRIX) release-staged-distribution
	test -s target/quality/release-staged-distribution-verification-report.json
	@rg -q '"decision": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"failed_upgrade_rollback": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"source_checkout_required": false' target/quality/release-staged-distribution-verification-report.json

# Focused transitive dependant of readiness. This verifies the newly sealed
# candidate from existing artifacts and reports; it does not own their tests.
release-staged-distribution-verification-refresh: release-readiness-attestation-refresh | terlan-tvm-platform-matrix-bootstrap
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) verify --version $(RELEASE_VERSION)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_TVM_PLATFORM_MATRIX) release-staged-distribution
	test -s target/quality/release-staged-distribution-verification-report.json
	@rg -q '"decision": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"failed_upgrade_rollback": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"source_checkout_required": false' target/quality/release-staged-distribution-verification-report.json

terlan-web-manifest-preflight-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/WebManifestPreflight.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/WebManifestPreflight.terls \
		--target terlan-vm --out-dir $(TERLAN_WEB_MANIFEST_PREFLIGHT_DIR)
	test -s $(TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE)
endif

terlan-self-validation-checkout-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/SelfValidationCheckout.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/SelfValidationCheckout.terls \
		--target terlan-vm --out-dir $(TERLAN_SELF_VALIDATION_CHECKOUT_DIR)
	test -s $(TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE)
endif

terlan-stdlib-validation-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_STDLIB_VALIDATION_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/StdlibValidation.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/StdlibValidation.terls \
		--target terlan-vm --out-dir $(TERLAN_STDLIB_VALIDATION_DIR)
	test -s $(TERLAN_STDLIB_VALIDATION_IMAGE)
endif

terlan-docs-static-release-parity-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(DOCS_STATIC_RELEASE_PARITY_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/docs_static_release_parity -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/docs_static_release_parity \
		--target terlan-vm --out-dir $(DOCS_STATIC_RELEASE_PARITY_DIR)
	test -s $(DOCS_STATIC_RELEASE_PARITY_IMAGE)
endif

terlan-repository-validation-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_REPOSITORY_VALIDATION_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/RepositoryValidation.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/RepositoryValidation.terls \
		--target terlan-vm --out-dir $(TERLAN_REPOSITORY_VALIDATION_DIR)
	test -s $(TERLAN_REPOSITORY_VALIDATION_IMAGE)
endif

terlan-self-validation-bootstrap: terlan-compiler-bootstrap terlan-artifact-measurement-bootstrap terlan-make-recipe-bootstrap terlan-semantic-kernel-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-release-promotion-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_PROOF_RELEASE_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/proof_release_evidence -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/proof_release_evidence/scripts/ProofReleaseEvidence.terls \
		--target terlan-vm --out-dir $(TERLAN_PROOF_RELEASE_DIR)
	test -s $(TERLAN_PROOF_RELEASE_IMAGE)
endif

$(TERLAN_COMPILER_CONSUMER_GATES): | terlan-compiler-bootstrap
lean-proof-templates-routes-check lean-proof-concurrency-check lean-proof-collections-check lean-proof-wasm-bridge-check lean-proof-db-sql-check lean-proof-std-package-check lean-proof-semantic-kernels-check: | terlan-semantic-kernel-bootstrap

check: rust-test-suite
	TERLAN_RUST_SUITE_ALREADY_RUN=1 \
		$(MAKE) --no-print-directory \
		terlan-quality-tools-bootstrap
	TERLAN_RUST_SUITE_ALREADY_RUN=1 \
	TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		$(MAKE) --no-print-directory --jobs=$(TERLAN_VALIDATOR_BUILD_JOBS) \
		terlan-self-validation-bootstrap
	TERLAN_RUST_SUITE_ALREADY_RUN=1 \
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
	TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		$(MAKE) --no-print-directory \
		TERLC=$(CURDIR)/target/debug/terlc \
		TERLAN_QUALITY=$(CURDIR)/target/debug/terlan-quality \
		check-gates

check-gates: $(CHECK_GATES)

terlan-self-validation-inventory-check:
	target/debug/terlc test scripts/self_validation/InventoryTest.terl

terlan-self-validation-capabilities-check:
	target/debug/terlc test \
		std/system \
		std/io/FileTest.terl \
		std/io/DirectoryTest.terl \
		std/crypto/HashTest.terl \
		std/data/JsonTest.terl \
		std/regex/RegexTest.terl
	target/debug/terlc test std/core/StringTest.terl \
		--name contains_accepts_present_pattern \
		--name starts_with_accepts_prefix \
		--name ends_with_accepts_suffix \
		--name lowercase_converts_ascii_text \
		--name direct_length_counts_unicode_scalars \
		--name direct_byte_size_counts_utf8_bytes \
		--name trim_removes_surrounding_whitespace \
		--name trim_start_removes_leading_whitespace \
		--name trim_end_removes_trailing_whitespace \
		--name compare_orders_earlier_text_first \
		--name compare_orders_later_text_after \
		--name compare_accepts_equal_text

# One public closeout owns the Python-free source inventory, typed capability
# surface, generated std contracts, thin Make dispatch, browser-manifest
# consumers, documentation validation, and release-promotion orchestration.
# Every prerequisite reuses the compiler and sealed images from the shared
# bootstrap boundary.
terlan-self-validation-check: \
	terlan-self-validation-clean-checkout-check \
	terlan-self-validation-inventory-check \
	terlan-self-validation-capabilities-check \
	terlan-make-recipe-thinness-check \
	stdlib-summary-inventory-check \
	stdlib-summary-drift-check \
	stdlib-native-artifacts-check \
	stdlib-rust-backed-manifest-check \
	stdlib-release-manifest-check \
	docs-light-check \
	browser-package-preflight \
	web-profile-preflight \
	release-promotion-pipeline-check
	@echo "[terlan-self-validation] Python-free direct-AOT validation passed"

terlan-self-validation-clean-checkout-check: terlan-self-validation-checkout-bootstrap
	$(TERLAN_SELF_VALIDATION_CHECKOUT) "$(CURDIR)"

editor-release-parity-check: | terlan-docs-static-release-parity-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_extract_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_surface_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_packaged_smokes_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_parser_freshness_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_assemble_surface_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_lsp_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_adversarial_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_closeout_entry
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(EDITOR_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.editor_release_parity_artifact_size_entry

docs-static-release-parity-check: | terlan-docs-static-release-parity-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(DOCS_STATIC_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.run_entry

.PHONY: terlan-make-recipe-thinness-check
terlan-make-recipe-thinness-check: | terlan-make-recipe-bootstrap
	TERLAN_MAKE_RECIPE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_MAKE_RECIPE_IMAGE) --script-eval

.PHONY: terlan-benchmark-framework-check
terlan-benchmark-framework-check:
	target/debug/terlc test tests/language/BenchmarkFrameworkTest.terl
	target/debug/terlc test tests/language/BenchmarkFrameworkTest.terl \
		--bench --warmup 1 --samples 5 \
		--emit-test-manifest target/quality/benchmark-framework-discovery.json \
		--emit-test-result-manifest target/quality/benchmark-framework-result.json
	TERLAN_JSON_EVIDENCE_PATH=target/quality/benchmark-framework-result.json \
	TERLAN_JSON_EVIDENCE_REQUIRED='"kind": "benchmark";;"benchmark_samples": 5;;"benchmark_min_nanoseconds":;;"benchmark_p95_nanoseconds":' \
		target/debug/terlc test scripts/self_validation/JsonEvidenceContractTest.terl \
			--name selected_json_evidence_holds

.PHONY: check-experimental
check-experimental: check
	$(MAKE) --no-print-directory package-test-exec-check
	$(MAKE) --no-print-directory cuda-package-check
	TERLAN_EXTERNAL_PACKAGE_PROFILE=experimental \
		$(MAKE) --no-print-directory external-package-execution-matrix-check

.PHONY: value-lifecycle-contract-check
value-lifecycle-contract-check: validate-ebnf tree-sitter-cli-check editor-check
	$(RUST_TEST) -p terlan --lib value_lifecycle_test -- --nocapture
	$(RUST_TEST) -p terlan --lib const_eval_test -- --nocapture
	$(RUST_TEST) -p terlan --lib expression_macro -- --nocapture
	$(RUST_TEST) -p terlan --lib value_lifecycle_ -- --nocapture
	$(RUST_TEST) -p terlan --lib repl_rejects_local_constants_but_accepts_constant_imports -- --nocapture
	$(RUST_TEST) -p terlan --lib --features editor-lsp completion_items_include_local_and_imported_shapes_and_functions -- --nocapture
	$(RUST_TEST) -p terlan --lib --features editor-lsp value_lifecycle_ -- --nocapture
	$(TERLC) test tests/language/ValueLifecycleTest.terl --target terlan-vm
	$(TERLC) --target-profile js.shared test tests/language/ValueLifecycleTest.terl --target js

test: cli-test

rust-test-suite:
ifeq ($(TERLAN_RUST_SUITE_ALREADY_RUN),1)
	@echo "[rust-test-suite] canonical owned Rust suite already passed."
else ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
	@set -eu; \
	for binary in terlc terlan-vm terlan-native-worker terlan-test-orchestrator; do \
		test -x "target/debug/$$binary"; \
	done; \
	TERLAN_RUST_SUITE_REPORT=$(CURDIR)/target/quality/rust-test-suite-report.json \
		target/debug/terlan-test-orchestrator
	test -s target/quality/rust-test-suite-report.json
else
	$(CARGO) build --bin terlc --bin terlan-vm --bin terlan-native-worker --bin terlan-test-orchestrator
	TERLAN_RUST_SUITE_REPORT=$(CURDIR)/target/quality/rust-test-suite-report.json \
		target/debug/terlan-test-orchestrator
	test -s target/quality/rust-test-suite-report.json
endif

test-release: cli-test-release terlan-release-train-check $(TEST_RELEASE_STDLIB_TARGET)

.PHONY: dev-check dev-vm-check dev-web-check build
dev-check: rust-warnings-check std-test-honesty-check terlan-lint-style-profile-check cli-exact-selector-check

dev-vm-check: terlan-vm-run-command-check vm-diagnostics-quality-check vm-runtime-concept-inventory-check

dev-web-check: tree-sitter-cli-check editor-debugger-surface-check angular-ts-namespace-generation-check

build: cli-build

.PHONY: formal-cloud-deploy-plan-check formal-cloud-release-bundle-check formal-cloud-dashboard-toolchain-check
formal-cloud-deploy-plan-check: cli-build
	$(RUST_TEST) -p terlan --lib commands::deploy::deploy_test -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_parses_semantic_deployment_intent_without_values -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_rejects_secret_values_and_undeclared_secret_names -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_rejects_machine_local_deployment_paths_and_urls -- --nocapture
	bash scripts/check_cloud_deploy_plan_v2.sh

formal-cloud-release-bundle-check: cli-build
	$(RUST_TEST) -p terlan --lib release_bundle_is_complete_portable_and_deterministic -- --nocapture
	bash scripts/check_cloud_release_bundle_v1.sh

formal-cloud-dashboard-toolchain-check: cli-build
	$(RUST_TEST) -p terlan --lib parse_bind_angular_ts_args -- --nocapture
	$(RUST_TEST) -p terlan --lib commands::build::web_toolchain::tests -- --nocapture
	TERLC="$(if $(CARGO_TARGET_DIR),$(abspath $(CARGO_TARGET_DIR)),$(abspath target))/debug/terlc" bash scripts/check_managed_web_toolchain.sh

LOCAL_TERLC ?= $(shell command -v terlc 2>/dev/null || printf '%s/.local/bin/terlc' "$$HOME")
LOCAL_TERLAN_VM ?= $(dir $(LOCAL_TERLC))terlan-vm
LOCAL_TERLAN_NATIVE_WORKER ?= $(dir $(LOCAL_TERLC))terlan-native-worker

upgrade-local:
	$(CARGO) build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-native-worker
	@install_path="$(LOCAL_TERLC)"; \
	vm_install_path="$(LOCAL_TERLAN_VM)"; \
	worker_install_path="$(LOCAL_TERLAN_NATIVE_WORKER)"; \
	mkdir -p "$$(dirname "$$install_path")"; \
	mkdir -p "$$(dirname "$$vm_install_path")"; \
	mkdir -p "$$(dirname "$$worker_install_path")"; \
	install -m 0755 target/release/terlc "$$install_path"; \
	install -m 0755 target/release/terlan-vm "$$vm_install_path"; \
	install -m 0755 target/release/terlan-native-worker "$$worker_install_path"; \
	echo "installed local terlc to $$install_path"; \
	echo "installed local terlan-vm to $$vm_install_path"; \
	echo "installed local terlan-native-worker to $$worker_install_path"; \
	"$$install_path" --version; \
	"$$vm_install_path" --version; \
	"$$worker_install_path" --version

update-terlc: upgrade-local vscode-extension-check editor-extension-install-update-check
	@printf '%s\n' "updated local terlc at $(LOCAL_TERLC)"
	@printf '%s\n' "updated local terlan-vm at $(LOCAL_TERLAN_VM)"
	@printf '%s\n' "updated local terlan-native-worker at $(LOCAL_TERLAN_NATIVE_WORKER)"
	@printf '%s\n' "validated VS Code and Tree-sitter editor package update artifacts"
	@printf '%s\n' "reload VS Code so the Terlan extension uses the updated compiler"

validate-ebnf: | terlan-ebnf-validator-bootstrap
	TERLAN_EBNF_ROOT="$(CURDIR)" \
	TERLAN_EBNF_COMPILER="$(CURDIR)/$(TERLAN_BOOTSTRAP_COMPILER)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_EBNF_VALIDATOR_IMAGE) --script-eval -- --self-test
	TERLAN_EBNF_ROOT="$(CURDIR)" \
	TERLAN_EBNF_COMPILER="$(CURDIR)/$(TERLAN_BOOTSTRAP_COMPILER)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_EBNF_VALIDATOR_IMAGE) --script-eval -- --strict

workspace-version-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) workspace-version

release-version-metadata-check: | terlan-compiler-bootstrap
	TERLAN_RELEASE_VERSION="$(VERSION)" \
	TERLAN_RELEASE_CHANNEL=dev \
		target/debug/terlc test scripts/self_validation/ReleaseVersionChannelTest.terl

release-version-channel-check: release-version-metadata-check

release-version-bump: | terlan-compiler-bootstrap
	TERLAN_RELEASE_VERSION="$(VERSION)" \
	TERLAN_RELEASE_VERSION_WRITE=1 \
	TERLAN_RELEASE_CHANNEL=dev \
		target/debug/terlc test scripts/self_validation/ReleaseVersionChannelTest.terl

source-extension-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) source-extensions

.PHONY: repository-validation-self-test repository-build-release-contract-check release-artifact-set-check
repository-validation-self-test: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) self-test

repository-build-release-contract-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) build-release-contract
	@attempt=0; \
	until test -s target/quality/validation-build-plan-report.json; do \
		attempt=$$((attempt + 1)); \
		if test "$$attempt" -ge 50; then \
			echo "error: repository validator did not seal target/quality/validation-build-plan-report.json" >&2; \
			exit 1; \
		fi; \
		sleep 0.1; \
	done
	@rg -q '"decision": "pass"' target/quality/validation-build-plan-report.json
	@rg -q '"duplicate_equivalent_build_count": 0' target/quality/validation-build-plan-report.json
	@rg -q '"terlc_test_invocation_count":' target/quality/validation-build-plan-report.json
	@rg -q '"terlc_test_invocation_maximum": 32' target/quality/validation-build-plan-report.json
	@rg -q '"terlc_build_invocation_maximum": 16' target/quality/validation-build-plan-report.json
	@rg -q '"incremental_terlc_build_invocation_count":' target/quality/validation-build-plan-report.json
	@rg -q '"lifecycle_partial_check_count": 2' target/quality/validation-build-plan-report.json
	@rg -q '"cargo_invocation_maximum": 6' target/quality/validation-build-plan-report.json
	@rg -q '"typed_validator_request_maximum": 17' target/quality/validation-build-plan-report.json
	@rg -q '"typed_validator_parallelism_maximum": 2' target/quality/validation-build-plan-report.json

release-boundary-check: repository-build-release-contract-check
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) release-boundary

RELEASE_ARTIFACT_SET_ROOT ?= target/release-distribution
RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD ?= 0
release-artifact-set-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) release-artifact-set \
		"$(RELEASE_ARTIFACT_SET_ROOT)" \
		$(if $(filter 1,$(RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD)),--with-local-payload,)

single-root-contract-check:
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/SingleRootContractTest.terl

diff-whitespace-check:
	git diff --check

dormant-runtime-code-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools dormant_runtime_code_test
	$(TERLAN_QUALITY) dormant-runtime-code

vm-deterministic-hashmap-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_deterministic_hashmap_test
	$(TERLAN_QUALITY) vm-deterministic-hashmap

safe-rust-runtime-check:
	target/debug/terlc test scripts/self_validation/SafeRustRuntimeTest.terl

test-hierarchy-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools test_hierarchy_test
	$(TERLAN_QUALITY) test-hierarchy

dev-fast-feedback-profile-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools dev_fast_feedback_profile_test
	$(TERLAN_QUALITY) dev-fast-feedback-profile

std-source-naming-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_source_naming_test
	$(TERLAN_QUALITY) std-source-naming

std-generated-metadata-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_generated_metadata_test
	$(TERLAN_QUALITY) std-generated-metadata

cli-exact-selector-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools cli_exact_selectors_test
	$(TERLAN_QUALITY) cli-exact-selectors

core-typing-spec-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools core_typing_spec_test
	$(TERLAN_QUALITY) core-typing-spec

target-inference-contract-check:
	$(RUST_TEST) -p terlan --lib target_profile_inference
	$(RUST_TEST) -p terlan --lib accepts_asset_import_resolution_for_browser_target_profile
	$(RUST_TEST) -p terlan --lib build_command_infers_js_browser_target_from_asset_imports
	$(RUST_TEST) -p terlan --lib build_command_rejects_explicit_vm_target_for_js_evidence
	$(RUST_TEST) -p terlan --lib run_check_single_file_infers_js_shared_profile_from_js_import
	$(RUST_TEST) -p terlan --lib run_check_single_file_rejects_explicit_core_v0_profile_for_js_import
	$(RUST_TEST) -p terlan --lib run_check_dir_infers_js_shared_profile_from_js_import
	$(RUST_TEST) -p terlan --lib run_check_dir_rejects_explicit_core_v0_profile_for_js_import
	$(RUST_TEST) -p terlan --lib run_check_dir_rejects_map_for_core_v0_target_profile
	$(RUST_TEST) -p terlan --lib run_command_rejects_js_target_evidence_before_build
	$(RUST_TEST) -p terlan --lib run_command_rejects_explicit_js_profile_for_vm_source
	$(RUST_TEST) -p terlan --lib repl_seed_target_inference_rejects_js_source_evidence
	$(RUST_TEST) -p terlan --lib repl_seed_target_inference_rejects_explicit_js_profile_for_vm_source
	$(RUST_TEST) -p terlan --lib validate_web_package_rejects_non_browser_target_profile

.PHONY: target-inference-default-vm-check
target-inference-default-vm-check:
	$(EXACT_CARGO_TEST) -p terlan --lib target_profile_inference
	$(EXACT_CARGO_TEST) -p terlan --lib parse_build_args_defaults_to_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --lib parse_build_args_rejects_explicit_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_defaults_to_terlan_vm_artifact_without_erlang_or_beam
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_defaults_project_directory_to_terlan_vm_artifacts
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_infers_js_browser_target_from_asset_imports
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_rejects_explicit_vm_target_for_js_evidence
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_infers_wasm_core_target_from_i32_abi_import
	$(EXACT_CARGO_TEST) -p terlan --lib package_metadata_defaults_to_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --lib validate_run_args_defaults_to_vm_target
	$(EXACT_CARGO_TEST) -p terlan --lib validate_run_args_accepts_vm_and_rejects_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --lib build_command_for_run_appends_default_vm_target
	$(EXACT_CARGO_TEST) -p terlan --lib run_command_rejects_js_target_evidence_before_build
	$(EXACT_CARGO_TEST) -p terlan --lib run_command_rejects_explicit_js_profile_for_vm_source
	$(EXACT_CARGO_TEST) -p terlan --lib parse_test_args_accepts_default_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --lib parse_test_args_rejects_explicit_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --lib run_test_defaults_to_terlan_vm_execution
	$(EXACT_CARGO_TEST) -p terlan --lib repl_runtime_selects_effective_target_profile
	$(EXACT_CARGO_TEST) -p terlan --lib repl_seed_target_inference_rejects_js_source_evidence
	$(EXACT_CARGO_TEST) -p terlan --lib repl_seed_target_inference_rejects_explicit_js_profile_for_vm_source

shared-helper-check: rust-boundary-audit-report terlan-shared-helper-bootstrap
	TERLAN_SHARED_HELPER_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SHARED_HELPER_IMAGE) --script-eval -- --self-test
	@ulimit -v $(TERLAN_SHARED_HELPER_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_SHARED_HELPER_TIMEOUT_SECONDS)s env \
			TERLAN_SHARED_HELPER_ROOT="$(CURDIR)" \
			$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SHARED_HELPER_IMAGE) --script-eval

release-code-hygiene-check: \
	rust-warnings-check \
	rust-quality-check \
	rust-file-headroom-check \
	dormant-runtime-code-check \
	vm-deterministic-hashmap-check \
	shared-helper-check \
	terlan-lint-style-profile-check \
	terlan-lint-pipe-canonicalization-check
	$(TERLAN_QUALITY) release-code-hygiene


installer-contract-check: | terlan-compiler-bootstrap
	target/debug/terlc test scripts/self_validation/installer_contract

vm-release-install-validation-check:
	$(TERLAN_QUALITY) vm-release-install-validation

vm-release-artifact-matrix-check: \
	vm-release-install-validation-check \
	cli-release-artifact-current \
	release-artifact-installer-smoke
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-matrix



rust-build-feature-shipping-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib --features quality-tools rust_build_feature_shipping_test
	$(TERLAN_QUALITY) rust-build-feature-shipping

language-feature-coverage-100-check: comprehension-guards-check shape-implications-check
	$(RUST_TEST) -p terlan --lib --features quality-tools language_feature_full_coverage_test
	$(TERLAN_QUALITY) language-feature-coverage-100
	$(TERLC) test tests/language/LanguageFeatureCoverageTest.terl

operator-coverage-100-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools operator_full_coverage_test
	$(TERLAN_QUALITY) operator-coverage-100
	$(TERLC) test tests/operator/OperatorCoverageTest.terl
	$(TERLC) test tests/comparison/ComparisonTest.terl

pattern-matching-support-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools pattern_matching_support_test
	$(TERLAN_QUALITY) pattern-matching-support
	$(TERLC) test tests/pattern/PatternMatchingTest.terl

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

string-pattern-long-tail-check:
	$(TERLAN_QUALITY) pattern-matching-support
	@tmp_home="$$(mktemp -d)"; trap 'rm -rf "$$tmp_home"' EXIT; HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check && HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check:cli
	$(TERLC) test tests/pattern/StringPatternLongTailTest.terl

binary-bitstring-processing-check: binary-descriptor-check binary-syntax-scaffold-check binary-error-taxonomy-check binary-protocol-helper-check binary-protocol-benchmark-check vm-tcp-framing-check

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

binary-descriptor-check: binary-descriptor-contract-check binary-runtime-suite-check

binary-descriptor-contract-check:
	$(TERLAN_QUALITY) binary-descriptor-contract

binary-error-taxonomy-check: binary-runtime-suite-check

binary-protocol-helper-check: binary-runtime-suite-check
	target/debug/terlc test tests/binary/BinaryProtocolHelperTest.terl

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

core-type-contracts-check: tree-sitter-package-check tree-sitter-cli-check

type-alias-shorthand-check: tree-sitter-cli-check
	target/debug/terlc test std/core/AtomTest.terl

compiler-purity-metadata-check:
	$(TERLC) test \
		std/core/EffectTest.terl \
		tests/language/PurityEffectsTest.terl \
		tests/fixtures/purity_template/PurityTemplateTest.terl

lean-proof-track-check:
	$(MAKE) --no-print-directory --jobs=1 \
		lean-proof-feature-cull-check \
		lean-proof-semantic-kernels-check \
		lean-proof-track-runtime-check \
		proof-repro-check \
		lean-proof-smoke-check \
		lean-proof-track-pr-gate \
		lean-proof-track-regression-check
	target/debug/terlc test \
		scripts/self_validation/LeanProofLanesTest.terl \
		scripts/self_validation/LeanProofFeatureBindingTest.terl

lalrpop-parser-parity-check: tree-sitter-package-check tree-sitter-cli-check editor-check
	target/debug/terlc test scripts/self_validation/LalrpopGrammarContractTest.terl
	$(RUST_TEST) -p terlan --lib compiler::syntax:: -- --test-threads=1

lean-proof-parser-shape-check:
	target/debug/terlc test scripts/self_validation/LalrpopGrammarContractTest.terl
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=parser_shape/ParserShape.lean \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl

lean-proof-native-boundary-check: native-boundary-security-check
	target/debug/terlc test scripts/self_validation/LeanProofNativeBoundaryTest.terl

lean-proof-smoke-check: lean-proof-native-boundary-check
	target/debug/terlc test scripts/self_validation/LeanProofSmokeTest.terl

lean-proof-feature-binding-check: proof-repro-check
	target/debug/terlc test scripts/self_validation/LeanProofFeatureBindingTest.terl
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=diff \
		target/debug/terlc test scripts/self_validation/LeanProofSnapshotTest.terl

lean-proof-change-impact-report: lean-proof-feature-binding-check
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=impact \
		target/debug/terlc test scripts/self_validation/LeanProofSnapshotTest.terl \
			--name selected_snapshot_task_holds

lean-proof-feature-binding-review: lean-proof-change-impact-report
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=review \
		target/debug/terlc test scripts/self_validation/LeanProofSnapshotTest.terl \
			--name selected_snapshot_task_holds

lean-proof-snapshot-consistency-check: lean-proof-feature-binding-review
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=snapshot \
		target/debug/terlc test scripts/self_validation/LeanProofSnapshotTest.terl \
			--name selected_snapshot_task_holds

lean-proof-counterexample-check:
	target/debug/terlc test scripts/self_validation/LeanProofCounterexampleTest.terl

lean-proof-feature-cull-check:
	$(TERLAN_QUALITY) lean-proof-feature-cull
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=Terlan/FeatureCull/LegacyBoundaries.lean \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl

proof-repro-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_track
	$(TERLAN_QUALITY) lean-proof-track
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_TASK=proof-repro-report \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl \
			--name selected_lean_task_holds

proof_repro_check: proof-repro-check

lean-proof-track-pr-gate:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_pr_test
	$(TERLAN_QUALITY) lean-proof-pr
lean-proof-templates-routes-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family templates-routes

lean-proof-concurrency-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family concurrency

lean-proof-collections-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family collections

lean-proof-wasm-bridge-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family wasm-bridge

lean-proof-db-sql-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family database-sql

lean-proof-std-package-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family std-packages

lean-proof-semantic-kernels-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- self-test
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check

lean-proof-track-runtime-check: lean-proof-semantic-kernels-check
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_runtime_test
	$(TERLAN_QUALITY) lean-proof-runtime

lean-proof-track-regression-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_regression_test
	$(TERLAN_QUALITY) lean-proof-regression


# These two gates share build/artifacts/lean-proof-gate.json. Keep their order
# explicit: runtime validation establishes the runner contract, then the proof
# replay gate seals the family evidence consumed by the lane and release checks.
# Ordinary prerequisites are insufficient because parallel make may reorder them.
release-artifacts-closeout-check: | terlan-self-validation-bootstrap
	$(MAKE) TERLAN_VALIDATION_BOOTSTRAPPED=1 lean-proof-track-runtime-check
	$(MAKE) TERLAN_VALIDATION_BOOTSTRAPPED=1 proof-repro-check
	target/debug/terlc test scripts/self_validation/LeanProofLanesTest.terl
	TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- self-test
	TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- check

proof-coverage-release-artifacts-smoke: release-artifacts-closeout-check
	@TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- summary

proof-readiness-release-mode-check: release-artifacts-closeout-check
	TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- release-mode

lean-proof-track-gap-hygiene-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_gap_hygiene
	$(TERLAN_QUALITY) lean-proof-gap-hygiene

lean-proof-track-release-closeout-check: rust-test-suite lalrpop-parser-parity-check lean-proof-parser-shape-check lean-proof-native-boundary-check lean-proof-counterexample-check lean-proof-smoke-check lean-proof-snapshot-consistency-check $(LEAN_PROOF_CLOSEOUT_DEPS) lean-proof-track-gap-hygiene-check
	target/debug/terlan-lean-proof-closeout


RELEASE_EVIDENCE_GATES := \
	vm-http-runtime-attribution-check \
	release-failure-reproduction-check \
	internal-docs-check \
	release-generated-artifacts-check \
	release-version-channel-check \
	lean-proof-snapshot-consistency-check \
	lean-proof-track-release-closeout-check \
	release-artifacts-closeout-check \
	proof-readiness-release-mode-check \
	release-staged-distribution-verification-check

# Refresh is intentionally separate from preflight. The canonical `check`
# invocation owns the Rust suite, compiler, quality tools, and typed validator
# builds. Only after that boundary passes do the evidence owners run in one
# inherited Make graph, with duplicate Rust/build launchers disabled. The
# preflight below only composes and validates existing evidence, so a late
# failure can be repaired by rerunning the invalidated owner instead of
# replaying every successful gate.
release-evidence-compose:
	TERLAN_RUST_SUITE_ALREADY_RUN=1 \
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
		$(MAKE) --no-print-directory $(RELEASE_EVIDENCE_GATES)
	@echo "[release-evidence-compose] version $(RELEASE_VERSION) candidate-bound evidence composed"

release-evidence-refresh: check
	$(MAKE) --no-print-directory release-evidence-compose

release-preflight:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_RELEASE_PROMOTION_IMAGE)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) preflight-self-test
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) preflight --version "$(RELEASE_VERSION)"
	test -s target/quality/release-support-manifest.json
	test -s target/quality/release-preflight-report.json
	@rg -q '"decision": "pass"' target/quality/release-preflight-report.json
	@rg -q '"release_version": "$(RELEASE_VERSION)"' target/quality/release-preflight-report.json
	@rg -q '"outcome_report_count":' target/quality/release-preflight-report.json
	@rg -q '"candidate_build_count": 1' target/quality/release-preflight-report.json
	@rg -q '"candidate_seal_count": 1' target/quality/release-preflight-report.json
	@rg -q '"publication_required": false' target/quality/release-preflight-report.json
	@rg -q '"evidence_strategy": "candidate-bound-composition-v1"' target/quality/release-preflight-report.json
	@rg -q '"preflight_replays_completed_gates": false' target/quality/release-preflight-report.json
	@rg -q '"same_process_required": false' target/quality/release-preflight-report.json
	@rg -q '"selective_invalidation_required": true' target/quality/release-preflight-report.json
	@echo "[release-preflight] version $(RELEASE_VERSION) candidate-bound semantic evidence passed"

release-check: release-evidence-refresh
	$(MAKE) --no-print-directory release-preflight RELEASE_VERSION="$(RELEASE_VERSION)"

release-candidate-check: build-artifact-budget-record
	TERLAN_BUILD_ARTIFACTS_PREBUILT=0 \
		$(MAKE) --no-print-directory terlan-quality-tools-bootstrap
	bash scripts/clean_build_outputs.sh --check-partials
	TERLAN_ARTIFACT_MEASUREMENT_ALREADY_BUILT=1 \
	TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		$(MAKE) --no-print-directory check
	bash scripts/clean_build_outputs.sh --check-partials

function-head-migration-diagnostic-policy-check:
	$(TERLAN_QUALITY) function-head-migration-diagnostic-policy

function-head-migration-lint-check: function-head-migration-diagnostic-policy-check
	$(TERLAN_QUALITY) function-head-migration-lint

function-head-pattern-migration-assist-check: function-head-migration-lint-check
	$(TERLAN_QUALITY) function-head-pattern-migration-assist

function-head-pattern-migration-benchmark-check: function-head-pattern-migration-assist-check
	$(TERLAN_QUALITY) function-head-pattern-migration-benchmark

function-head-pattern-migration-docs-check: function-head-migration-diagnostic-policy-check function-head-pattern-parameters-check
	$(TERLAN_QUALITY) function-head-pattern-migration-docs

function-head-pattern-0-0-7-handoff-check: function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-parameters-check function-head-pattern-migration-docs-check
	$(TERLAN_QUALITY) function-head-pattern-handoff

function-head-pattern-parameters-hardening-check: function-head-pattern-0-0-7-handoff-check

function-head-pattern-parameters-check:
	$(TERLAN_QUALITY) function-head-observability
	grep -F 'pub add({left, right}: {Int, Int}): Int ->' docs/grammar/README.md
	grep -F 'pub describe({status, body}: Dynamic): String.' docs/grammar/README.md
	grep -F 'pub full_name({name, family_name} = user: User): String ->' docs/grammar/README.md

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

syntax-contract-check: validate-ebnf tree-sitter-package-check tree-sitter-cli-check
	grep -F 'ImplicationConstraint ::= "=>" StructuralEvidenceShape .' docs/grammar/TERLAN_SYNTAX_SPEC.ebnf
	grep -F 'The implication arrow is accepted only as generic-parameter shorthand' docs/grammar/README.md
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_structural_implication_in_function_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_structural_generic_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_implication_arrow_as_runtime_expression_operator -- --exact

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

typed-template-interpolation-check: shape-implications-check tree-sitter-package-check tree-sitter-cli-check

wasm-coreir-lowering-check:

wasm-runtime-exec-check:

wasm-contract-discovery-check:

oxc-boundary-check:
	$(TERLAN_QUALITY) oxc-boundary

adversarial-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib adversarial -- --nocapture
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-adversarial

coverage-check:
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
		echo "coverage-check requires cargo-llvm-cov; install with: cargo install cargo-llvm-cov --locked"; \
		exit 127; \
	}
	$(CARGO) build --quiet -p terlan --bin terlan-vm
	TERLAN_VM_RUNNER=$(COVERAGE_VM_RUNNER) $(CARGO) llvm-cov --quiet -p terlan --bin terlc --ignore-filename-regex '$(COVERAGE_IGNORE_FILENAME_REGEX)' --fail-under-lines $(COVERAGE_MIN)

release-hardening-check: adversarial-check coverage-check

erlang-backend-classification-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools erlang_backend_classification_test
	$(TERLAN_QUALITY) erlang-backend-classification

terlan-vm-artifact-format-check:
	$(RUST_TEST) -p terlan --lib native_image_test
	$(TERLAN_QUALITY) vm-artifact-format

tvm-native-image-format-check: terlan-vm-artifact-format-check

tvm-direct-aot-backend-check: terlan-vm-artifact-format-check
	$(RUST_TEST) -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact

tvm-aot-lowering-coverage-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::lowering_coverage_test

tvm-aot-application-closure-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::application_admission_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::static_callable_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::higher_order_specialization_test

tvm-aot-case-lowering-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::case_lowering_test

tvm-aot-managed-field-projection-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::field_projection_test

tvm-aot-owned-closure-representation-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::managed_closure_test

tvm-aot-closure-dispatch-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::managed_closure_dispatch_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::closure_conversion_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::cranelift::managed_callback_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::vm_artifact::native_descriptor_test::lifted_callable_descriptor_separates_captures_from_call_arguments -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::managed_execution_test::executable_metadata_installs_generation_scoped_closure_dispatch -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::descriptor_rejects_invalid_abi_ids_and_boundary_types -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::deterministic_artifact_test::deterministic_module_emits_reproducible_vm_artifact_bytes -- --exact

tvm-aot-typed-mailbox-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::core_intrinsic_process_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::trait_negative_test::syntax_output_rejects_actor_message_with_denied_delivery -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::trait_negative_test::syntax_output_accepts_actor_and_node_values_with_default_delivery -- --exact
	$(RUST_TEST) -p terlan --lib compiler::native_ir::cranelift::managed_type_test

tvm-aot-typed-lifecycle-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_keeps_entry_operation_and_erases_value_descriptors -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_accepts_explicit_typed_process_lifecycle_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_rejects_scalar_process_lifecycle_arguments -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::cranelift::managed_type_test::typed_public_lifecycle_operations_lower_to_existing_vm_transitions -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::lowering_coverage_test::intrinsic_families_have_explicit_lowering_dispositions -- --exact
	$(RUST_TEST) -p terlan --lib vm::main_test::native_transition_test

tvm-aot-managed-continuation-check:
	$(RUST_TEST) -p terlan --test managed_continuation_aot

tvm-aot-thread-neutral-continuation-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::thread_neutral::thread_neutral_test::parked_native_continuation_is_send_sync_and_static -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::pure_native_transport_test::parked_native_continuation_resumes_after_thread_transfer -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test::direct_backend_parked_state_is_send_sync_and_static -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_suspension_test

tvm-aot-multicore-readiness-check: tvm-aot-thread-neutral-continuation-check
	$(RUST_TEST) -p terlan --lib runtime::vm::pure_native::multicore_model_test
	$(RUST_TEST) -p terlan --lib runtime::vm::pure_native
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::actor_continuations_interleave_reentrantly_on_one_shard -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::empty_shard_forks_execute_concurrently_without_shared_state -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test::execution_runtime_interleaves_owner_scoped_continuations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::memory::memory_test::limits_and_accounting::memory_accounted_mailbox_send_receive_and_pressure_are_atomic -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_test::actor_message_wakeup_is_deduplicated_and_missing_target_is_side_effect_free -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::managed_test::actor_local_collection_budget_cannot_pause_or_mutate_another_heap -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact
	@rg -q 'struct PureNativeExecutionContext' crates/terlan/src/runtime/vm/pure_native.rs
	@rg -q 'struct PureNativeExecutionRuntime' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'continuations: BTreeMap<u64, PendingNativeContinuation>' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'struct NativeContinuationClaim' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'claim_continuation' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'struct VmMailboxPublication' crates/terlan/src/runtime/vm/memory/publication.rs
	@rg -q 'accepted accounted actor send must produce a publication receipt' crates/terlan/src/runtime/vm/actor.rs
	@rg -q 'execution: PureNativeExecutionRuntime' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'context: &mut PureNativeExecutionContext' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'execution_context_rejects_foreign_actor_before_transition_service' crates/terlan/src/runtime/vm/pure_native_transport_test.rs
	@rg -q 'image: Arc<PureNativeExecutionImage>' crates/terlan/src/commands/serve/handler_cache.rs
	@if sed -n '/pub(crate) struct DirectNativeBackend {/,/^}/p' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs | rg -n 'pending|continuation|Mutex|RwLock'; then \
		echo 'error[aot.reentrant]: admitted direct backend must retain immutable image code only'; \
		exit 1; \
	fi

	@if sed -n '/pub(crate) struct PureNativeBoundary {/,/^}/p' crates/terlan/src/runtime/vm/pure_native.rs | rg -n 'next_request|pending|continuation|Mutex|RwLock'; then \
		echo 'error[aot.reentrant]: admitted boundary must not retain mutable actor execution state'; \
		exit 1; \
	fi
	@if rg -n 'Mutex<PureNativeExecution(Image|Shard)>|\.(image|shard)[[:space:]]*\.lock\(' crates/terlan/src/commands/serve/handler_cache.rs; then \
		echo 'error[aot.reentrant]: HTTP execution must not serialize on an image-wide shard mutex'; \
		exit 1; \
	fi
	@if rg -n 'Mutex|RwLock|thread_local!|Arc<Mutex<ManagedExecutionRuntime|lock_managed|managed_lock' \
		crates/terlan/src/runtime/vm/pure_native.rs \
		crates/terlan/src/runtime/vm/pure_native/direct_backend.rs \
		crates/terlan/src/runtime/vm/pure_native/execution.rs \
		crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs \
		crates/terlan/src/runtime/vm/pure_native/execution_shard.rs \
		crates/terlan/src/runtime/native_image/managed/heap.rs; then \
		echo 'error[aot.multicore]: direct actor execution must not use process-global locks or thread-local runtime state'; \
		exit 1; \
	fi

tvm-aot-thread-sanitizer-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) tsan-self-test
	@if rustup toolchain list | grep -Eq '^nightly-2026-07-16-'; then \
		$(TERLAN_TVM_PLATFORM_MATRIX) tsan-run; \
	elif test "$${GITHUB_ACTIONS:-}" = true; then \
		echo 'error[aot.tsan]: pinned nightly ThreadSanitizer toolchain is mandatory in CI'; \
		exit 1; \
	else \
		echo 'TVM AOT ThreadSanitizer executable lane unavailable locally; contract passed'; \
	fi

tvm-aot-thread-sanitizer-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) tsan-self-test

AOT_RELEASE_LOCAL_GATES := \
	runtime-aot-only-check \
	tvm-direct-aot-backend-check \
	tvm-managed-memory-check \
	tvm-managed-list-profile-benchmark-check \
	terlan-vm-artifact-format-check \
	tvm-native-image-format-check \
	tvm-native-image-loader-check \
	tvm-aot-consumer-check \
	tvm-aot-package-install-consumer-check \
	tvm-aot-runtime-transition-check \
	tvm-aot-shard-ownership-check \
	tvm-aot-supervisor-lifecycle-check \
	tvm-aot-stale-epoch-check \
	tvm-aot-crash-injection-check \
	tvm-aot-capability-worker-check \
	tvm-aot-image-lifetime-check \
	tvm-aot-platform-matrix-check \
	tvm-aot-lowering-coverage-check \
	tvm-aot-http-persistent-shard-check \
	tvm-aot-http-generation-lifetime-check \
	tvm-aot-http-performance-check \
	tvm-aot-multicore-readiness-check \
	tvm-aot-c-abi-boundary-check \
	tvm-aot-compilation-time-check \
	tvm-single-image-artifact-check \
	no-tvm-json-runtime-check \
	no-vmir-interpreter-check \
	rust-quality-check

AOT_RELEASE_MULTICORE_REUSED_GATES := \
	tvm-aot-runtime-transition-check \
	tvm-managed-memory-check \
	rust-quality-check

ifeq ($(TERLAN_RUST_SUITE_ALREADY_RUN),1)
AOT_RELEASE_CARGO_CHECK := true
else
AOT_RELEASE_CARGO_CHECK := env -u RUSTFLAGS $(CARGO) check -p terlan
endif

ifeq ($(TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN),1)
AOT_RELEASE_LOCAL_GATES_TO_RUN := $(filter-out $(AOT_RELEASE_MULTICORE_REUSED_GATES),$(AOT_RELEASE_LOCAL_GATES))
else
AOT_RELEASE_LOCAL_GATES_TO_RUN := $(AOT_RELEASE_LOCAL_GATES)
endif

tvm-aot-release-closeout-contract-check: tvm-aot-thread-sanitizer-contract-check tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) release-self-test

tvm-aot-release-closeout-check: tvm-aot-release-closeout-contract-check
	@if test "$(TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN)" = 1; then \
		test -s target/quality/vm-multicore-release-closeout.json; \
		rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json; \
		revision=$$(git rev-parse HEAD); \
		rg -q "\"source_revision\": \"$$revision\"" target/quality/vm-multicore-release-closeout.json; \
	fi
	$(MAKE) $(AOT_RELEASE_LOCAL_GATES_TO_RUN)
	$(AOT_RELEASE_CARGO_CHECK)
	$(TERLAN_TVM_PLATFORM_MATRIX) release-record

tvm-aot-publish-evidence-check:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	$(TERLAN_TVM_PLATFORM_MATRIX) release-verify

tvm-aot-static-callable-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::static_callable_test

tvm-aot-higher-order-specialization-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::higher_order_specialization_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::static_callable_test::source_captured_lambda_lowers_into_native_application -- --exact

tvm-aot-application-conformance-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::aot3_conformance_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::expression::free_variable_analysis_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::closure_conversion_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::generic_specialization_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::cast_lowering_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::collection_values_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::structured_case_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::try_lowering_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::capability_transition_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::constructor_lowering_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::template_values_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::http_values_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::lowering_coverage_test

tvm-managed-memory-check:
	$(RUST_TEST) -p terlan --lib managed_test
	$(RUST_TEST) -p terlan --lib managed_atom_test
	$(RUST_TEST) -p terlan --lib managed_sequence_test
	$(RUST_TEST) -p terlan --lib managed_aggregate_test
	$(RUST_TEST) -p terlan --lib managed_list_test
	$(RUST_TEST) -p terlan --lib managed_map_test
	$(RUST_TEST) -p terlan --lib managed_set_test
	$(RUST_TEST) -p terlan --lib collection_abi_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::collections::collections_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::atom_inventory_test
	$(RUST_TEST) -p terlan --lib managed_execution_test
	$(RUST_TEST) -p terlan --lib direct_backend_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::constructor_lowering_test
	$(RUST_TEST) -p terlan --lib managed_callback
	$(RUST_TEST) -p terlan --lib compiler::native_ir::escape_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::scalar_replacement_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_mailbox_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::cranelift::managed_type_test::typed_public_mailbox_operations_lower_to_fixed_native_transition_frames -- --exact
	$(RUST_TEST) -p terlan --lib typed_native_
	$(RUST_TEST) -p terlan --lib pure_native_transport

tvm-aot-capability-worker-check: tvm-aot-stale-epoch-check | terlan-native-worker-bootstrap
	$(RUST_TEST) -p terlan --lib main_test
	$(RUST_TEST) -p terlan --lib sandbox
	$(RUST_TEST) -p terlan --lib capability_wire
	$(RUST_TEST) -p terlan --lib protocol::protocol_test
	$(RUST_TEST) -p terlan --lib capability_worker
	TERLAN_TEST_CAPABILITY_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_process_transport_runs_full_cycle -- --ignored --exact
	TERLAN_TEST_CAPABILITY_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_sandbox_closes_inherited_descriptor -- --ignored --exact
	@rg -q '#!\[deny\(unsafe_code\)\]' crates/terlan/src/native_worker/main.rs
	@rg -q 'NativeBoundaryWorker::new' crates/terlan/src/native_worker/protocol/execution.rs
	@rg -q 'call_for_process_with_policy_and_cancellation' crates/terlan/src/native_worker/protocol/execution.rs
	@rg -q 'NativeBoundaryCancellationToken' crates/terlan/src/runtime/native_boundary/cancellation.rs crates/terlan/src/native_worker/protocol/execution.rs
	@rg -q 'CapabilitySandboxProfile::current' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'NativeBoundaryExecutionProfile' crates/terlan/src/runtime/native_boundary/metadata.rs crates/terlan/src/runtime/vm/capability_worker.rs crates/terlan/src/native_worker/protocol.rs
	@rg -q '\.arg\("--execution-profile"\)' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q '"--execution-profile"' crates/terlan/src/native_worker/protocol.rs
	@rg -q 'platform_admission_has_no_weak_fallback' crates/terlan/src/runtime/native_boundary/capability_sandbox_test.rs
	@rg -q '\.env_clear\(\)' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'VmNativeBoundaryDeadlineQueue' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'VmCapabilityRequestContext' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'VmCapabilityWorkerIdentity' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'VmShardEpochOperation' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -Fq 'sync_channel(queue_limit)' crates/terlan/src/runtime/vm/capability_worker.rs
	@rg -q 'worker_rejects_capability_operation_identity_mismatch' crates/terlan/src/native_worker/protocol_test.rs
	@rg -q 'capability_worker_restart_generation_attributes_reused_request_ids' crates/terlan/src/runtime/vm/capability_worker_test.rs
	@rg -q 'capability_worker_rejects_undeclared_capability_before_parking' crates/terlan/src/runtime/vm/capability_worker_test.rs
	@rg -q 'sandbox_file_descriptor_attestation_rejects_inherited_resources' crates/terlan/src/native_worker/sandbox_test.rs
	@rg -q 'CLOSE_INHERITED_DESCRIPTORS' crates/terlan/src/runtime/vm/capability_worker/sandbox.rs
	@if rg -n 'mpsc::channel\(\)' crates/terlan/src/runtime/vm/capability_worker.rs; then \
		echo 'error[aot.capability_worker]: capability-worker queues must remain bounded'; \
		exit 1; \
	fi
	@rg -q 'CapabilityRequest::Cancel' crates/terlan/src/runtime/vm/capability_worker.rs crates/terlan/src/native_worker/protocol/execution.rs
	@rg -q 'BUBBLEWRAP_PATH' crates/terlan/src/runtime/vm/capability_worker/sandbox.rs
	@rg -q 'PRLIMIT_PATH' crates/terlan/src/runtime/vm/capability_worker/sandbox.rs
	@rg -q 'verify_capability_worker_sandbox' crates/terlan/src/native_worker/main.rs crates/terlan/src/native_worker/sandbox.rs
	@if rg -n 'libloading|inspect_tvm_image|TVM_DISPATCH_SYMBOL|ManagedExecutionRuntime|PendingWorkerContinuation|TvmControlFrame' crates/terlan/src/native_worker; then \
		echo 'error[aot.capability_worker]: capability worker must not load Terlan images, dispatch application code, or own Terlan heaps and continuations'; \
		exit 1; \
	fi
	@if rg -n 'VmCapabilityWorker|TERLAN_NATIVE_WORKER|terlan-native-worker' crates/terlan/src/vm/main/native_image_runner.rs crates/terlan/src/commands/test/vm_runner.rs crates/terlan/src/commands/vm.rs crates/terlan/src/commands/repl/evaluation.rs crates/terlan/src/commands/serve/handler_cache.rs; then \
		echo 'error[aot.capability_worker]: ordinary actor execution must not reference capability-worker transport'; \
		exit 1; \
	fi

.PHONY: tvm-managed-list-profile-benchmark-check
tvm-managed-list-profile-benchmark-check:
	TERLAN_MANAGED_LIST_PROFILE_OUTPUT=$(CURDIR)/target/quality/tvm-managed-list-profile.json $(EXACT_CARGO_TEST) --locked --release -p terlan --lib runtime::native_image::managed::lists::managed_list_profile_benchmark_test::managed_list_profiles_emit_stable_benchmark_report -- --exact --nocapture
	test -s target/quality/tvm-managed-list-profile.json

.PHONY: tvm-aot-runtime-workload-benchmark-check
tvm-aot-runtime-workload-benchmark-check: | terlan-benchmark-release-bootstrap
	TERLAN_BENCH_AOT_RUNTIME_OUTPUT=$(CURDIR)/target/quality/vm-aot-runtime-workloads.json target/release/terlan-benchmark vm-aot-runtime-workloads
	test -s target/quality/vm-aot-runtime-workloads.json
	@rg -q '"status": "completed"' target/quality/vm-aot-runtime-workloads.json
	@rg -q '"p99_ns":' target/quality/vm-aot-runtime-workloads.json
	TERLAN_JSON_EVIDENCE_PATH=target/quality/vm-aot-runtime-workloads.json \
	TERLAN_JSON_EVIDENCE_REQUIRED='"name": "actor_heap_allocation";;"name": "local_message_round_trip";;"name": "scheduler_yield_cycle";;"name": "actor_local_collection_pause";;"name": "actor_spawn_exit_churn";;"name": "mixed_actor_runtime_tail"' \
		target/debug/terlc test scripts/self_validation/JsonEvidenceContractTest.terl \
			--name selected_json_evidence_holds

tvm-native-image-loader-check: tvm-direct-aot-backend-check

tvm-aot-consumer-check: tvm-native-image-loader-check
	$(RUST_TEST) -p terlan --test tvm_transition_rejection

tvm-aot-test-consumer-check: tvm-aot-consumer-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_executes_bool_test -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_fails_false_bool_test -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_writes_runtime_manifests -- --exact
	@rg -q 'compile_test_native_image' crates/terlan/src/commands/test/execution.rs
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/test/vm_runner.rs
	@if rg -n 'struct TerlanVm|runtime::vm::TerlanVm|evaluate_[A-Za-z0-9_]*|apply_closure|\.tvm\.json' \
		crates/terlan/src/commands/test --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.test_consumer]: test execution must use one admitted native image without evaluator or serialized-runtime fallback'; \
		exit 1; \
	fi

tvm-aot-repl-consumer-check: tvm-aot-consumer-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::scalar_repl_generation_executes_without_resident_core_ir -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::float_repl_generation_executes_without_resident_core_ir -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::managed_repl_generation_executes_without_resident_core_ir -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::unchanged_repl_generation_reuses_active_native_shard -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact
	@rg -q 'compile_repl_native_image' crates/terlan/src/commands/repl/evaluation.rs
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/repl/evaluation.rs
	@rg -q 'active\.shard\.replace_image\(&native_image\)' crates/terlan/src/commands/repl/evaluation.rs
	@if rg -n 'enum ReplRuntime|enum ActiveReplRuntime|--runtime|struct TerlanVm|runtime::vm::TerlanVm|evaluate_repl_function|apply_closure|\.tvm\.json|PureNativeWorker|Evaluator|evaluator' \
		crates/terlan/src/commands/repl --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.repl_consumer]: REPL execution must own one persistent admitted native shard without runtime selection or evaluator fallback'; \
		exit 1; \
	fi
	@if rg -n 'evaluator bridge|CoreIR evaluator|Native worker' crates/terlan/src/commands/repl/README.md; then \
		echo 'error[aot.repl_consumer]: REPL documentation must describe the native-only consumer contract'; \
		exit 1; \
	fi

tvm-aot-debugger-consumer-check: tvm-aot-consumer-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_renamed_json_target -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_stale_source_map -- --exact
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/debug/session.rs
	@rg -q 'inspect_tvm_native_debug' crates/terlan/src/commands/debug/session.rs
	@if rg -n 'struct TerlanVm|runtime::vm::TerlanVm|evaluate_[A-Za-z0-9_]*|apply_closure|\.tvm\.json|PureNativeWorker|Evaluator|evaluator' \
		crates/terlan/src/commands/debug --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.debugger_consumer]: debugger admission must use a native image without evaluator or serialized-runtime fallback'; \
		exit 1; \
	fi

tvm-aot-hot-reload-consumer-check: tvm-aot-consumer-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test::drain_timeout_quarantines_and_retains_reachable_image -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::draining_generation_closes_entries_and_preserves_accepted_continuations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::native_image_diagnostics::generation_lifetime_test::generation_reference_snapshot_proves_quiescence_and_orders_diagnostics -- --exact
	@rg -q 'compile_reload_native_image' crates/terlan/src/commands/vm/native_reload.rs
	@rg -q 'publish_native_generation' crates/terlan/src/commands/vm/native_reload.rs
	@rg -q 'replace_image_before_deadline' crates/terlan/src/runtime/vm/source_reload.rs
	@rg -q 'quarantine_drain_timeout' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@if rg -n 'publish_changed_files_with_report' crates/terlan/src/commands/vm/native_reload.rs; then \
		echo 'error[aot.hot_reload_consumer]: native hot reload must not publish code-server-only generations'; \
		exit 1; \
	fi

tvm-aot-package-install-consumer-check: tvm-aot-consumer-check
	$(TERLAN_TVM_PACKAGE_CONSUMER) self-test
	$(TERLAN_TVM_PACKAGE_CONSUMER) check
	$(TERLAN_QUALITY) vm-release-install-validation

tvm-aot-support-crash-metadata-check: tvm-aot-package-install-consumer-check
	$(RUST_TEST) -p terlan --lib native_image_diagnostics
	$(RUST_TEST) -p terlan --lib fatal_diagnostics
	$(RUST_TEST) -p terlan --lib support_bundle_replay_metadata_binds_native_generation_once
	$(RUST_TEST) -p terlan --lib drain_timeout_quarantines_and_retains_reachable_image
	$(RUST_TEST) -p terlan --lib pure_native::execution_shard::execution_shard_test
	@rg -q 'support-bundle <file.tvm>' crates/terlan/src/vm/execution.rs
	@if rg -n 'CoreExpr|CoreFunction|TvmExecutableDescriptor|executable_bytes|source_path' \
		crates/terlan/src/runtime/vm/native_image_diagnostics.rs \
		crates/terlan/src/runtime/vm/support_bundle.rs \
		crates/terlan/src/runtime/vm/fatal_diagnostics.rs; then \
		echo 'error[aot.support_bundle]: native diagnostic metadata must not retain executable CoreIR, code bytes, or host source paths'; \
		exit 1; \
	fi

vm-ignore-cores-parity-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::fatal_diagnostics::fatal_diagnostics_ignore_cores_parity_test::ignore_cores_helper_is_replaced_by_explicit_non_mutating_artifact_publication -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::fatal_diagnostics
	@! rg -q 'ignore_core_files|/cores|set_current_dir|set_var.*PWD' crates/terlan/src/runtime/vm/fatal_diagnostics.rs

vm-iovec-suite-parity-check:
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::iovec::iovec_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::driver::driver_beam_suite_parity_test
	@rg -q 'let mut pending = vec!\[value\]' crates/terlan/src/runtime/vm/iovec.rs
	@rg -q 'Arc::clone\(bytes\)' crates/terlan/src/runtime/vm/iovec.rs
	@rg -q '0\.\.8_192' crates/terlan/src/runtime/vm/iovec_beam_suite_parity_test.rs
	@rg -q 'write_vectored' crates/terlan/src/runtime/vm/protocol_task_executor/transport.rs crates/terlan/src/commands/serve/hyper_server.rs

vm-lcnt-suite-parity-check:
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler::contention::contention_beam_suite_parity_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_fairness_telemetry_is_deterministic_under_cpu_bound_load -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::process::process_registry_test::process_registry_lists_resolves_and_unregisters_names_deterministically -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::table::table_test::table_store_creates_owner_table_and_exposes_snapshot -- --exact
	@rg -q 'enabled_mask: u8' crates/terlan/src/runtime/vm/scheduler/contention.rs
	@rg -q 'VM_CONTENTION_RECORD_CAPACITY: usize = 4_096' crates/terlan/src/runtime/vm/scheduler/contention.rs
	@rg -q '0\.\.1_000' crates/terlan/src/runtime/vm/scheduler/contention_beam_suite_parity_test.rs
	@rg -q 'observe_scheduler_wait' crates/terlan/src/runtime/vm/scheduler.rs crates/terlan/src/runtime/vm/scheduler/contention.rs
	@! rg -q 'Mutex|RwLock|Instant|SystemTime' crates/terlan/src/runtime/vm/scheduler/contention.rs

vm-list-bif-suite-parity-check:
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::list_bif_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::integer::test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::diagnostic_test::arguments_and_constructors::syntax_output_list_cons_expr_rejects_non_list_tail_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::lists::managed_list_test::rrb_lookup_handles_leaf_and_multilevel_boundaries -- --exact
	@rg -q 'MAX_INTEGER_PARSE_BYTES: usize = 128' crates/terlan/src/runtime/native_image/managed/operation_abi/integer.rs
	@rg -q 'managed_expression_layouts' crates/terlan/src/compiler/native_ir/application.rs crates/terlan/src/compiler/native_ir/aggregate_types.rs
	@rg -q 'option_constructor_plan' crates/terlan/src/compiler/native_ir/structured_case.rs
	@rg -q 'assert_managed_native_object_invocations' crates/terlan/src/compiler/native_ir/list_bif_suite_native_parity_test.rs

vm-literal-area-collector-parity-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::literal_area_collector_helper_maps_to_synchronous_generation_quiescence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::literal_area_collector_helper_observes_actor_drain_without_polling -- --exact

vm-lttng-suite-parity-check: vm-iovec-suite-parity-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::driver::lttng_beam_suite_parity_test
	@rg -q 'VM_DRIVER_TRACE_CAPACITY: usize = 4_096' crates/terlan/src/runtime/vm/driver/trace.rs
	@rg -q 'enabled_mask: u8' crates/terlan/src/runtime/vm/driver/trace.rs
	@rg -q 'expired; oldest retained sequence' crates/terlan/src/runtime/vm/driver/trace.rs
	@! rg -q 'org_erlang_otp|erl_driver|std::process|Command::new' crates/terlan/src/runtime/vm/driver.rs crates/terlan/src/runtime/vm/driver/trace.rs

vm-map-suite-parity-check:
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::map_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::maps::managed_map_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::structured_case_test
	@rg -q 'assert_managed_native_object_invocations' crates/terlan/src/compiler/native_ir/map_suite_native_parity_test.rs
	@rg -q 'MAP_FROM_ENTRY_LIST' crates/terlan/src/runtime/native_image/managed/operation_abi/collections.rs
	@rg -q 'MAP_ITERATOR' crates/terlan/src/runtime/native_image/managed/operation_abi/collections.rs
	@rg -q 'map_intrinsics::lower' crates/terlan/src/compiler/native_ir/expression/intrinsics.rs
	@! rg -q 'CoreIR|interpreter|evaluator' crates/terlan/src/compiler/native_ir/map_suite_native_parity_test.rs

vm-match-spec-suite-parity-check: vm-map-suite-parity-check vm-guard-suite-parity-check vm-table-primitives-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::match_spec_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_call_trace_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_trace_meta_beam_suite_parity_test
	@rg -q 'assert_managed_native_object_invocations' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@rg -q 'aot_comprehension_' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@rg -q 'dynamic match-spec call must fail before native linking' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@! rg -q 'match_spec|match-spec' crates/terlan/src/runtime --glob '*.rs'

vm-module-info-suite-parity-check: vm-code-server-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::module_info_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::code_server::code_server_inspection_test
	@rg -q 'defined_functions: BTreeMap' crates/terlan/src/runtime/vm/code_server.rs
	@rg -q 'pub\(crate\) fn active_module_info' crates/terlan/src/runtime/vm/code_server.rs
	@rg -q 'with_defined_functions' crates/terlan/src/runtime/vm/code_server_compiler.rs
	@rg -q 'function.public' crates/terlan/src/compiler/native_ir/module_info_suite_native_parity_test.rs
	@rg -q 'all\(\|\(name, _, _, _\)\| \*name != "module_info"' crates/terlan/src/compiler/native_ir/module_info_suite_native_parity_test.rs
	@rg -q 'assert_native_object_invocations' crates/terlan/src/compiler/native_ir/module_info_suite_native_parity_test.rs

vm-mtx-suite-parity-check: vm-multicore-memory-model-check vm-table-primitives-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_mtx_beam_suite_parity_test
	@rg -q 'const PRODUCERS: usize = 20' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'const WRITERS: usize = 6' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'ACTOR_MAILBOX_CAPACITY' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@! rg -q 'Mutex|RwLock|sleep|Instant|SystemTime|erl_nif|ethr_' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs

vm-multi-load-suite-parity-check: vm-code-server-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::multi_load_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::code_server::multi_load_beam_suite_parity_test
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::application_admission_test::duplicate_module_identity_is_rejected -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_parallel_load_beam_suite_parity_test::code_parallel_load_suite_simultaneous_identical_publish_happens_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_duplicate_modules_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::native_reload_rejects_duplicate_modules_before_image_admission -- --exact
	@rg -q 'const MODULES: usize = 100' crates/terlan/src/compiler/native_ir/multi_load_suite_native_parity_test.rs
	@rg -q 'assert_native_object_invocations' crates/terlan/src/compiler/native_ir/multi_load_suite_native_parity_test.rs
	@rg -q 'validate_staged_batch' crates/terlan/src/runtime/vm/source_reload.rs
	@! rg -q 'on_load|finish_loading|prepare_loading|CoreIR|interpreter|evaluator' crates/terlan/src/compiler/native_ir/multi_load_suite_native_parity_test.rs crates/terlan/src/runtime/vm/multi_load_beam_suite_parity_test.rs

vm-native-record-suite-parity-check: vm-gc-suite-parity-check vm-distribution-envelope-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::native_record_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::field_projection_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::aggregates::managed_aggregate_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::mailbox_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_native_record_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::term_format::term_format_runtime_test::native_record_suite
	@rg -q 'const MESSAGES: i64 = 1_000' crates/terlan/src/runtime/vm/actor/actor_native_record_beam_suite_parity_test.rs
	@rg -q '1\.\.=64' crates/terlan/src/compiler/native_ir/native_record_suite_native_parity_test.rs
	@rg -q 'remaining: Int.*Pair' crates/terlan/src/compiler/native_ir/native_record_suite_native_parity_test.rs
	@rg -q 'validate_text_field\("record_name"' crates/terlan/src/runtime/vm/term_format.rs crates/terlan/src/runtime/vm/term_format/decoder.rs
	@! rg -q 'records::|term_to_binary|binary_to_term|RecordExt|atom cache|CoreIR|interpreter|evaluator' crates/terlan/src/compiler/native_ir/native_record_suite_native_parity_test.rs crates/terlan/src/runtime/vm/actor/actor_native_record_beam_suite_parity_test.rs crates/terlan/src/runtime/vm/term_format_runtime_test.rs

vm-nif-suite-parity-check: tvm-aot-capability-worker-check vm-resource-ownership-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::worker::nif_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::term::term_test
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::dispatch::panic_boundary::panic_boundary_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_native_failure_test::native_failure_uses_vm_exit_propagation_monitoring_and_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::draining_generation_closes_entries_and_preserves_accepted_continuations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::orderly_shard_shutdown_records_one_generation_qualified_lifecycle -- --exact
	@rg -q 'nif_suite_portable_calls_keep_values_resources_and_failures_vm_owned' crates/terlan/src/runtime/native_boundary/nif_suite_parity_test.rs
	@rg -q 'nif_suite_request_lifecycle_recovers_backpressure_without_id_reuse' crates/terlan/src/runtime/native_boundary/nif_suite_parity_test.rs
	@rg -q 'CapabilityRequest::Shutdown' crates/terlan/src/native_worker/protocol/execution.rs
	@rg -q 'CapabilityResponse::ShutdownAck' crates/terlan/src/native_worker/protocol/execution.rs
	@if rg -n 'erl_nif|load_nif|rustler::|rustler_|VM/NIF|thread, NIF' crates/terlan/src/runtime/native_boundary crates/terlan/src/runtime/native/vector.rs --glob '*.rs' --glob '!**/*test*.rs'; then \
		echo 'error[vm.nif_suite]: the active native boundary must not present an Erlang NIF compatibility surface'; \
		exit 1; \
	fi

tvm-aot-image-lifetime-check: tvm-aot-package-install-consumer-check
	$(RUST_TEST) -p terlan --lib runtime::native_image::sealed
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::admitted_native_generation_survives_source_image_replacement -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::replacement_rejects_duplicate_image_generation_before_drain -- --exact
	@rg -q 'Library::new\(sealed\.path\(\)\)' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'sealed\.verify_unchanged\(\)' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'loaded image does not match admitted package bytes' crates/terlan/src/runtime/native_image/package_validation.rs
	@if rg -n 'tvm\.reuse' crates/terlan/src/commands/build/vm_artifact \
		--glob '*.rs' --glob '!output_cleanup.rs' --glob '!output_cleanup_test.rs'; then \
		echo 'error[aot.image_lifetime]: compiler cache metadata must not be published beside native images'; \
		exit 1; \
	fi
	@rg -q 'ends_with\("\.tvm\.reuse"\)' crates/terlan/src/commands/build/vm_artifact/output_cleanup.rs
	@if rg -n 'fs::write|File::create|OpenOptions|write_all|fs::rename' crates/terlan/src/commands/build/vm_artifact/output_cleanup.rs; then \
		echo 'error[aot.image_lifetime]: retired-sidecar cleanup must remain delete-only'; \
		exit 1; \
	fi
	@if rg -n 'Library::new\(path\)|fs::read\(path\)' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs; then \
		echo 'error[aot.image_lifetime]: direct admission must not reopen the caller-controlled image path'; \
		exit 1; \
	fi

tvm-aot-c-abi-boundary-check:
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::adapter_abi::adapter_abi_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::native_inspection_accepts_real_elf_and_rejects_wrong_target_and_abi -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::fixtures_and_generation::structured_c_metadata_generates_real_ffi_package -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::cpp_binding_generator::generator::generator_test::fixtures_and_generation::structured_cpp_metadata_generates_real_cxx_package -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::ownership_adapters::generated_c_adapter_compiles_and_enforces_public_protocol -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::cpp_binding_generator::generator::generator_test::fixtures_and_generation::generated_cxx_adapter_compiles_and_enforces_public_protocol -- --exact
	@rg -q 'PUBLIC_ADAPTER_ABI_VERSION' crates/terlan/src/runtime/native_image/image.rs crates/terlan/src/commands/build/vm_artifact/native_descriptor.rs
	@rg -q 'cache_identity\(&target\.triple, &target\.calling_convention\)' crates/terlan/src/commands/build/vm_artifact/native_image.rs
	@if rg -ni 'TvmRef|actor.?heap|Cranelift|continuation|native.?stack|shard|thread.?identity' \
		crates/terlan/src/runtime/native_boundary/adapter_abi.rs \
		crates/terlan/src/commands/bind/c_abi_binding_generator.rs \
		crates/terlan/src/commands/bind/cpp_binding_generator --glob '*.rs'; then \
		echo 'error[aot.public_adapter]: public adapter metadata must not expose the private runtime ABI'; \
		exit 1; \
	fi

.PHONY: tvm-aot-platform-matrix-contract-check tvm-aot-platform-target-check tvm-aot-platform-aggregate-check tvm-aot-platform-matrix-check
tvm-aot-platform-matrix-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) self-test

tvm-aot-platform-target-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) target

tvm-aot-platform-aggregate-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) aggregate $(TERLAN_TVM_PLATFORM_REPORT_ROOT)

tvm-aot-platform-matrix-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) target

tvm-aot-http-managed-cycle-check: tvm-aot-consumer-check
	$(RUST_TEST) -p terlan --lib literal_abi_test
	$(RUST_TEST) -p terlan --lib http_values_test
	$(RUST_TEST) -p terlan --lib response_bridge_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-request-accessor-check: tvm-aot-http-managed-cycle-check
	$(RUST_TEST) -p terlan --lib operation_abi_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::request_accessors_lower_to_checked_managed_operations -- --exact

tvm-aot-http-response-mutation-check: tvm-aot-http-request-accessor-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::response_updates_are_persistent_and_preserve_repeated_headers -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::response_status_headers_and_raw_cookies_lower_to_persistent_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::response_bridge::response_bridge_test::native_repeated_headers_are_validated_and_preserved -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-typed-metadata-check: tvm-aot-http-response-mutation-check
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::http::http_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::typed_cookie_jar_and_security_calls_rewrite_to_managed_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::typed_security_policy_rejects_unknown_marker -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::response_bridge::response_bridge_test::native_security_headers_are_not_claimed_by_transport_framing -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-router-callable-check: tvm-aot-http-typed-metadata-check
	$(RUST_TEST) -p terlan --lib compiler::router::router_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::case_lowering_test::string_case_patterns_lower_to_managed_value_equality -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_equal_operation_is_value_based_and_checked -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_append_operation_concatenates_validated_values -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact

tvm-aot-http-managed-error-check: tvm-aot-http-router-callable-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::aggregate_scalar_projection_returns_an_unboxed_native_word -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::typed_http_error_constructor_and_accessors_lower_to_managed_values -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::typed_http_error_operations_reject_invalid_arities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact

tvm-aot-http-template-check: tvm-aot-http-managed-error-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::template_values_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_list_join
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_managed_template_html_handler -- --exact

tvm-aot-http-template-render-plan-check: tvm-aot-http-template-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_carries_validated_template_render_plans_into_core_ir -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::html_escape_operations_preserve_context_and_validate_arity -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_external_template_render_plan -- --exact

tvm-aot-http-template-expression-check: tvm-aot-http-template-render-plan-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::template_values_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::typed_template_rendering_enforces_attributes_options_and_urls -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_complete_typed_template_matrix -- --exact

tvm-aot-http-body-json-check: tvm-aot-http-template-expression-check
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::json::json_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::body_json_result_case_lowers_to_typed_managed_branches -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_decodes_managed_json_body_result -- --exact

tvm-aot-http-session-check: tvm-aot-http-body-json-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::session_calls_lower_to_vm_owned_managed_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::session_import_installs_complete_managed_boundary_metadata -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_adapter_functions_delegate_to_actor_runtime -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_session_state_and_lifecycle_are_vm_owned -- --exact

tvm-aot-http-managed-boundary-check: tvm-aot-http-session-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::complete_http_managed_boundary_inventory_is_closed_and_decodable -- --exact

tvm-aot-http-channel-plan-check: tvm-aot-http-managed-boundary-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::router::router_test::aot_router_plan_materializes_canonical_channel_targets -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::request_option_case_lowers_without_scalar_constructor_patterns -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::upgrades_and_acme::vm_stream_websocket_upgrade_activates_materialized_router_middleware -- --exact

tvm-aot-http-persistent-shard-check: tvm-aot-http-channel-plan-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::one_owner_loop_services_multiple_parked_actors_without_migration -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::new_actors_balance_across_shards_and_resume_sticky -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_timer_resumes_after_its_owner_deadline -- --exact
	@! rg -q 'spawn_shard\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'shard\.shutdown\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'Mutex<PureNativeExecutionShard' crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache/
	@rg -q 'sync_channel\(SHARD_INBOX_CAPACITY\)' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs

tvm-http-axum-performance-record: terlan-http-benchmark-release-bootstrap
	TERLAN_BENCH_HTTP_AXUM_BIN=$(CURDIR)/target/release/terlan-axum-baseline \
	TERLAN_BENCH_HTTP_AXUM_OUTPUT=$(CURDIR)/target/quality/http-axum-performance.json \
	$(CURDIR)/target/release/terlan-http-framework-benchmark
	test -s target/quality/http-axum-performance.json
	@rg -q '"schema": "terlan-http-framework-performance-v2"' target/quality/http-axum-performance.json
	@rg -q '"memory_snapshots":' target/quality/http-axum-performance.json
	@rg -q '"rounds":' target/quality/http-axum-performance.json

HTTP_PAIRED_OUTPUT ?= $(CURDIR)/target/quality/http-paired-performance.json

tvm-http-paired-performance-check: terlan-benchmark-release-bootstrap terlan-serve-runtime-bootstrap terlan-http-benchmark-release-bootstrap
	$(CURDIR)/target/release/terlan-http-paired-benchmark --self-test
	TERLAN_COMPILER=$(CURDIR)/target/debug/terlc \
	TERLAN_BENCH_HTTP_AOT_BENCHMARK_BIN=$(CURDIR)/target/release/terlan-benchmark \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(TERLAN_SERVE_RUNTIME_BIN) \
	TERLAN_BENCH_HTTP_AXUM_BENCHMARK_BIN=$(CURDIR)/target/release/terlan-http-framework-benchmark \
	TERLAN_BENCH_HTTP_AXUM_BIN=$(CURDIR)/target/release/terlan-axum-baseline \
	TERLAN_BENCH_HTTP_HYPER_BENCHMARK_BIN=$(CURDIR)/target/release/terlan-http-framework-benchmark \
	TERLAN_BENCH_HTTP_HYPER_BIN=$(CURDIR)/target/release/terlan-hyper-baseline \
	TERLAN_BENCH_HTTP_PAIRS=$${TERLAN_BENCH_HTTP_PAIRS:-3} \
	TERLAN_BENCH_HTTP_DURATION_MS=$${TERLAN_BENCH_HTTP_DURATION_MS:-2000} \
	TERLAN_BENCH_HTTP_SOAK_SECONDS=$${TERLAN_BENCH_HTTP_SOAK_SECONDS:-10} \
	TERLAN_BENCH_HTTP_WRK_SECONDS=$${TERLAN_BENCH_HTTP_WRK_SECONDS:-1} \
	TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS=$${TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS:-1} \
	TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS=$${TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS:-0} \
	TERLAN_BENCH_HTTP_PERF_SECONDS=$${TERLAN_BENCH_HTTP_PERF_SECONDS:-1} \
	TERLAN_BENCH_HTTP_PAIRED_OUTPUT=$(HTTP_PAIRED_OUTPUT) \
	$(CURDIR)/target/release/terlan-http-paired-benchmark
	@test -s $(HTTP_PAIRED_OUTPUT)
	@rg -q '"schema": "terlan-http-paired-performance-v1"' $(HTTP_PAIRED_OUTPUT)
	@rg -q '"rotating_order": true' $(HTTP_PAIRED_OUTPUT)
	@rg -q '"hyper_comparisons":' $(HTTP_PAIRED_OUTPUT)
	@rg -q '"confidence_method": "deterministic-bootstrap-of-paired-medians"' $(HTTP_PAIRED_OUTPUT)
	@rg -q '"ratio_95_percent_interval":' $(HTTP_PAIRED_OUTPUT)
	@rg -q '"isolation":' $(HTTP_PAIRED_OUTPUT)

tvm-http-decisive-performance-check:
	@test -n "$$TERLAN_BENCH_HTTP_CPU_LIST"
	@test -n "$$TERLAN_BENCH_HTTP_CLIENT_CPU_LIST"
	TERLAN_BENCH_HTTP_DECISIVE=1 \
	TERLAN_BENCH_HTTP_PAIRS=$${TERLAN_BENCH_HTTP_PAIRS:-10} \
	TERLAN_BENCH_HTTP_MIN_ACCEPTED_PAIRS=$${TERLAN_BENCH_HTTP_MIN_ACCEPTED_PAIRS:-7} \
	TERLAN_BENCH_HTTP_DURATION_MS=$${TERLAN_BENCH_HTTP_DURATION_MS:-10000} \
	TERLAN_BENCH_HTTP_WRK_SECONDS=$${TERLAN_BENCH_HTTP_WRK_SECONDS:-10} \
	TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS=$${TERLAN_BENCH_HTTP_WRK_MATRIX_SECONDS:-10} \
	TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS=$${TERLAN_BENCH_HTTP_OPEN_LOOP_SECONDS:-10} \
	$(MAKE) tvm-http-paired-performance-check
	@rg -q '"mode": "decisive"' target/quality/http-paired-performance.json
	@rg -q '"status": "accepted"' target/quality/http-paired-performance.json

tvm-aot-http-native-invocation-check: tvm-aot-http-persistent-shard-check

tvm-aot-http-websocket-invocation-check: tvm-aot-http-native-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::router::router_test::aot_router_plan_materializes_websocket_callbacks -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::websocket_invocation::websocket_invocation_test::websocket_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact

tvm-aot-http-sse-invocation-check: tvm-aot-http-websocket-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::router::router_test::aot_router_plan_materializes_sse_callbacks -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::sse_invocation::sse_invocation_test::sse_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact

tvm-aot-http-generation-lifetime-check: tvm-aot-http-sse-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::hot_reload_pins_in_flight_generation_until_its_last_lease_drops -- --exact

tvm-aot-http-channel-transport-check: tvm-aot-http-generation-lifetime-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::channel_transport::channel_transport_test::production_channel_pumps_preserve_vm_lifecycle_and_pressure_contracts -- --exact

tvm-aot-http-cleanup-check: tvm-aot-http-channel-transport-check
	$(RUST_TEST) -p terlan --lib callbacks_share_native_invocation_entry_resume_and_cancellation
	$(RUST_TEST) -p terlan --lib request_resources_track_peaks_and_release_every_transient_class
	$(RUST_TEST) -p terlan --lib request_resources_reject_duplicate_stale_and_unknown_completion
	$(RUST_TEST) -p terlan --lib vm_accounted_websocket_queue_cancellation_releases_pending_frames
	$(RUST_TEST) -p terlan --lib vm_accounted_sse_stream_cancellation_releases_all_pending_buffers
	$(RUST_TEST) -p terlan --lib http_session_expiration_cleans_actor_table_and_reports_stale
	$(RUST_TEST) -p terlan --lib timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order
	$(RUST_TEST) -p terlan --lib resource_table_cleans_up_owner_resources_on_process_exit
	$(RUST_TEST) -p terlan --lib vm_http_tcp_server_shutdown_closes_listener_and_active_handlers
	$(RUST_TEST) -p terlan --lib native_boundary_deadline_timeout_wakes_actor_and_rejects_late_completion

tvm-aot-http-lifecycle-inventory-check: tvm-aot-http-cleanup-check runtime-aot-only-check rust-quality-check | terlan-benchmark-release-bootstrap
	target/release/terlan-benchmark http-aot-performance-self-test
	@rg -q '^## AOT-5F Lifecycle Inventory$$' crates/terlan/src/commands/serve/handler/README.md
	@rg -q '^\| Generation replacement and unload ' crates/terlan/src/commands/serve/handler/README.md
	@rg -q '^\| Request and channel cleanup ' crates/terlan/src/commands/serve/handler/README.md
	@rg -q '^\| Bounded channel pressure and drain ' crates/terlan/src/commands/serve/handler/README.md
	@rg -q '^\| Runtime fallback deletion ' crates/terlan/src/commands/serve/handler/README.md
	@rg -q '^\| Same-machine performance comparison .* Complete ' crates/terlan/src/commands/serve/handler/README.md
	@if test -e ../benchmarks/results/http-checked-coreir-performance.json || \
		test -e target/quality/http-native-aot-performance.json || \
		test -e target/quality/http-aot-performance-comparison.json; then \
		test -s ../benchmarks/results/http-checked-coreir-performance.json && \
		test -s target/quality/http-native-aot-performance.json && \
		test -s target/quality/http-aot-performance-comparison.json && \
		TERLAN_BENCH_HTTP_CHECKED_COREIR_REPORT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
		TERLAN_BENCH_HTTP_NATIVE_AOT_REPORT=$(CURDIR)/target/quality/http-native-aot-performance.json \
		TERLAN_BENCH_HTTP_AOT_COMPARISON_OUTPUT=$(CURDIR)/target/quality/http-aot-performance-comparison.json \
		TERLAN_BENCH_HTTP_AOT_POLICY=$(CURDIR)/benchmarks/baselines/http-aot-performance-limits.json \
		target/release/terlan-benchmark http-aot-performance-compare || \
		(echo 'error[aot.http.lifecycle_inventory]: performance evidence must be wholly absent or complete, comparable, and within policy' && exit 1); \
	fi

tvm-aot-http-checked-coreir-reference-record: | terlan-benchmark-release-bootstrap
	@test -n "$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN)" || (echo 'error[aot.http.performance]: set TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN to the preserved checked-CoreIR terlc binary' && exit 1)
	target/release/terlan-benchmark http-aot-performance-self-test
	TERLAN_BENCH_HTTP_AOT_LANE=checked-coreir \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN) \
	TERLAN_BENCH_HTTP_AOT_OUTPUT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
	target/release/terlan-benchmark http-aot-performance
	test -s ../benchmarks/results/http-checked-coreir-performance.json

tvm-aot-http-performance-check: tvm-aot-http-cleanup-check | terlan-benchmark-release-bootstrap terlan-serve-runtime-bootstrap
	target/release/terlan-benchmark http-aot-performance-self-test
	test -s ../benchmarks/results/http-checked-coreir-performance.json
ifneq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	$(CARGO) build --release -p terlan --bin terlc
endif
	@! nm -C --defined-only $(TERLAN_SERVE_RUNTIME_BIN) | rg -q 'cranelift_codegen|cranelift_native|regalloc2|terlan_libpq|docker_compose_types|start_project_dependencies|parse_project_manifest'
	@! ldd $(TERLAN_SERVE_RUNTIME_BIN) | rg -q 'libpq|libssl|libcrypto|libldap|libgnutls|libsasl'
	@test "$$(size $(TERLAN_SERVE_RUNTIME_BIN) | awk 'NR == 2 { print $$1 }')" -lt 7250000
	TERLAN_COMPILER=$(CURDIR)/target/release/terlc \
	TERLAN_BENCH_HTTP_AOT_LANE=native-aot \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(TERLAN_SERVE_RUNTIME_BIN) \
	TERLAN_BENCH_HTTP_AOT_OUTPUT=$(CURDIR)/target/quality/http-native-aot-performance.json \
	TERLAN_BENCH_HTTP_AOT_MAX_RSS_BYTES=33554432 \
	TERLAN_BENCH_HTTP_MAX_ROUND_SPREAD_PERCENT=50 \
	TERLAN_BENCH_HTTP_DURATION_MS=1000 \
	target/release/terlan-benchmark http-aot-performance
	TERLAN_BENCH_HTTP_CHECKED_COREIR_REPORT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
	TERLAN_BENCH_HTTP_NATIVE_AOT_REPORT=$(CURDIR)/target/quality/http-native-aot-performance.json \
	TERLAN_BENCH_HTTP_AOT_COMPARISON_OUTPUT=$(CURDIR)/target/quality/http-aot-performance-comparison.json \
	TERLAN_BENCH_HTTP_AOT_POLICY=$(CURDIR)/benchmarks/baselines/http-aot-performance-limits.json \
	target/release/terlan-benchmark http-aot-performance-compare
	test -s target/quality/http-aot-performance-comparison.json
	@rg -q '"status": "completed"' target/quality/http-aot-performance-comparison.json
	@rg -q '"performance_policy_sha256":' target/quality/http-aot-performance-comparison.json
	@rg -q '"schema": "terlan-http-aot-performance-limits-v1"' target/quality/http-aot-performance-comparison.json
	@rg -q '"schema": "terlan-http-aot-performance-comparison-v2"' target/quality/http-aot-performance-comparison.json
	@rg -q '"p99_ns":' target/quality/http-aot-performance-comparison.json
	@rg -q '"throughput_requests_per_second":' target/quality/http-aot-performance-comparison.json
	@rg -q '"generation_overlap":' target/quality/http-aot-performance-comparison.json
	@rg -q '"sequentialP50Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"sequentialP95Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"pressureP50Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"pressureP95Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"longevityP50Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"longevityP95Percent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"longevityThroughputPercent":' target/quality/http-aot-performance-comparison.json
	@rg -q '"peakResidentMemoryPercent":' target/quality/http-aot-performance-comparison.json

tvm-single-image-artifact-check: tvm-native-image-format-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::vm_artifact::native_cache_test::native_cache_rejects_poisoned_keys_target_drift_and_incomplete_publications -- --exact
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::native_reuse::native_reuse_test
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_package package_build_emits_one_tvm_image_with_qualified_module_exports -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_cache native_aot_cache_verifies_and_recovers_every_required_file -- --exact

tvm-aot-compilation-benchmark-check:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	test -x target/release/terlc
	test -x target/release/terlan-vm
	test -x target/release/terlan-benchmark
else
	$(CARGO) build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark --features benchmark-tools
endif
	$(CURDIR)/target/release/terlan-benchmark aot-compilation-self-test
	@mkdir -p target/quality
	TERLAN_BENCH_AOT_COMPILATION_OUTPUT=$(CURDIR)/target/quality/aot-compilation-benchmark.json \
		$(CURDIR)/target/release/terlan-benchmark aot-compilation
	TERLAN_BENCH_AOT_COMPILATION_REPORT=$(CURDIR)/target/quality/aot-compilation-benchmark.json \
		TERLAN_BENCH_AOT_COMPILATION_POLICY=$(CURDIR)/benchmarks/baselines/aot-compilation-limits.json \
		$(CURDIR)/target/release/terlan-benchmark aot-compilation-validate
	@test -s target/quality/aot-compilation-benchmark.json
	@rg -q '"schema": "terlan-aot-compilation-benchmark-v1"' target/quality/aot-compilation-benchmark.json
	@rg -q '"status": "completed"' target/quality/aot-compilation-benchmark.json
	@rg -q '"sample_count": 7' target/quality/aot-compilation-benchmark.json
	TERLAN_JSON_EVIDENCE_PATH=target/quality/aot-compilation-benchmark.json \
	TERLAN_JSON_EVIDENCE_REQUIRED='"name": "small_cold_development";;"name": "multi_cold_development";;"name": "one_package_edit";;"name": "no_op_development";;"name": "cold_release";;"name": "package_relink";;"name": "repl_startup";;"name": "first_repl";;"name": "changed_repl";;"name": "unchanged_repl"' \
		target/debug/terlc test scripts/self_validation/JsonEvidenceContractTest.terl \
			--name selected_json_evidence_holds

tvm-aot-compilation-time-check: tvm-single-image-artifact-check tvm-aot-compilation-benchmark-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::specialization_budget_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::codegen_policy_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::args_test::parse_build_args_selects_explicit_native_release_policy -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::args_test::build_command_rejects_release_policy_for_non_vm_target -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_cache native_codegen_policies_publish_and_reuse_distinct_cache_entries -- --exact
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::parallel_compile_test
	$(RUST_TEST) -p terlan --lib commands::build::source_roots_test
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::checked_cache_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::parallel_compilation_test::parallel_frontend_compilation_preserves_one_application_link -- --exact
	$(EXACT_CARGO_TEST) --locked --release -p terlan --test direct_aot_cache vm_aot_timings_report_compile_and_native_artifact_phases -- --exact
	$(EXACT_CARGO_TEST) --locked --release -p terlan --test direct_aot_cache vm_aot_warm_noop_p95_stays_under_one_second -- --exact
	$(EXACT_CARGO_TEST) --locked --release -p terlan --test direct_aot_cache unchanged_repl_generation_reuses_native_image_without_relinking -- --exact
	$(EXACT_CARGO_TEST) --locked --release -p terlan --lib commands::repl::repl_aot_test::native_repl_unchanged_generation_p95_stays_under_one_second -- --exact --ignored
	$(EXACT_CARGO_TEST) --locked --release -p terlan --lib commands::repl::repl_aot_test::native_repl_changed_generation_p95_stays_under_one_second -- --exact --ignored

tail-recursion-lowering-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::tail_position_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::tail_position_source_test::source_case_tail_recursion_executes_one_million_edges_on_a_small_stack -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::cranelift::managed_stack_map_test::managed_tail_parameter_live_across_cranelift_safepoint_emits_precise_stack_map -- --exact
	$(RUST_TEST) -p terlan --lib commands::emit_js::tail_recursion_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::hot_reload_pins_in_flight_generation_until_its_last_lease_drops -- --exact

termination-productivity-analysis-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::core_ir::termination::termination_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::value_lifecycle::value_lifecycle_test::recursive_const_functions_require_core_termination_evidence -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_exposes_validated_termination_evidence -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::tail_position_source_test::source_case_tail_recursion_executes_one_million_edges_on_a_small_stack -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler::yielding_c_fun_beam_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_parallel_messages_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_process_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::supervision::supervision_test

binding-shadowing-safety-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::binding_identity_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_created_by_shape_expansion -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::binding_identity_source_test::native_lowering_preserves_outer_and_nested_binding_identities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::emit_js::binding_identity_emit_test::javascript_preserves_outer_and_nested_binding_identities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_exposes_validated_binding_identity_evidence -- --exact
	$(RUST_TEST) -p terlan --features editor-lsp --lib binding_navigation_test

no-tvm-json-runtime-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_tvm_json_artifact_rejections

no-vmir-interpreter-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_vmir_interpreter_rejections

runtime-aot-only-check:
	target/debug/terlc test scripts/self_validation/ReleaseTransitionScanTest.terl
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::args_test::parse_build_args_rejects_runtime_fallback_selection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::vm_artifact::output_cleanup_test::native_output_cleanup_removes_json_and_reuse_sidecars -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::run::run_test::validate_run_args_rejects_runtime_fallback_selection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::test::test_command_test::configuration_cases::parse_test_args_rejects_runtime_fallback_selection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::arguments_and_fixtures::parse_serve_args_rejects_explicit_beam_handler_runtime -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::handler_cache_compilation_removes_legacy_runtime_sidecars -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_renamed_json_target -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_stale_source_map -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::debug_args_reject_runtime_fallback_selection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::vm_native_reload_ignores_renamed_json_and_cleans_legacy_sidecars -- --exact
	$(RUST_TEST) -p terlan --test tvm_transition_rejection

.PHONY: tvm-aot-shard-ownership-check
tvm-aot-shard-ownership-check: tvm-direct-aot-backend-check
	$(RUST_TEST) -p terlan --lib runtime::vm::execution_shard_protocol::execution_shard_protocol_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_shard_service_isolation_test::actor_runtime_services_and_image_generations_are_shard_local -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::code_server::code_parallel_load_beam_suite_parity_test::code_parallel_load_suite_shard_local_workers_switch_without_global_lock -- --exact
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/vm/main/native_image_runner.rs
	@rg -q 'code_server: VmCodeServer' crates/terlan/src/runtime/vm/actor.rs
	@rg -q 'release_process_bindings' crates/terlan/src/runtime/vm/actor_exit.rs crates/terlan/src/runtime/vm/code_server.rs
	@if rg -n 'Mutex|RwLock|OnceLock|LazyLock|Arc<Mutex' \
		crates/terlan/src/runtime/vm/actor.rs \
		crates/terlan/src/runtime/vm/actor_code.rs \
		crates/terlan/src/runtime/vm/timer.rs \
		crates/terlan/src/runtime/vm/resource.rs \
		crates/terlan/src/runtime/vm/failure.rs \
		crates/terlan/src/runtime/vm/dynamic_module.rs; then \
		echo 'error[aot.shard_ownership]: ordinary actor services must not use process-global locks'; \
		exit 1; \
	fi
	@if awk '/impl VmConcurrentCodeServer \{/{inside=1} /impl VmCodeServer \{/{inside=0} inside' crates/terlan/src/runtime/vm/code_server.rs | rg -n 'bind_process|switch_process|release_process|enter_process|return_process'; then \
		echo 'error[aot.shard_ownership]: administrative code registry must not expose actor transitions'; \
		exit 1; \
	fi
	@if rg -n 'TERLAN_NATIVE_WORKER|PureNativeBoundary::load_image\(path\)' crates/terlan/src/vm/main/native_image_runner.rs; then \
		echo 'error[aot.shard_ownership]: standalone application execution must not use the adapter-worker path'; \
		exit 1; \
	fi
	@rg -q 'let \(backend, managed\) = DirectNativeBackend::load\(path\)' crates/terlan/src/runtime/vm/pure_native.rs
	@rg -q 'ManagedExecutionRuntime' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'park_continuation_captures' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'restore_continuation_captures' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@if rg -n 'TERLAN_NATIVE_WORKER|PureNativeWorker|Command::new' crates/terlan/src/runtime/vm/pure_native.rs crates/terlan/src/runtime/vm/pure_native/execution.rs; then \
		echo 'error[aot.shard_ownership]: application AOT boundaries must not contain worker-process transport'; \
		exit 1; \
	fi
	@rg -q 'VmShardControlClass::ALL' crates/terlan/src/runtime/vm/execution_shard_protocol_test.rs
	@if rg -n 'ReplValue|PureNativeSuspension|TvmTransitionOperation|VmCapabilityWorker|NativeShardDispatchEvent|^[[:space:]]*(Entry|Resume|Send|Receive|Yield|Timer|Link|Monitor|Cancellation|Failure|Scheduling|Resource)([[:space:]]*\{|[[:space:]]*\(|[[:space:]]*,)' crates/terlan/src/runtime/vm/execution_shard_protocol.rs; then \
		echo 'error[aot.shard_ownership]: supervisor control protocol must not encode local actor operations or capability RPC'; \
		exit 1; \
	fi
	@rg -q 'external-capability-worker.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'execution-shard-application-boundary.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md

.PHONY: tvm-aot-supervisor-lifecycle-check
tvm-aot-supervisor-lifecycle-check: tvm-aot-shard-ownership-check
	$(RUST_TEST) -p terlan --lib runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_admission_and_shutdown_follow_supervisor_lifecycle -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_replacement_drains_and_publishes_the_next_epoch -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_crash_recovery_rejects_early_restart_and_stale_execution -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::repl::repl_aot_test::float_repl_generation_executes_without_resident_core_ir -- --exact
	@rg -q 'VmRestartBackoffSchedule' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@rg -Fq 'matches!(self.phase, VmShardPhase::Ready)' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@rg -q 'supervisor: VmExecutionShardSupervisor' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'active\.shard\.replace_image\(&native_image\)' crates/terlan/src/commands/repl/evaluation.rs
	@if rg -n '^crates/terlan/src/runtime/vm/execution_shard_supervisor\.rs[[:space:]]' docs/runtime/DORMANT_RUNTIME_CODE.tsv; then \
		echo 'error[aot.supervisor_lifecycle]: active shard supervisor must not remain in dormant-runtime inventory'; \
		exit 1; \
	fi
	@if rg -n 'Local|Entry|Resume|Send|Receive|Yield|Timer|Link|Monitor|Resource' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs; then \
		echo 'error[aot.supervisor_lifecycle]: shard lifecycle must not encode actor-local operations'; \
		exit 1; \
	fi

.PHONY: tvm-aot-stale-epoch-check
tvm-aot-stale-epoch-check: tvm-aot-supervisor-lifecycle-check
	$(RUST_TEST) -p terlan --lib runtime::vm::execution_shard_epoch::execution_shard_epoch_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test::supervisor_rejects_stale_operations_and_suppresses_uncertain_recovery -- --exact
	@rg -q 'VmShardOperationKind::ALL' crates/terlan/src/runtime/vm/execution_shard_epoch_test.rs
	@rg -q 'pub\(crate\) replay_policy: VmShardReplayPolicy' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@rg -q 'IndeterminateSuppressed' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@rg -q 'DuplicateSuppressed' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@if rg -n 'epoch:[[:space:]]*u64' crates/terlan/src/runtime/vm/execution_shard_epoch.rs; then \
		echo 'error[aot.stale_epoch]: shard operations must use the canonical typed epoch'; \
		exit 1; \
	fi

tvm-aot-crash-injection-check: tvm-aot-stale-epoch-check
	$(RUST_TEST) -p terlan --lib runtime::vm::execution_shard_supervisor::execution_shard_fault_injection_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::actor_exit_releases_native_continuation_ownership -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_send_transition_delivers_before_exact_owner_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_reply_completes_live_vm_deadline -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_cancellation_wins_over_late_reply -- --exact
	@rg -q 'const ALL: \[Self; 16\]' crates/terlan/src/runtime/vm/execution_shard_fault_injection_test.rs
	@rg -q 'pub\(crate\) shard_id: VmExecutionShardId' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@if rg -n 'CrashBoundary' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs crates/terlan/src/runtime/vm/execution_shard_epoch.rs crates/terlan/src/runtime/vm/execution_shard_protocol.rs; then \
		echo 'error[aot.crash_injection]: crash injection must remain test-only'; \
		exit 1; \
	fi

tvm-aot-runtime-transition-check: tvm-aot-runtime-transition-focused-check

tvm-aot-runtime-transition-focused-check: tvm-native-image-format-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::local_actor_entry_and_resume_never_use_worker_transport -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::rejected_entry_releases_its_local_actor -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::foreign_resume_owner_cannot_consume_or_fail_another_actor -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::resume_failure_propagates_and_releases_all_direct_path_ownership -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test::execution_runtime_interleaves_owner_scoped_continuations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_continuation_resume_requires_exact_process_request_and_continuation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_continuation_parking_rejects_duplicate_and_zero_identities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::actor_exit_releases_native_continuation_ownership -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_send_transition_delivers_before_exact_owner_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_send_transition_rejects_invalid_ownership_without_mailbox_mutation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_receive_transition_consumes_typed_mailbox_value_before_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_receive_transition_retains_lease_and_nonmatching_mailbox_values -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_spawn_transition_creates_scheduled_child_before_parent_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_spawn_transition_rejects_invalid_ownership_without_child_creation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_timer_transition_fires_before_exact_owner_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_timer_transition_rejects_invalid_ownership_without_wakeup -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_link_transition_creates_failure_relationship_before_owner_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_link_transition_rejects_invalid_ownership_without_relationship_mutation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_monitor_transition_allocates_reference_before_down_delivery -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_monitor_transition_rejects_missing_targets_before_reference_allocation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_resource_transition_registers_owned_handle_and_cleans_up_on_exit -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_resource_transition_rejects_invalid_authority_before_allocation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_cancellation_records_target_before_resuming_exact_owner -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_cancellation_rejects_invalid_authority_before_target_mutation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_self_cancellation_wins_before_resume_boundary -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_native_failure_test::native_failure_uses_vm_exit_propagation_monitoring_and_cleanup -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_native_failure_test::native_failure_rejects_invalid_authority_and_code_before_exit -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_native_scheduling_test::native_scheduling_reclassifies_owner_before_exact_resume -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_native_scheduling_test::native_scheduling_rejects_foreign_owner_without_reclassification -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_send_transition_dispatches_through_vm_mailbox_ownership -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_receive_transition_dispatches_typed_mailbox_result -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_spawn_transition_dispatches_vm_owned_child_identity -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_timer_transition_dispatches_vm_owned_deadline -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_link_transition_dispatches_vm_owned_failure_relationship -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_monitor_transition_dispatches_vm_owned_reference -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_resource_transition_dispatches_vm_owned_identity -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_cancellation_transition_dispatches_scheduler_owned_request -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_failure_transition_dispatches_vm_owned_abnormal_exit -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_scheduling_transition_dispatches_vm_owned_reclassification -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_accepts_active_typed_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_send_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_receive_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_spawn_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_timer_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_link_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_monitor_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_resource_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_cancellation_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_failure_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib vm::main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_scheduling_before_parking -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_test::surface_intrinsics::syntax_output_lowering_canonicalizes_process_send_int_transition -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_process_receive_int_transition -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_keeps_entry_operation_and_erases_value_descriptors -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact

vm-native-worker-runtime-check: terlan-vm-artifact-format-check stdlib-native-artifacts-check
	$(TERLAN_QUALITY) vm-native-worker-runtime

vm-io-reactor-runtime-check: vm-native-worker-runtime-check no-default-tokio-runtime-check
	$(TERLAN_QUALITY) vm-io-reactor-runtime

vm-supervision-restart-check: vm-supervision-primitives-check vm-timer-deadline-check
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_parent_strategy_restarts_failed_and_sibling_supervisor_subtrees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_native_boundary_worker_crash_uses_vm_backoff_and_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_handler_pool_memory_exhaustion_restarts_the_group -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_in_flight_shutdown_timeout_cancels_old_actor_and_restarts -- --exact
	$(TERLC) test tests/language/VmSupervisorPolicyTest.terl
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_relationship_test::actor_unlinked_child_termination_preserves_parent_mailbox_progress -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_waits_for_clean_exit_and_cancels_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_deadline_forces_typed_exit_and_restarts_child -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_normal_exit_honors_transient_restart_class -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically -- --exact
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_supervision_restart_test
	$(TERLAN_QUALITY) vm-supervision-restart

vm-http-handler-dispatch-check: vm-io-reactor-runtime-check

vm-http-handler-scheduler-fairness-check: vm-http-handler-dispatch-check | terlan-benchmark-release-bootstrap
	target/release/terlan-benchmark http-aot-performance-self-test
	$(TERLAN_QUALITY) vm-http-handler-scheduler-fairness

vm-http-stateful-actor-session-check: vm-http-handler-scheduler-fairness-check
	$(TERLAN_QUALITY) vm-http-stateful-actor-session

vm-live-template-stream-check: \
	vm-http-stateful-actor-session-check \
	vm-http-sse-check \
	vm-http-websocket-source-check \
	vm-http-websocket-queue-check \
	vm-http-websocket-termination-check \
	vm-http-live-channel-source-check
	$(TERLAN_QUALITY) vm-live-template-stream

vm-live-template-client-protocol-check: \
	vm-live-template-stream-check \
	angular-ts-terlan-integration-check \
	angular-ts-namespace-generation-check
	$(TERLAN_QUALITY) vm-live-template-client-protocol

typed-template-render-mode-check: vm-live-template-client-protocol-check typed-template-interpolation-check
	node editors/vscode/test/template_links_test.js
	$(TERLAN_QUALITY) typed-template-render-mode

web-asset-pipeline-check: \
	typed-template-render-mode-check \
	browser-package-preflight \
	web-profile-preflight
	$(TERLAN_QUALITY) web-asset-pipeline

vm-web-security-policy-check: \
	web-asset-pipeline-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(TERLAN_QUALITY) vm-web-security-policy

vm-web-config-secret-boundary-check: \
	vm-web-security-policy-check \
	http-tls-check \
	web-compose-check
	$(TERLAN_QUALITY) vm-web-config-secret-boundary

vm-web-observability-check: \
	vm-http-serve-config-check \
	vm-runtime-observability-check \
	vm-web-config-secret-boundary-check \
	http-observability-check \
	vm-diagnostics-quality-check
	$(TERLAN_QUALITY) vm-web-observability

vm-http-serve-config-check:
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_precedence_is_default_manifest_environment_cli -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_unsafe_public_default_before_socket_startup -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_accepts_explicit_public_bind_and_tracks_its_origin -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_malformed_and_ambiguous_values -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_unsupported_protocol_and_bad_environment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_fingerprint_and_artifact_are_replay_stable -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_path_escape_and_zero_pressure_limits -- --exact
	$(CARGO) check -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin

vm-runtime-observability-check:
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_schema_spans_every_vm_serve_domain -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_is_bounded_and_reports_overflow_and_partial_failure -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::traceparent_validation_rejects_malformed_and_zero_identifiers -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_correlates_request_without_recording_payloads -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::shutdown_lifecycle_handles_repeated_signals_and_forced_deadline -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_flushes_replayable_events_metrics_and_traces -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_rejects_zero_capacity -- --exact
	! rg -n 'dbg!|println!|eprintln!' crates/terlan/src/commands/serve/config.rs crates/terlan/src/commands/serve/observability.rs

vm-web-lifecycle-health-check: \
	vm-web-observability-check \
	web-compose-check \
	http-tls-check \
	vm-source-hot-reload-check
	$(TERLAN_QUALITY) vm-web-lifecycle-health

vm-web-deployment-profile-check: \
	vm-web-lifecycle-health-check \
	http-router-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(TERLAN_QUALITY) vm-web-deployment-profile

vm-web-route-schema-client-check: \
	vm-web-deployment-profile-check \
	api-schema-check \
	web-profile-preflight
	$(TERLAN_QUALITY) vm-web-route-schema-client

vm-model-sync-store-check: \
	vm-web-route-schema-client-check \
	native-boundary-postgres-check \
	db-command-check
	$(TERLC) test std/vm/ModelSyncTest.terl
	$(TERLAN_QUALITY) vm-model-sync-store

vm-persistent-actor-store-check: \
	vm-model-sync-store-check \
	vm-process-model-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check \
	vm-distributed-transport-check
	$(TERLC) check std/vm/PersistentActorTest.terl
	$(TERLAN_QUALITY) vm-persistent-actor-store

vm-persistent-actor-schema-check: \
	vm-persistent-actor-store-check \
	vm-distributed-state-check \
	vm-distributed-transport-check
	$(TERLAN_QUALITY) vm-persistent-actor-schema

vm-persistent-actor-compaction-check: \
	vm-persistent-actor-schema-check \
	vm-distributed-state-check \
	vm-resource-ownership-check
	$(TERLAN_QUALITY) vm-persistent-actor-compaction

vm-persistent-actor-restore-check: \
	vm-persistent-actor-compaction-check \
	vm-distributed-state-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check
	$(TERLAN_QUALITY) vm-persistent-actor-restore

vm-persistent-actor-adapter-conformance-check: \
	vm-persistent-actor-restore-check \
	vm-distributed-state-check
	$(TERLAN_QUALITY) vm-persistent-actor-adapter

vm-persistent-actor-performance-budget-check: vm-persistent-actor-adapter-conformance-check
	TERLAN_BENCH_PERSISTENT_ACTOR_OUTPUT=target/quality/vm-persistent-actor-benchmark.json $(TERLAN_BENCHMARK) vm-persistent-actor-runtime-baseline
	$(TERLAN_QUALITY) vm-persistent-actor-performance

vm-persistent-actor-telemetry-check: vm-persistent-actor-performance-budget-check
	$(TERLAN_QUALITY) vm-persistent-actor-telemetry

vm-persistent-actor-policy-check: vm-persistent-actor-telemetry-check
	$(TERLAN_QUALITY) vm-persistent-actor-policy

vm-http-acme-tls-base-check: vm-timer-deadline-check http-tls-check

vm-http-acme-worker-migration-check: vm-http-acme-tls-base-check
	$(TERLAN_QUALITY) vm-http-acme-worker

vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check
	$(TERLAN_QUALITY) vm-http-acme-cache-custody

vm-http-acme-renewal-rotation-check: vm-http-acme-cache-custody-check
	$(TERLAN_QUALITY) vm-http-acme-renewal

vm-http-acme-tls-production-check: vm-http-acme-renewal-rotation-check
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_accepts_manual_certificate_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::hyper_server::hyper_server_test::vm_owned_tls_serves_http2_selected_by_rustls_alpn -- --exact
	$(CARGO) check -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin
	! rg -n 'serve_tls\.adapter_missing|maintained async Hyper TLS adapter is required' crates/terlan/src/commands/serve

vm-http-protocol-readiness-check: vm-http-acme-tls-production-check
	$(TERLC_EXACT_TEST) commands::serve::hyper_server::http2::http2_test::http2_limits_bound_streams_flow_headers_frames_and_owner_tasks -- --exact
	$(TERLC_EXACT_TEST) commands::serve::hyper_server::http2::http2_test::owner_local_http2_executor_fails_loudly_at_capacity -- --exact
	$(TERLC_EXACT_TEST) runtime::vm::http::deadline_test::vm_http_tcp_server_deadline_cancels_parked_handler_and_closes_stream -- --exact
	$(TERLC_EXACT_TEST) runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_does_not_count_cancellation_as_completion -- --exact
	grep -F 'hyper = { version = "1.10.1", features = ["client", "http1", "http2", "server"] }' crates/terlan/Cargo.toml

terlan-vm-run-command-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_defaults_to_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_accepts_vm_and_rejects_erlang_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_rejects_unsupported_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::build_command_for_run_appends_default_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::build_command_for_run_preserves_explicit_target -- --exact
		$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::find_native_image_for_source_ignores_other_native_images -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::find_native_image_for_source_rejects_transitional_artifact -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::run_built_native_image_executes_vm_runner -- --exact

terlan-vm-repl-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact

terlan-vm-test-command-check:
	target/debug/terlc test scripts/self_validation/VmCliExactSelectorSurfaceTest.terl
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::configuration_cases::parse_test_args_accepts_default_terlan_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::configuration_cases::parse_test_args_defaults_to_tests_directory -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::configuration_cases::parse_test_args_rejects_explicit_erlang_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::configuration_cases::parse_test_args_accepts_explicit_terlan_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_executes_bool_test -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::execution_cases::run_test_defaults_to_terlan_vm_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::execution_cases::run_project_directory_tests_default_to_vm_and_prepare_source_roots -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_fails_false_bool_test -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::test::test_command_test::execution_cases::run_terlan_vm_tests_writes_runtime_manifests -- --exact

vm-http-stack-check: \
	vm-tcp-framing-check \
	vm-http-in-memory-transport-check \
	vm-http-router-middleware-check \
	vm-http-static-streaming-check \
	vm-http-sse-check \
	vm-http-websocket-source-check \
	vm-http-websocket-upgrade-check \
	vm-http-websocket-queue-check \
	vm-http-websocket-policy-check \
	vm-http-websocket-tls-check \
	vm-http-websocket-termination-check \
	vm-http-live-channel-source-check \
	vm-http-concurrency-hot-reload-check

vm-http-concurrency-hot-reload-check: \
	vm-http-queue-check \
	http-session-actor-check \
	vm-source-hot-reload-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::request_exchange::vm_http_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::request_exchange::vm_http_tcp_actor_poll_rejects_missing_and_exited_handler_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_accepts_tcp_stream_into_handler_process_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_finishes_tcp_handler_by_closing_stream_and_exiting_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_finishes_cancelled_tcp_handler_with_error_reason -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_polls_runnable_handlers_and_skips_parked_handlers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_reuses_handler_for_pipelined_requests -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_parks_idle_handler_and_wakes_on_later_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_accept_limit_bounds_accept_work_per_poll -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_handler_limit_bounds_handler_work_per_poll -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_handler_budget_uses_round_robin_cursor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_keep_alive_honors_connection_close_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_keep_alive_reports_half_closed_truncated_body -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_keep_alive_reports_half_closed_partial_headers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_cancels_parked_handler_and_closes_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_rejects_invalid_transitions_without_closing_listener -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_completes_woken_handler_before_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_forces_parked_handler_at_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_does_not_count_cancellation_as_completion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::lifecycle_test::vm_http_tcp_server_tls_drain_removes_plan_only_after_terminal_tick -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::protocol::lifecycle_hooks::lifecycle_hooks_test::vm_http_lifecycle_hook_observes_ordered_worker_request_channel_and_shutdown_events -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::protocol::lifecycle_hooks::lifecycle_hooks_test::vm_http_lifecycle_hook_rejects_request_before_handler_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::protocol::lifecycle_hooks::lifecycle_hooks_test::vm_http_lifecycle_hook_can_reject_drain_without_closing_listener -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::protocol::lifecycle_hooks::lifecycle_hooks_test::vm_http_lifecycle_hook_channel_rejection_rolls_back_process_and_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_queue_overload_policies_preserve_full_queue_and_work_ownership -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_queue_overload_policies_enqueue_when_capacity_is_available -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_server_queue_policy_backpressures_at_the_listener_bound -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_server_reject_policy_closes_saturated_work_without_leaking_a_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_server_spill_policy_reports_fallback_admission -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::overload_test::vm_http_server_saturation_stress_preserves_policy_accounting_and_cleanup -- --exact
	@$(TERLC) test std/http/RouterTest.terl
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::handler_lifecycle::vm_http_tcp_server_shutdown_closes_listener_and_active_handlers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_inspects_listener_pressure_and_handler_counters -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_propagates_handler_errors_without_finishing_handler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_reports_missing_retained_handler_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_cancel_adjusts_round_robin_cursor_edges -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::shutdown_and_inspection::vm_http_tcp_server_shutdown_with_tls_removes_listener_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::transport_fixtures::vm_http_tcp_server_reports_plaintext_transport_mode -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::transport_fixtures::vm_http_tcp_server_reports_tls_transport_mode -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::transport_fixtures::vm_http_tcp_server_reports_missing_transport_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_poll_with_tls_allows_plaintext_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_poll_with_tls_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_allows_plaintext_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_honors_connection_close_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_accept_limit_preserves_accept_budget -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_accept_limit_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_limits_preserves_scheduler_budgets -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http::http_test::tls_transport::vm_http_tcp_server_keep_alive_with_tls_limits_handles_encrypted_transport -- --exact

terlan-vm-http-lane-check: \
	vm-tcp-framing-check \
	vm-http-stream-serve-check \
	vm-http-router-middleware-check \
	http-session-actor-check \
	vm-http-sse-check \
	vm-http-live-channel-source-check

vm-http-stream-serve-check:
	$(RUST_TEST) -p terlan --lib vm_stream_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::build_test::tests::js_target_diagnostics_test::build_command_rejects_function_head_pattern_for_js_target -- --exact

vm-http-in-memory-transport-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::http::http_test

vm-http-router-middleware-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_router::route_concurrency_test::vm_http_router_middleware_bounded_concurrency_smoke -- --exact
	$(RUST_TEST) -p terlan --lib discover_web_handlers_rejects_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::asset_and_response_manifests::write_browser_package_serializes_constant_handlers_as_static_responses -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers_from_modules_extracts_grouped_router_builder_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_prefers_dynamic_handler_over_file_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_prefers_dynamic_handler_over_static_response_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact
	@$(TERLC) test std/http/RouterTest.terl

vm-http-sse-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::sse::sse_test

vm-http-websocket-source-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_adapter_frame_constructors_build_typed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_adapter_endpoint_validates_channel_limits -- --exact
	@$(TERLC) test std/http/WebSocketTest.terl

vm-http-websocket-upgrade-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::route_fixtures::discover_web_route_manifest_extracts_websocket_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::upgrades_and_acme::vm_stream_request_returns_websocket_upgrade_handshake_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::upgrades_and_acme::vm_stream_websocket_upgrade_activates_materialized_router_middleware -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::session_frames::vm_websocket_runtime_accept_upgrade_binds_stream_and_endpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::session_frames::vm_websocket_runtime_accept_upgrade_rejects_blank_key_without_session -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::session_frames::vm_websocket_runtime_accept_upgrade_rejects_inactive_stream_without_session -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::session_frames::vm_websocket_runtime_accept_upgrade_rejects_duplicate_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_upgrade_response_serializes_http1_switching_protocols -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_upgrade_response_serialization_rejects_invalid_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_upgrade_response_writes_to_peer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_upgrade_response_rejects_closed_stream -- --exact

vm-http-websocket-queue-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_endpoint_opens_bounded_inbound_queue -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_inbound_queue_preserves_order_and_pressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_inbound_queue_rejects_full_and_oversized_frames -- --exact

vm-http-websocket-policy-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_endpoint_declares_binary_payload_rejection_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_decode_client_frame_rejects_binary_frame -- --exact

vm-http-websocket-tls-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_tls_upgrade_response_writes_to_peer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_tls_upgrade_response_rejects_stream_mismatch -- --exact

vm-http-websocket-termination-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::termination::vm_websocket_runtime_timeout_termination_sends_close_and_reason -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::termination::vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame -- --exact

vm-http-live-channel-source-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::route_fixtures::discover_web_route_manifest_extracts_grouped_sse_routes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::observability_and_packages::package_validation_test::validate_web_package_rejects_sse_route_conflicting_with_http_handler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	@$(TERLC) test std/http/RouterTest.terl
	@$(TERLC) test std/http/SseTest.terl
	@$(TERLC) test std/http/WebSocketTest.terl
	@$(TERLC) test std/http/LiveChannelTest.terl

vm-in-memory-stream-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reads_writes_and_closes_raw_streams -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_preserves_partial_exact_frame_across_polls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_raw_read_drains_staged_bytes_first -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reads_delimited_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reads_fragmented_length_prefixed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_writes_length_prefixed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reports_eof_for_half_closed_partial_frame -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reports_timeout_for_pending_exact_read -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reports_cancelled_streams -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_rejects_bounded_buffer_overflow -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reports_backpressure_from_peer_inbox -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::framing::framing_test::vm_framing_fixture_reports_closed_reader_stream -- --exact
	mkdir -p target/quality
	target/debug/terlan-vm benchmark-in-memory-framing --iterations 100 --payload-bytes 128 >target/quality/terlan-vm-in-memory-framing-benchmark.json

vm-tcp-framing-check: vm-in-memory-stream-check

vm-http-static-streaming-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_table_infers_content_type_and_cache_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_table_marks_fingerprinted_assets_immutable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_table_preserves_manifest_overrides -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_table_rejects_invalid_manifest_entries -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_table_rolls_back_failed_manifest_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_modes_are_explicit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_stream_plan_requires_bounded_nonzero_limits -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_converts_text_and_binary_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_converts_static_asset_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_asset_emits_typed_byte_range_responses -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_static_asset_clamps_and_rejects_adversarial_ranges -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_converts_sse_events_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_rejects_invalid_sse_event_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http_static_test::vm_http_response_body_rejects_stream_conversion_until_emitter_exists -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_splits_and_partially_flushes_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_applies_atomic_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_finishes_and_aborts_with_stable_states -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_flushes_to_tcp_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_parks_and_retries_tcp_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::stream_test::vm_http_response_stream_aborts_on_terminal_tcp_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_writes_head_chunks_and_end_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_preserves_chunk_during_tcp_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_rejects_invalid_metadata_and_terminal_races -- --exact

vm-http-queue-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::http::http_test

vm-tcp-stream-check:
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::core_primitive_intrinsic_resolves_vm_library_primitives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::vm_library_primitive_registry_keys_are_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::vm_library_primitive_return_types_are_registered -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::core_primitive_intrinsic_rejects_wrong_vm_primitive_arities -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_accepts_streams_and_moves_bytes_between_peers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_preserves_accept_order_and_splits_large_receives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_applies_listener_backlog_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_inspects_listener_backlog_waiters_and_closed_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_write_half_close_blocks_sender_but_allows_peer_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_applies_stream_inbox_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_parks_send_and_reports_wakeup_when_peer_drains_capacity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_rejects_closed_cancelled_and_invalid_resources -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_rejects_zero_receive_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_parks_accept_and_reports_wakeup_when_connection_arrives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp::tcp_test::tcp_runtime_parks_receive_and_reports_wakeup_when_bytes_arrive -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_accept_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_read_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_write_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_reports_missing_and_exited_wake_targets -- --exact


http-session-actor-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_lookup_creates_actor_and_sticky_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_reuses_actor_and_table_state_for_cookie_lookup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_rotate_changes_cookie_without_losing_actor_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_expiration_cleans_actor_table_and_reports_stale -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_recovery_policy_can_fail_closed_for_stale_cookie -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::http_session::http_session_test::live_template_state::http_session_rejects_invalid_runtime_configuration -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::http_session::http_session_test

vm-process-model-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::process
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_send_test::actor_custom_name_registry_preserves_via_registration_routing_and_cleanup_semantics -- --exact

vm-scheduler-contract-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler

vm-actor-mutator-ownership-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_runs_runnable_process_and_requeues_yielded_slice -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact

vm-parallel-messages-suite-parity-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_parallel_messages_beam_suite_parity_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test::actor_directory_accepts_cross_thread_producers_before_owned_integration -- --exact
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@rg -q 'VmActorDirectoryError::MailboxFull' crates/terlan/src/runtime/vm/actor_parallel_messages_beam_suite_parity_test.rs

vm-efile-suite-parity-check:
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::efile_beam_suite_parity_test::efile_suite_repeated_reads_release_descriptors_and_recover_worker_capacity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::efile_beam_suite_parity_test::efile_suite_reads_zero_sized_proc_file_repeatedly_without_empty_results -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::sandbox::sandbox_test::sandbox_limit_attestation_is_exact -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::sandbox::sandbox_test::linux_sandbox_command_is_closed_and_bounded -- --exact
	@rg -q 'std\.io\.file\.read_text' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs crates/terlan/src/runtime/native_boundary/dispatch.rs
	@rg -q 'EFILE_REPETITIONS: usize = 10' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs
	@rg -q 'PROC_READ_REPETITIONS: usize = 500' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs

vm-float-native-arithmetic-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::float_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::float::test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::syntax_output_lowering_to_core_maps_selected_float_alias_to_intrinsic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::finite_float_patterns_and_equality_execute -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::non_finite_float_pattern_fails_closed -- --exact
	target/debug/terlc check std/core/FloatTest.terl
	@rg -q 'status::FLOAT_OVERFLOW' crates/terlan/src/compiler/native_ir/float_suite_native_parity_test.rs
	@rg -q 'status::FLOAT_DIVISION_BY_ZERO' crates/terlan/src/compiler/native_ir/float_suite_native_parity_test.rs

vm-fun-suite-parity-check: tvm-aot-application-closure-check tvm-aot-owned-closure-representation-check tvm-aot-closure-dispatch-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::fun_suite_native_parity_test
	@rg -q 'function arity mismatch: expected 1 args, found 0' crates/terlan/src/compiler/native_ir/fun_suite_native_parity_test.rs
	@rg -q 'cannot unify Int with' crates/terlan/src/compiler/native_ir/fun_suite_native_parity_test.rs
	@rg -q 'MAX_OWNED_CLOSURE_CAPTURES' crates/terlan/src/compiler/native_ir/closure_conversion.rs
	@rg -q 'ManagedClosureImageGeneration' crates/terlan/src/runtime/native_image/managed/closures.rs

vm-gc-suite-parity-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::gc_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::managed_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::mailbox_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::managed_execution_test::completed_actor_heap_capacity_is_reused_with_a_fresh_owner_token -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::managed_execution_test::fixed_owner_heap_resets_in_place_with_stale_token_protection -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_alias_test::actor_alias_send_rejects_unknown_alias_without_mailbox_mutation -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor::tests::actor_alias_test::actor_exit_revokes_all_owned_aliases_without_touching_other_owners -- --exact
	@rg -q 'pub fn collect' crates/terlan/src/runtime/native_image/managed/heap.rs
	@rg -q 'pub fn reclaim_all' crates/terlan/src/runtime/native_image/managed/heap.rs
	@! rg -q 'Mutex|RwLock|thread_local!' crates/terlan/src/runtime/native_image/managed/heap.rs

vm-guard-suite-parity-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::guard_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::case_lowering_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::structured_case_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_refines_case_guards_on_formal_path -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_non_bool_case_guard -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::pattern_test::generic_algorithms_and_guards::syntax_output_rejects_impure_case_guard_assignment -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::operators_fields_and_control::syntax_output_boolean_binary_ops_require_bool_operands -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::sequences::managed_sequence_test::binary_slices_enforce_bounds_and_bit_order -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::binary_pattern_operations_match_and_extract_checked_fields -- --exact
	@rg -q 'return Ok\(NativeExpr::If' crates/terlan/src/compiler/native_ir/expression.rs
	@rg -q 'check_clause_guard_purity' crates/terlan/src/compiler/typeck/expression/control_flow.rs

vm-guard-no-opt-suite-parity-check: vm-guard-suite-parity-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::guard_no_opt_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::codegen_policy_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::case_lowering_test::scalar_case_source_executes_through_linked_native_object -- --exact
	@rg -q 'Self::Development => "none"' crates/terlan/src/compiler/native_ir/codegen_policy.rs
	@rg -q 'Self::Serve \| Self::Release => "speed"' crates/terlan/src/compiler/native_ir/codegen_policy.rs
	@rg -q 'assert_eq!\(count_calls\(body, "next"\), 1\)' crates/terlan/src/compiler/native_ir/case_lowering_test.rs

vm-hash-suite-parity-check: vm-deterministic-hashmap-check
	$(RUST_TEST) -p terlan --lib runtime::vm::value::hash_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::bitstring::bitstring_test::bitstring_canonicalizes_storage_and_discards_unrepresented_bytes -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::bitstring::bitstring_test::bitstring_slices_aligned_and_unaligned_ranges_in_network_order -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::map_value::map_value_test::achamp_indexed_map_uses_collision_node_for_equal_hashes -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::map_value::map_value_test::achamp_indexed_map_compresses_long_shared_hash_prefixes -- --exact
	@rg -q 'enum Task' crates/terlan/src/runtime/vm/value_hash.rs
	@rg -q 'FinishMap' crates/terlan/src/runtime/vm/value_hash.rs
	@! rg -q 'DefaultHasher|RandomState' crates/terlan/src/runtime/vm/value_hash.rs

vm-hello-suite-parity-check: \
	tvm-aot-application-conformance-check \
	vm-float-native-arithmetic-check \
	vm-fun-suite-parity-check \
	vm-gc-suite-parity-check \
	vm-guard-suite-parity-check \
	vm-hash-suite-parity-check \
	vm-small-suite-parity-check \
	vm-process-model-check \
	vm-failure-primitives-check \
	vm-timer-primitives-check \
	vm-table-primitives-check \
	vm-code-server-check \
	binary-runtime-suite-check \
	vm-native-worker-runtime-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::hello_suite_native_parity_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::vm_run_loads_hello_world_source_and_executes_main -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::vm::vm_test::vm_run_rejects_unwired_capability_instead_of_spinning -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::aot3_conformance_test::one_image_executes_closed_application_features_and_rejects_unbounded_peers -- --exact
	@rg -q 'without a runtime IR fallback' crates/terlan/src/commands/vm_test.rs
	@rg -q 'runtime CoreIR interpretation has been removed' crates/terlan/src/compiler/native_ir/application.rs

vm-small-suite-parity-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::small_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::scalar_replacement_index_test
	@rg -q 'sadd_overflow' crates/terlan/src/compiler/native_ir/cranelift.rs
	@rg -q 'ssub_overflow' crates/terlan/src/compiler/native_ir/cranelift.rs
	@rg -q 'smul_overflow' crates/terlan/src/compiler/native_ir/cranelift.rs
	@rg -q 'status::DIVISION_BY_ZERO' crates/terlan/src/compiler/native_ir/cranelift.rs
	@rg -q 'status::OVERFLOW' crates/terlan/src/compiler/native_ir/small_suite_native_parity_test.rs

vm-smoke-suite-parity-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_smoke_beam_suite_parity_test
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::dense_and_sparse_integer_dispatch_executes_through_linked_native_object -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	@rg -q 'cfg!\(target_has_atomic = "32"\)' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs
	@rg -q 'cfg!\(target_has_atomic = "64"\)' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs
	@rg -q 'VM_MAX_SCHEDULERS' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs

vm-multicore-mailbox-publication-check: vm-process-model-check vm-failure-primitives-check
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::mailbox::mailbox_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test::publication_during_execution_prevents_lost_wakeup_park -- --exact
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@! rg -q 'ConcurrentQueue::unbounded\(\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_signal_beam_suite_parity_test::signal_suite_contended_enqueue_inspection_and_single_wakeup_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_signal_beam_suite_parity_test::signal_suite_message_before_down_order_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::failure::failure_erl_link_parity_test::erl_link_suite_portable_link_monitor_race_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::process::process_registry_test::process_registry_exit_removes_every_name_before_reuse -- --exact

vm-multicore-fixed-placement-check: vm-multicore-mailbox-publication-check vm-scheduler-fairness-check rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_telemetry::fixed_scheduler_telemetry_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::shard_owner::shard_owner_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test

tvm-aot-multicore-migration-check: tvm-aot-runtime-transition-check tvm-managed-memory-check vm-actor-mutator-ownership-check rust-quality-check
	$(RUST_TEST) -p terlan --lib transfer_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact

vm-multicore-work-stealing-policy-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::work_stealing::work_stealing_test

vm-multicore-work-stealing-check: vm-multicore-work-stealing-policy-check tvm-aot-multicore-migration-check vm-scheduler-fairness-check rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::shard_owner::shard_owner_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_yields_requeue_before_each_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::resumed_generated_aot_actor_yields_before_replying -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test::queued_actor_migration_publishes_destination_before_reacquisition -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_runnable_actor_is_stolen_between_scheduler_owners -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::stolen_generated_actor_retains_destination_route_when_it_parks -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::rejected_generated_runnable_steal_rolls_back_without_actor_loss -- --exact
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_runnable_classes_receive_weighted_local_service -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_multicore_fanout_completes_under_adversarial_class_skew -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_runnable_shutdown_reclaims_queued_class_work -- --exact

vm-multicore-runtime-cleanup-check:
	@! rg -q 'VmWorkStealingRuntime|VmSchedulerStealClaim|VmActorStealClaim' crates/terlan/src
	@! rg -q '^(vm-multicore-work-stealing-owner-check|tvm-aot-multicore-yield-queue-check|tvm-aot-multicore-runnable-steal-check|tvm-aot-multicore-policy-coordination-check):' Makefile
	@! rg -q 'hidden MC-5|next MC-5|staged MC-5|Activated by the MC-6|Used when MC-6' crates/terlan/src/runtime/vm crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache

tvm-aot-multicore-io-epoch-check: vm-multicore-work-stealing-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::stale_io_completion_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact

vm-multicore-timer-epoch-check: tvm-aot-multicore-io-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::current_timer_tick_delivers_once_through_shard_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::foreign_shard_timer_tick_fails_before_timer_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::stale_timer_tick_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_runtime_transfer_test::actor_runtime_transfer_moves_delayed_message_with_exact_timer_deadline -- --exact

vm-multicore-timer-scheduler-check: vm-multicore-timer-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_parks_until_scheduler_owned_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_does_not_block_peer_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_is_cancelled_by_scheduler_shutdown -- --exact

vm-multicore-protocol-reactor-check: vm-multicore-timer-scheduler-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::protocol_task_executor::protocol_task_executor_test::protocol_completion_origin_rejects_foreign_and_ambient_threads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::immediate_callback_executes_on_its_protocol_owner_without_rpc -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_completion_stays_on_protocol_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_rejects_same_scheduler_foreign_connection -- --exact

vm-multicore-capability-worker-check: vm-multicore-protocol-reactor-check tvm-aot-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_enforces_bounded_non_reentrant_admission -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_duplicate_completion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_cancellation_releases_exact_request_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_replaces_crashed_slot_without_capacity_bypass -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_rejects_identity_and_capability_bypass -- --exact

vm-multicore-capability-completion-check: vm-multicore-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_completes_an_already_parked_generated_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_late_already_parked_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_capability_completion_is_published_before_owner_dispatch -- --exact
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@if rg -n 'capability_dispatch_missing' crates/terlan/src/commands/serve/handler_cache/shard_owner; then \
		echo 'error[multicore.capability]: generated capability suspensions must not be rejected'; \
		exit 1; \
	fi

vm-multicore-capability-event-pump-check: vm-multicore-capability-completion-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_correlates_completion_with_fixed_owner_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_returns_payload_on_backpressure_and_cancellation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_drains_generation_payloads_on_worker_loss -- --exact
	@rg -q 'VmCapabilityWorkerEventPump' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs
	@if rg -n 'HashMap|mpsc::channel\(\)' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs; then \
		echo 'error[multicore.capability_event_pump]: correlation must remain deterministic and bounded by worker credits'; \
		exit 1; \
	fi

vm-multicore-capability-scheduler-check: vm-multicore-capability-event-pump-check | terlan-native-worker-bootstrap
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_shutdown_returns_all_pending_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::protocol_test::worker_admits_declared_filesystem_operation -- --exact
	TERLAN_NATIVE_WORKER=$(CURDIR)/target/debug/terlan-native-worker TERLAN_TEST_AOT_CAPABILITY_PUMP=1 TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX=1 $(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_capability_event_pump_executes_real_worker_full_cycle -- --ignored --exact
	TERLAN_NATIVE_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_capability_worker_wakes_and_resumes_exact_actor -- --ignored --exact
	@rg -q 'GeneratedCapabilityDispatcher' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs

vm-epmd-discovery-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::epmd_protocol_round_trips_alive2_and_rejects_malformed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::epmd_registry_owns_registration_until_exact_connection_closes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::logical_node_registers_only_after_pool_listener_and_router_are_ready -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::one_logical_registration_survives_scheduler_owner_migration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::node_shutdown_closes_admission_before_unregistering -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::fixed_scheduler_connection_handler_owns_alive_registration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::fixed_scheduler_connection_handler_rejects_bad_alive_name_without_registration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::logical_node_router_publishes_to_current_actor_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::logical_node_transport_frame_is_bounded_and_actor_addressed -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::epmd::epmd_test::logical_node_bootstrap_runs_discovery_transport_and_shutdown_full_cycle -- --ignored --exact
	@rg -q 'VmEpmdConnection' crates/terlan/src/runtime/vm/epmd/transport.rs
	@rg -q 'start_protocol_tasks_with_topology' crates/terlan/src/runtime/vm/epmd/bootstrap.rs
	@rg -q 'resolve_route' crates/terlan/src/runtime/vm/epmd/node_transport.rs crates/terlan/src/runtime/vm/fixed_scheduler_control.rs

vm-multicore-runtime-integration-check: vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-timer-deadline-check native-boundary-runtime-adversarial-check vm-http-concurrency-investigation-check rust-quality-check

vm-multicore-replay-observability-check: rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::multicore_replay::multicore_replay_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_telemetry::fixed_scheduler_telemetry_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::shard_owner::shard_owner_test::panic_detail_is_bounded_and_stable_for_all_payload_classes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::support_bundle::support_bundle_test::native_support_bundle_serializes_validated_multicore_evidence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_yields_requeue_before_each_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_runnable_actor_is_stolen_between_scheduler_owners -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_parks_until_scheduler_owned_deadline -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::debugger_control::debugger_control_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::debugger_pause_and_step_follow_owner_migration_without_duplicate_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::detached_actor_generation_blocks_source_reload_and_rejects_replaced_destination -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_crash_recovery_rejects_early_restart_and_stale_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::orderly_shard_shutdown_records_one_generation_qualified_lifecycle -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_runnable_shutdown_reclaims_queued_class_work -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::scheduler_panic_fails_the_whole_handler_generation_closed -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	@rg -q 'VM_MULTICORE_REPLAY_SCHEMA' crates/terlan/src/runtime/vm/multicore_replay.rs
	@rg -q 'record_with_context' crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@rg -q 'publish_identified' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs crates/terlan/src/commands/serve/handler_cache/shard_owner/timer_dispatch.rs crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs
	@rg -q 'record_dispatch' crates/terlan/src/commands/serve/handler_cache/shard_owner/owner_loop.rs
	@rg -q 'ExecutionStarted' crates/terlan/src/commands/serve/handler_cache/shard_owner/replay_events.rs crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@rg -q 'MigrationStarted' crates/terlan/src/commands/serve/handler_cache/shard_owner/migration.rs
	@rg -q 'VmMulticoreReplayEvidence' crates/terlan/src/runtime/vm/multicore_replay.rs crates/terlan/src/runtime/vm/support_bundle.rs crates/terlan/src/commands/serve/handler_cache/replay_evidence.rs
	@rg -q 'ImageGeneration' crates/terlan/src/commands/debug/session.rs crates/terlan/src/runtime/vm/source_reload.rs
	@rg -q 'DebuggerStepped' crates/terlan/src/runtime/vm/multicore_replay.rs crates/terlan/src/commands/serve/handler_cache/shard_owner/owner_loop.rs
	@rg -q 'ActorTransfer' crates/terlan/src/runtime/vm/native_image_diagnostics.rs crates/terlan/src/runtime/vm/pure_native/execution_shard/generation_lifetime.rs
	@rg -q 'SupervisionRestartScheduled' crates/terlan/src/runtime/vm/multicore_replay.rs crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'ShutdownStarted' crates/terlan/src/runtime/vm/multicore_replay.rs crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'SchedulerPanicked' crates/terlan/src/runtime/vm/multicore_replay.rs crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@rg -q 'AotSchedulerPanicEvidence' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs crates/terlan/src/commands/serve/handler_cache/shard_owner/panic_evidence.rs
	@! rg -q 'SystemTime|Instant|thread::current' crates/terlan/src/runtime/vm/multicore_replay.rs

vm-multicore-performance-check: rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::multicore_performance_test -- --nocapture
	$(MAKE) vm-multicore-performance-record

vm-multicore-performance-record:
	test -s benchmarks/baselines/vm-multicore-performance-limits.json
	TERLAN_VM_MULTICORE_PERFORMANCE_OUTPUT=$(CURDIR)/target/quality/vm-multicore-performance.json $(EXACT_CARGO_TEST) --locked --release -p terlan --lib commands::serve::handler_cache::multicore_performance_test::multicore_runtime_width_matrix_records_workloads_and_owner_overlap -- --ignored --exact --nocapture
	test -s target/quality/vm-multicore-performance.json
	@rg -q '"schema": "terlan.vm-multicore-performance.v1"' target/quality/vm-multicore-performance.json
	@rg -q '"effective_parallelism":' target/quality/vm-multicore-performance.json
	@rg -q '"maximum_simultaneously_active_schedulers":' target/quality/vm-multicore-performance.json
	@rg -q '"runtime_workload_contract_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"mixed_tail_contract_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"performance_policy_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"source_revision":' target/quality/vm-multicore-performance.json
	@rg -q '"source_tree_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"provenance":' target/quality/vm-multicore-performance.json
	@rg -q '"cpu_bound_actor":' target/quality/vm-multicore-performance.json
	@rg -q '"iterations_per_actor": 200000' target/quality/vm-multicore-performance.json
	@rg -q '"confidence_level": 0.95' target/quality/vm-multicore-performance.json
	@rg -q '"resamples": 4096' target/quality/vm-multicore-performance.json
	@rg -q '"mixed_load_tail":' target/quality/vm-multicore-performance.json
	@rg -q '"cpu_overlap_proven": true' target/quality/vm-multicore-performance.json
	@rg -q '"operations_per_sample": 256' target/quality/vm-multicore-performance.json
	@rg -q '"performance_policy":' target/quality/vm-multicore-performance.json
	@rg -q '"status": "(record_only|passed)"' target/quality/vm-multicore-performance.json
	TERLAN_JSON_EVIDENCE_PATH=target/quality/vm-multicore-performance.json \
	TERLAN_JSON_EVIDENCE_REQUIRED='"requested_schedulers": 1;;"requested_schedulers": 2;;"requested_schedulers": 4;;"workload": "actor_spawn_exit";;"workload": "mailbox_round_trip";;"workload": "timer_delivery";;"workload": "http_handler_response";;"workload": "supervision_restart";;"workload": "epmd_registration_lifecycle";;"metric": "scheduler_wait";;"metric": "mailbox_delivery";;"metric": "timer_delay";;"metric": "http_latency";;"metric": "failed_steal_backoff";;"metric": "allocation_pause";;"metric": "collection_pause"' \
		target/debug/terlc test scripts/self_validation/JsonEvidenceContractTest.terl \
			--name selected_json_evidence_holds

vm-multicore-memory-model-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::multicore_model_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::actor_directory::mailbox::mailbox_test::seeded_mailbox_flood_preserves_every_sender_under_forced_interleaving -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::work_stealing::work_stealing_test::seeded_skew_burst_and_fanout_decisions_remain_bounded_and_work_conserving -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::deadlock_watchdog_terminates_stuck_child -- --exact
	TERLAN_VM_MULTICORE_STRESS_OUTPUT=$(CURDIR)/target/quality/vm-multicore-memory-model.json $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::bounded_seeded_multicore_memory_model_has_deadlock_watchdog -- --exact --nocapture
	test -s target/quality/vm-multicore-memory-model.json
	@rg -q '"schema": "terlan.vm-multicore-memory-model.v1"' target/quality/vm-multicore-memory-model.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-memory-model.json
	@rg -q '"watchdog_timeout_millis": 15000' target/quality/vm-multicore-memory-model.json
	@test "$$(rg -c '0x[0-9a-f]{16}' target/quality/vm-multicore-memory-model.json)" -eq 8

vm-multicore-thread-sanitizer-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-tsan-self-test

vm-multicore-thread-sanitizer-check: vm-multicore-memory-model-check vm-multicore-thread-sanitizer-contract-check
	@if rustup target list --installed --toolchain 1.96.0 2>/dev/null | grep -Fqx 'x86_64-unknown-linux-gnutsan'; then \
		$(TERLAN_TVM_PLATFORM_MATRIX) multicore-tsan-run; \
	elif test "$${GITHUB_ACTIONS:-}" = true; then \
		echo 'error[vm.multicore.tsan]: pinned Rust 1.96.0 ThreadSanitizer target is mandatory in CI'; \
		exit 1; \
	else \
		echo 'VM multicore ThreadSanitizer target unavailable locally; portable memory-model gate passed'; \
	fi
	@test ! -f target/quality/vm-multicore-thread-sanitizer-report.json || rg -q '"source_tree_sha256":' target/quality/vm-multicore-thread-sanitizer-report.json

vm-multicore-mc9-evidence-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-mc9-self-test

vm-multicore-mc9-evidence-check: vm-multicore-mc9-evidence-contract-check
	test -s target/quality/vm-multicore-performance.json
	test -s target/quality/vm-multicore-thread-sanitizer-report.json
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-mc9-seal
	test -s target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"schema": "terlan.vm-multicore-mc9-evidence.v2"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"source_tree_sha256":' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"dedicated_runner_label": "terlan-linux-x86_64-multicore-v1"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"sanitizer_toolchain": "1.96.0"' target/quality/vm-multicore-mc9-evidence.json

vm-multicore-mc9-local-evidence-check:
	@rustup target list --installed --toolchain 1.96.0 | rg -qx 'x86_64-unknown-linux-gnutsan' || { \
		echo 'error[vm.multicore.mc9.local]: install Rust 1.96.0 x86_64-unknown-linux-gnutsan first'; \
		exit 1; \
	}
	$(TERLAN_TVM_PLATFORM_MATRIX) controlled-performance -- \
		env -u GITHUB_ACTIONS \
		RUSTUP_TOOLCHAIN=1.96.0 \
		TERLAN_VM_MULTICORE_DEDICATED_RUNNER=terlan-linux-x86_64-multicore-v1 \
		TERLAN_BENCH_BACKGROUND_LOAD=controlled \
		$(MAKE) vm-multicore-performance-record
	test -s target/quality/vm-multicore-thread-sanitizer-report.json
	$(MAKE) vm-multicore-mc9-evidence-check

VM_MULTICORE_RELEASE_LOCAL_GATES := \
	vm-multicore-invariant-inventory-check \
	vm-actor-mutator-ownership-check \
	vm-multicore-mailbox-publication-check \
	vm-multicore-fixed-placement-check \
	tvm-aot-multicore-migration-check \
	vm-multicore-work-stealing-check \
	vm-multicore-runtime-cleanup-check \
	vm-multicore-runtime-integration-check \
	vm-epmd-discovery-check \
	vm-multicore-replay-observability-check \
	vm-multicore-memory-model-check \
	vm-scheduler-fairness-check \
	tvm-aot-runtime-transition-check \
	tvm-managed-memory-check \
	rust-quality-check

VM_MULTICORE_PUBLISH_REUSED_GATES := \
	vm-multicore-memory-model-check \
	rust-quality-check

VM_MULTICORE_PUBLISH_LOCAL_GATES := $(filter-out $(VM_MULTICORE_PUBLISH_REUSED_GATES),$(VM_MULTICORE_RELEASE_LOCAL_GATES))

vm-multicore-release-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-self-test

vm-multicore-release-record:
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-record
	test -s target/quality/vm-multicore-release-closeout.json
	@rg -q '"schema": "terlan.vm-multicore-release-closeout.v3"' target/quality/vm-multicore-release-closeout.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json
	@rg -q '"source_tree_sha256":' target/quality/vm-multicore-release-closeout.json

vm-multicore-release-check: vm-multicore-release-contract-check
	$(MAKE) vm-multicore-mc9-evidence-check
	$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)
	$(MAKE) vm-multicore-release-record

vm-multicore-publish-evidence-refresh: vm-multicore-release-contract-check
	test -s target/quality/hosted-candidate-validation.json
	@revision=$$(git rev-parse HEAD); rg -q "\"source_revision\": \"$$revision\"" target/quality/hosted-candidate-validation.json
	$(MAKE) vm-multicore-mc9-local-evidence-check
	@rg -q '"source_tree_clean": true' target/quality/vm-multicore-mc9-evidence.json
	TERLAN_RUST_SUITE_ALREADY_RUN=1 $(MAKE) $(VM_MULTICORE_PUBLISH_LOCAL_GATES)
	$(MAKE) vm-multicore-release-record

vm-multicore-publish-check:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-verify

vm-final-health-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact

vm-memory-heap-pressure-check: vm-process-model-check vm-resource-ownership-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::memory::memory_test::limits_and_accounting::memory_accounting_writes_deterministic_pressure_report -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::memory::memory_test::shared_ownership_and_soak::memory_accounting_soak_preserves_ownership_and_writes_benchmark_report -- --exact
	test -f target/quality/vm-memory-pressure-report.json
	test -f target/quality/vm-memory-soak-report.json

vm-scheduler-fairness-check: vm-memory-heap-pressure-check vm-scheduler-contract-check
	$(RUST_TEST) -p terlan --lib runtime::vm::http::response_memory_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::native_boundary::deadline::deadline_test::native_boundary_deadline_charges_only_successful_parks_to_scheduler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_accounting_test::timer_table_charges_only_successful_mailbox_deliveries -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_terminal_accounting_test::scheduler_charges_terminal_reductions_only_after_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_reclassification_accounting_test::scheduler_charges_only_successful_explicit_reclassification -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_cancellation_accounting_test::scheduler_charges_only_successful_cancellation_requests -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_exit_accounting_test::actor_runtime_charges_only_newly_initiated_exit_to_exiting_actor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_checkpoint_accounting_test::actor_runtime_separates_checkpoint_operation_and_memory_reductions -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_timer_accounting_test::actor_runtime_charges_only_successful_timer_scheduling_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_timer_cancellation_accounting_test::actor_runtime_charges_only_successful_timer_cancellation_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_spawn_test::actor_spawn_charges_only_successful_child_creation_to_parent -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_relationship_accounting_test::actor_runtime_charges_only_successful_relationship_operations_to_initiator -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_registry_accounting_test::actor_runtime_charges_only_successful_registry_mutations_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_suspension_accounting_test::actor_runtime_charges_only_successful_suspension_operations_to_actor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_send_accounting_test::actor_runtime_charges_only_successful_send_operations_to_sender -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_receive_accounting_test::actor_runtime_charges_receive_operations_without_charging_invalid_attempts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_fairness_telemetry_is_deterministic_under_cpu_bound_load -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_writes_fairness_report_with_starvation_evidence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_weighted_classes_preserve_order_and_bound_background_wait -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_rejects_silent_reclassification_of_queued_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test
	test -f target/quality/vm-scheduler-fairness-report.json

vm-actor-primitives-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor


vm-failure-primitives-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::failure
	$(RUST_TEST) -p terlan --lib runtime::vm::reference

vm-supervision-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_starts_child_and_exposes_inspection_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_only_failed_child_for_one_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_all_children_for_one_for_all_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_one_for_all_enforces_restart_limit_before_group_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_restarts_failed_and_later_children_for_rest_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_rest_for_one_enforces_restart_limit_before_group_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_temporary_child_never_restarts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_transient_child_restarts_only_after_abnormal_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_skips_non_restartable_children_without_blocking_restartable_siblings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_applies_exponential_restart_backoff_for_one_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_reports_per_child_backoff_delays -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_records_shutdown_timeout_for_live_child_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_group_restart_reports_per_child_shutdown_timeouts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_enforces_restart_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::restart_fixtures::supervision_system_records_supervisor_failure_when_restart_limit_escalates -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_propagates_child_supervisor_failure_to_parent_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_records_restart_history_for_restart_and_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_records_restart_history_for_non_restartable_child -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_child_diagnostic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_supervisor_diagnostic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_rejects_duplicate_child_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_restart_exits_live_child_before_restarting -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_process_instead_of_panicking_on_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::supervision_test::hierarchy_and_history::supervision_system_reports_missing_supervisor_for_restart_and_snapshot -- --exact

vm-timer-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_starts_one_shot_timer_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_interval_timer_fires_and_reschedules -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_coalesces_late_interval_timer_and_reschedules_after_skipped_deadlines -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_overflow_when_interval_reschedule_exceeds_tick_range -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_overflow_when_late_interval_coalescing_exceeds_tick_range -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_deadline_missed_for_late_interval_before_next_interval_boundary -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_rejects_zero_interval_timer_without_installing_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_cancels_timer_and_reports_missing_timer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_distinguishes_manual_cancel_from_owner_exit_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_fires_due_timers_only_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_deadline_missed_for_late_one_shot_timer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_reports_owner_exited_if_due_timer_owner_exited_before_fire -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_fires_equal_deadlines_in_timer_id_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_receive_timeout_wakes_blocked_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_deadline_missed_receive_timeout_still_wakes_blocked_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_rejects_exited_process_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_rejects_receive_timeout_deadline_overflow_without_blocking_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_table_rejects_missing_process_owner -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::timer::timer_test

vm-timer-deadline-check:
	test -f target/quality/vm-timer-deadline-report.json

vm-resource-ownership-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_registers_resource_and_exposes_inspection_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_transfers_transferable_resource_between_live_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_releases_transferred_resource_from_new_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_rejects_owner_only_transfer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleanup_owner_handles_removes_live_process_handle_rows -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_owner_test::resource_table_owner_snapshots_are_ordered_isolated_and_live_only -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_rejects_wrong_owner_access_transfer_and_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_reports_stale_handle_for_transfer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_reports_stale_handle_for_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_rejects_missing_process_roles -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_rejects_exited_process_roles -- --exact

vm-table-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_creates_owner_table_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_inserts_replaces_looks_up_and_deletes_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_reports_missing_or_exited_processes_and_stale_handles -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_delete_returns_none_for_missing_key -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_public_read_allows_reads_but_rejects_non_owner_writes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_owner_only_rejects_non_owner_reads_and_writes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_public_read_write_allows_non_owner_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_public_read_write_allows_non_owner_delete -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_cleans_up_owner_tables_on_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_traversal_preserves_stable_entry_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_traversal_handles_empty_replacement_deletion_and_missing_keys -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::table::table_test::table_store_traversal_enforces_read_access -- --exact

vm-code-server-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_publishes_initial_generation_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_inspection_test::code_server_module_scoped_inspection_excludes_unrelated_lifecycle_traffic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_inspection_test::code_server_tracks_active_coreir_function_exports_across_reload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_inspection_test::code_server_replaces_module_info_lifecycle_fixture -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_inspection_test::code_server_rejects_unloading_process_bound_module_without_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_false_dependency_test::returned_functions_do_not_leave_false_module_generation_dependencies -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_false_dependency_test::nested_calls_release_once_and_failed_entry_is_side_effect_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_failed_lifecycle_operations_are_mutation_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_hot_reload_binds_new_processes_to_new_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_release_retires_drained_old_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_hot_reload_retires_unused_previous_generation_immediately -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_reports_missing_active_generation_and_missing_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_reports_stale_release_binding_and_active_release_noop -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_rejects_duplicate_release_and_keeps_unique_process_bindings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_purges_retired_generations_in_generation_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_reload_after_purge_keeps_generation_identity_monotonic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_purge_preserves_process_bound_retiring_generation_until_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_process_bound_reload_records_ordered_retire_and_purge_events -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_renders_generation_purge_event_without_internal_debug_shape -- --exact



vm-source-hot-reload-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_publishes_compiled_generations_and_preserves_bindings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_publish_source_compiles_and_publishes_new_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_detects_changed_helper_function_body -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_rollback_validates_artifact_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_rollback_keeps_live_replaced_generation_retiring -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_reports_missing_generation_and_active_promote_noop -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::source_hot_reload_records_reload_and_rollback_events_for_inspection -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_publishes_changed_terlan_file_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_publishes_only_sources_from_mixed_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_reports_mixed_batch_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_invalid_mixed_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_report_rejects_invalid_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_unreadable_mixed_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_collapses_duplicate_source_paths_in_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_ignores_non_terlan_paths -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_reports_unreadable_terlan_source -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_invalid_source_without_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_publishes_changed_sources_through_code_server -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_ignores_assets_in_mixed_source_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_parses_diagnostics_flag_as_command_option -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_reports_mixed_batch_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_reload_rejects_non_source_inputs -- --exact

vm-distribution-envelope-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::term_format::term_format_runtime_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_coordination_builds_tetf_distribution_envelope_with_refs -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_distributed_transport_decodes_declared_atom_payload_before_acceptance -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_distributed_transport_rejects_corrupt_or_mismatched_tetf_without_advancing -- --exact

vm-distributed-transport-check: binary-bitstring-processing-check vm-distribution-envelope-check
	$(TERLC) test std/vm/ClusterTest.terl



vm-distributed-scheduling-check: vm-distributed-transport-check
	$(TERLC) check std/vm/SchedulerTest.terl
	$(TERLC) test std/vm/SchedulerTest.terl
	$(TERLC) check std/vm/FaultTest.terl
	$(TERLC) test std/vm/FaultTest.terl

vm-distributed-state-check: vm-distributed-scheduling-check
	$(TERLC) check std/vm/DistributedStateTest.terl
	$(TERLC) test std/vm/DistributedStateTest.terl
	$(TERLC) check std/vm/DistributedStorageTest.terl
	$(TERLC) test std/vm/DistributedStorageTest.terl

vm-distribution-suite-parity-check: vm-distributed-state-check vm-failure-primitives-check










vm-debug-key-compatibility-check:
	$(RUST_TEST) -p terlan --lib key_compatibility_ -- --nocapture


vm-latin1-source-policy-check:
	$(RUST_TEST) -p terlan --lib latin1_source_ -- --nocapture





vm-compiler-transform-retirement-check:
	$(RUST_TEST) -p terlan --lib compiler_transform_retirement_ -- --nocapture

vm-source-column-ownership-check: \
	vm-compiler-transform-retirement-check

vm-source-provenance-artifact-check:

vm-call-dependency-artifact-check:







vm-executable-source-span-artifact-check:


vm-runtime-semantics-check: \
	vm-process-model-check \
	vm-memory-heap-pressure-check \
	vm-native-boundary-contract-check \
	vm-postgres-runtime-check \
	vm-scheduler-contract-check \
	vm-final-health-check \
	vm-actor-primitives-check \
	vm-failure-primitives-check \
	vm-timer-deadline-check \
	vm-supervision-primitives-check \
	vm-resource-ownership-check \
	vm-table-primitives-check \
	vm-code-server-check \
	vm-source-column-ownership-check \
	vm-debug-key-compatibility-check \
	vm-latin1-source-policy-check \
	vm-source-provenance-artifact-check \
	vm-call-dependency-artifact-check \
	vm-executable-source-span-artifact-check \
	vm-source-hot-reload-check \
	vm-distributed-scheduling-check \
	vm-distributed-state-check \
	vm-coordination-docker-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_test::actor_message_wakeup_is_deduplicated_and_missing_target_is_side_effect_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_wakeup_keeps_exactly_one_scheduler_entry -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::timer::timer_test::scheduling_and_overflow::timer_wakeup_preserves_suspension_until_explicit_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_explicit_reclassification_moves_one_queued_entry -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_reclassifies_blocked_process_without_waking_it -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_reclassification_rejects_missing_and_exited_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::checksum::checksum_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::bitstring::bitstring_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::packet::packet_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::base64::base64_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::process::process_location_test::nested_call_frames_restore_continuations_in_lifo_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_test::actor_runtime_selective_receive_retries_after_matching_message_wakes_actor -- --exact

vm-diagnostics-quality-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_diagnostics_quality_test
	$(TERLAN_QUALITY) vm-diagnostics-quality
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_image::native_image_test::native_inspection_rejects_json_and_non_executables -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_image::native_image_test::descriptor_rejects_tampering_and_noncanonical_records -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_test::actor_runtime_reports_missing_and_exited_context_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::repl::repl_test::prompt_state::repl_json_event_without_extra_fields_is_valid_json -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::io_diagnostics::io_diagnostics_test::diagnostic_probe_latches_only_post_install_typed_resource_fault -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::io_diagnostics::io_diagnostics_test::diagnostic_probe_enforces_log_identity_and_close_lifecycle -- --exact

vm-coordination-docker-check:
	target/debug/terlc test scripts/self_validation/VmCoordinationDockerTest.terl

all-terlan-tests-vm-inventory-check:
	target/debug/terlc test scripts/self_validation/AllTerlanTestsVmInventoryTest.terl

all-terlan-tests-vm-check: all-terlan-tests-vm-inventory-check terlan-vm-run-command-check terlan-vm-repl-check terlan-vm-test-command-check flexible-shape-guards-check language-feature-coverage-100-check operator-coverage-100-check pattern-matching-support-check stdlib-release-tests-vm-default-check stdlib-release-tests

std-vm-parity-matrix-check: all-terlan-tests-vm-check

terlc-doctor-vm-pivot-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::parse_doctor_args_defaults_to_current_directory -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::parse_doctor_args_rejects_unknown_option -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_accepts_clean_vm_project -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_reports_vm_pivot_hazards -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_reports_summary_compiler_contract_mismatch -- --exact

no-default-erlang-emission-check: erlang-backend-classification-check

no-default-beam-runtime-check: no-implicit-otp-runtime-check terlan-vm-no-otp-runtime-fallback-check

no-default-tokio-runtime-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools no_default_tokio_runtime_test
	$(TERLAN_QUALITY) no-default-tokio-runtime

no-terlan-vm-erts-rust-dependency-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools no_terlan_vm_erts_rust_dependency_test
	$(TERLAN_QUALITY) no-terlan-vm-erts-rust-dependency

no-implicit-otp-runtime-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools no_implicit_otp_runtime_test
	$(TERLAN_QUALITY) no-implicit-otp-runtime

otp-runtime-exit-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools otp_runtime_exit_test
	$(TERLAN_QUALITY) otp-runtime-exit

vm-hibernate-suite-parity-check: vm-gc-suite-parity-check vm-scheduler-contract-check
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_hibernate_beam_suite_parity_test
	@rg -q 'VmProcessState::Hibernated' crates/terlan/src/runtime/vm/process.rs crates/terlan/src/runtime/vm/process/parking.rs
	@rg -q 'hibernate_process' crates/terlan/src/runtime/vm/scheduler.rs
	@rg -q 'hibernate_owner' crates/terlan/src/runtime/native_image/managed/execution/hibernation.rs

otp-test-pipeline-inventory-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools otp_test_pipeline_inventory_test
	$(TERLAN_QUALITY) otp-test-pipeline-inventory

terlan-vm-erl-suite-audit-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib module_layout_ -- --nocapture
	$(TERLAN_TVM_PLATFORM_MATRIX) erl-suite-audit-self-test
	$(TERLAN_TVM_PLATFORM_MATRIX) erl-suite-audit-check

otp-stdlib-port-check: vm-list-bif-suite-parity-check vm-map-suite-parity-check vm-process-model-check vm-failure-primitives-check vm-supervision-primitives-check vm-timer-primitives-check vm-diagnostics-quality-check vm-efile-suite-parity-check vm-otp-abstractions-terlan-stdlib-check
	$(TERLC) test std/vm/BytesTest.terl
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::coordination::coordination_distribution_beam_suite_parity_test::distribution_suite_bulk_delivery_is_ordered_deduplicated_and_transactional -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::coordination::coordination_distribution_beam_suite_parity_test::distribution_suite_node_lifecycle_restart_and_reconnect_are_generation_safe -- --exact
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::sets::managed_set_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_append_operation_concatenates_validated_values -- --exact
	$(RUST_TEST) -p terlan --lib runtime::native::json::json_test
	$(RUST_TEST) -p terlan --lib runtime::native::path::path_test
	$(RUST_TEST) -p terlan --lib runtime::native::uri::uri_test
	$(RUST_TEST) -p terlan --lib runtime::native::random::random_test
	$(RUST_TEST) -p terlan --lib runtime::native::regex::regex_test

callable-syntax-cleanup-check:
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_function_value_dot_call_syntax -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_parenthesized_function_value_dot_call_syntax -- --exact
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/CallableSyntaxCleanupTest.terl

terlan-lint-style-profile-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools terlan_lint_style_profile_test
	$(TERLAN_QUALITY) terlan-lint-style-profile

terlan-lint-pipe-canonicalization-check: \
	terlan-lint-style-profile-check \
	terlan-lint-style-check \
	formatter-pipe-canonicalization-check
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_default_argument_ambiguous_module_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) commands::lint::lint_test::pipe_test::lint_rejects_default_argument_ambiguous_receiver_pipe_candidate -- --exact

std-test-honesty-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_test_honesty_test
	$(TERLAN_QUALITY) std-test-honesty

std-test-table-check:
	$(EXACT_CARGO_TEST) -p terlan --lib formal_pipeline::formal_pipeline_test::interface_foundations::embedded_std_interfaces_include_float_math_contract -- --exact
	$(RUST_TEST) -p terlan --lib runtime::native::base64::base64_test
	$(RUST_TEST) -p terlan --lib runtime::native::md5::md5_test
	target/debug/terlc check std/test/Test.terl
	target/debug/terlc test \
		std/test/AssertionsTest.terl \
		std/test/TableTest.terl \
		std/test/LifecycleTest.terl \
		std/core/IntTest.terl \
		std/core/FloatTest.terl \
		std/core/StringTest.terl \
		std/encoding/Base64Test.terl \
		std/encoding/Md5Test.terl

std-test-property-check:
	target/debug/terlc test \
		std/collections/ListPropertyTest.terl \
		std/collections/MapPropertyTest.terl \
		std/collections/SetPropertyTest.terl \
		std/binary/BinaryPropertyTest.terl \
		std/core/AtomPropertyTest.terl \
		std/core/ErrorPropertyTest.terl \
		std/core/BoolPropertyTest.terl \
		std/core/FloatPropertyTest.terl \
		std/core/IntPropertyTest.terl \
		std/core/ObjectPropertyTest.terl \
		std/core/OptionPropertyTest.terl \
		std/core/OrderingPropertyTest.terl \
		std/core/ResultPropertyTest.terl \
		std/core/StringPropertyTest.terl \
		std/core/UnitPropertyTest.terl \
		std/data/JsonPropertyTest.terl \
		std/encoding/Base64PropertyTest.terl \
		std/io/PathPropertyTest.terl \
		std/net/UriPropertyTest.terl \
		std/range/RangePropertyTest.terl \
		std/random/RandomPropertyTest.terl \
		std/regex/RegexPropertyTest.terl \
		std/test/GenTest.terl \
		std/test/PropertyDistributionTest.terl \
		std/test/PropertyTest.terl \
		std/test/ShrinkTest.terl \
		std/test/StatefulPropertyTest.terl

std-range-check:
	target/debug/terlc test std/range

std-random-check:
	$(RUST_TEST) -p terlan --lib runtime::native::random::random_test
	target/debug/terlc test std/random

std-regex-check:
	target/debug/terlc test std/regex

std-package-coverage-100-check: shape-implications-check
	$(RUST_TEST) -p terlan --lib --features quality-tools std_package_full_coverage_test
	$(TERLAN_QUALITY) std-package-coverage-100

js-type-emission-contract-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools js_type_emission_contract_test
	$(TERLAN_QUALITY) js-type-emission-contract


vm-otp-abstractions-terlan-stdlib-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_otp_abstractions_terlan_stdlib_test
	$(TERLAN_QUALITY) vm-otp-abstractions-terlan-stdlib
	$(TERLC) test tests/language/VmServiceActorTest.terl
	$(TERLC) test std/vm/TaskTest.terl

vm-ownership-classification-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_ownership_classification_test
	$(TERLAN_QUALITY) vm-ownership-classification

vm-runtime-concept-inventory-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools vm_runtime_concept_inventory_test
	$(TERLAN_QUALITY) vm-runtime-concept-inventory

terlan-runtime-conformance-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::vector::vector_test::vector_mutations_update_storage -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::json::json_test::parse_and_stringify_round_trip_json_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact

terlan-release-train-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_emits_js_module_and_manifest_for_single_file -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib formal_pipeline::formal_pipeline_test::interface_foundations::compile_syntax_module_with_core_v0_profile_accepts_covered_subset -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::postgres_test::config_builders_set_pool_limits_and_timeouts -- --exact

otp-reference-inventory-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools otp_reference_inventory_test
	$(TERLAN_QUALITY) otp-reference-inventory

vm-multicore-invariant-inventory-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools multicore_invariant_inventory_test
	$(TERLAN_QUALITY) vm-multicore-invariant-inventory

vm-otp-corpus-inventory-check: otp-reference-inventory-check

terlan-vm-no-otp-runtime-fallback-check: otp-reference-inventory-check otp-test-pipeline-inventory-check erlang-backend-classification-check terlan-vm-external-repo-boundary-check

hex-target-metadata-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools hex_target_metadata_test
	$(TERLAN_QUALITY) hex-target-metadata

native-no-std-target-feasibility-check: hex-target-metadata-check | terlan-quality-tools-bootstrap
	target/debug/terlan-native-target-feasibility

device-target-planner-check: native-no-std-target-feasibility-check
	$(TERLAN_QUALITY) device-target-planner

terlan-package-git-source-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools package_git_source_test
	$(TERLAN_QUALITY) package-git-source
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::project_manifest::project_manifest_test::project_manifest_rejects_git_dependency_without_rev -- --exact

terlan-package-lockfile-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools package_lockfile_contract_test
	$(TERLAN_QUALITY) package-lockfile-contract

package-resolver-reproducibility-check: device-target-planner-check terlan-package-lockfile-check terlan-package-git-source-check
	$(TERLAN_QUALITY) package-resolver-reproducibility

package-registry-publish-check: package-resolver-reproducibility-check
	$(TERLAN_QUALITY) package-registry-publish

registry-protocol-schema-check: rust-test-suite
	bash scripts/check_registry_protocol_schema_v1.sh

registry-package-archive-check: rust-test-suite
	bash scripts/check_registry_package_archive_v1.sh

registry-lockfile-resolver-check: rust-test-suite
	bash scripts/check_registry_lockfile_resolver_v1.sh

milestone-registry-public-contract: registry-protocol-schema-check registry-package-archive-check registry-lockfile-resolver-check
	@echo "Registry MR0 public contract milestone passed"

registry-cli-integration-check: | terlan-compiler-bootstrap
	bash scripts/check_registry_cli_integration_v1.sh

.PHONY: registry-http-publication-check registry-trusted-resolution-check registry-graph-workflow-check registry-live-frontend-check
registry-http-publication-check: | terlan-compiler-bootstrap
	bash ../terlan-registry/scripts/check-registry-http-publication.sh

registry-trusted-resolution-check: registry-http-publication-check
	@echo "Registry trusted live/conditional/offline resolution passed"

registry-graph-workflow-check: registry-trusted-resolution-check
	@echo "Registry trusted whole-graph workflow passed"

registry-live-frontend-check: registry-graph-workflow-check
	@$(MAKE) -C ../terlan-registry registry-frontend-check TERLC="$(abspath $(TERLAN_BOOTSTRAP_COMPILER))"
	@echo "Registry DB-backed Terlan Angular.ts frontend passed"

registry-adversarial-corpus-check: rust-test-suite
	bash scripts/check_registry_cli_integration_v1.sh
	@echo "Registry adversarial package corpus passed"

package-capability-contract-check: package-registry-publish-check
	$(TERLAN_QUALITY) package-capability-contract

package-release-test-matrix-check: package-capability-contract-check
	$(TERLAN_QUALITY) package-release-test-matrix

package-api-compatibility-check: package-release-test-matrix-check
	$(TERLAN_QUALITY) package-api-compatibility

package-cli-workflow-check: package-api-compatibility-check
	$(TERLAN_QUALITY) package-cli-workflow

package-editor-integration-check: package-cli-workflow-check
	$(TERLAN_QUALITY) package-editor-integration

package-cache-integrity-check: package-editor-integration-check
	$(TERLAN_QUALITY) package-cache-integrity

package-workspace-graph-check: package-cache-integrity-check
	$(TERLAN_QUALITY) package-workspace-graph

package-build-artifact-isolation-check: package-workspace-graph-check
	$(TERLAN_QUALITY) package-build-artifact-isolation

source-map-debug-info-check: package-build-artifact-isolation-check
	$(TERLAN_QUALITY) source-map-debug-info

compiler-incremental-cache-check: source-map-debug-info-check
	$(TERLAN_QUALITY) compiler-incremental-cache

watch-mode-hot-reload-check: compiler-incremental-cache-check
	$(TERLAN_QUALITY) watch-mode-hot-reload

aot-developer-hot-reload-check: watch-mode-hot-reload-check
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::developer_reload_is_atomic_compatible_and_failed_edit_safe -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features quality-tools quality::aot_developer_hot_reload::aot_developer_hot_reload_test
	$(TERLAN_QUALITY) aot-developer-hot-reload

release-flake-detection-check: watch-mode-hot-reload-check
	$(TERLAN_QUALITY) release-flake-detection

release-gate-shard-resume-check: release-flake-detection-check
	$(TERLAN_QUALITY) release-gate-shard-resume

release-gate-duration-budget-check: release-gate-shard-resume-check
	$(TERLAN_QUALITY) release-gate-duration-budget

release-gate-report-schema-check: release-gate-duration-budget-check
	$(TERLAN_QUALITY) release-gate-report-schema

release-failure-reproduction-check: release-gate-report-schema-check
	$(TERLAN_QUALITY) release-failure-reproduction

release-generated-artifacts-freshness-pass:
	$(MAKE) --no-print-directory stdlib-summary-drift-check
	$(MAKE) --no-print-directory stdlib-js-bindings-drift-check
	$(MAKE) --no-print-directory stdlib-native-artifacts-check
	$(MAKE) --no-print-directory stdlib-release-manifest-check
	$(MAKE) --no-print-directory tree-sitter-cli-check

release-generated-artifacts-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-self-test
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-record
	$(MAKE) --no-print-directory release-generated-artifacts-freshness-pass
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-finalize
	rm -f target/quality/release-generated-artifacts-before.json

package-test-exec-check:
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) self-test
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) generate \
		--profile baseline --allow-missing

package-test-exec-check cpp-binding-generator-check generated-package-contract-check cuda-package-availability-check cuda-package-check external-package-execution-matrix-check: | terlan-external-package-matrix-bootstrap

terlan-vm-internal-crate-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools terlan_vm_internal_crate_test
	$(TERLAN_QUALITY) terlan-vm-internal-crate

terlan-vm-external-repo-boundary-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools terlan_vm_external_repo_boundary_test
	$(TERLAN_QUALITY) terlan-vm-external-repo-boundary

native-boundary-terminology-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_boundary_terminology_test
	$(TERLAN_QUALITY) native-boundary-terminology

native-boundary-security-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_boundary_security_test
	$(TERLAN_QUALITY) native-boundary-security

native-binding-generator-contract-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_binding_generator_contract_test
	$(TERLAN_QUALITY) native-binding-generator-contract

cpp-binding-generator-check cpp-package-consumer-check: export RUSTFLAGS := -D warnings

cpp-binding-generator-check: native-binding-generator-contract-check cpp-binding-metadata-extractor-check cpp-package-consumer-check
	$(RUST_TEST) -p terlan --lib cpp_binding_generator
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cpp-binding-generator-check --state passed --reason-code none

.PHONY: generated-package-contract-check
generated-package-contract-check: cpp-binding-generator-check
	test -s target/quality/cpp-binding-generator.gen_report.json
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/GeneratedPackageContractTest.terl
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate generated-package-contract-check --state passed --reason-code none

cpp-binding-metadata-extractor-check:
	target/debug/terlc test \
		scripts/self_validation/CppMetadataExtractorTest.terl \
		--name committed_cpp_metadata_contract_is_valid

cpp-binding-metadata-extractor-live-check: cpp-binding-metadata-extractor-check
	TERLAN_CPP_METADATA_LIVE=1 target/debug/terlc test \
		scripts/self_validation/CppMetadataExtractorTest.terl \
		--name opt_in_live_cpp_metadata_matches_fixture

cpp-binding-build-plan-check: cpp-binding-generator-check

cpp-binding-value-record-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check

cpp-binding-copied-containers-check: cpp-binding-generator-check

cpp-binding-enum-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check

cpp-binding-exception-check: cpp-binding-generator-check

cpp-package-consumer-check:
	$(RUST_TEST) -p terlan --lib generated_cpp_git_package_executes_and_rejects_stale_handles -- --ignored

c-abi-binding-generator-check: native-binding-generator-contract-check
	$(RUST_TEST) -p terlan --lib c_abi_binding_generator

terlan-ndarray-abi-check: export RUSTFLAGS := -D warnings
terlan-ndarray-abi-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" abi-check \
		TERLC="$(abspath target/debug/terlc)"

.PHONY: terlan-ndarray-package-check
terlan-ndarray-package-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" package-check \
		TERLC="$(abspath target/debug/terlc)"

.PHONY: terlan-ndarray-operations-check
terlan-ndarray-operations-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" operations-check \
		TERLC="$(abspath target/debug/terlc)"

terlan-ndarray-blas-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" blas-check \
		TERLC="$(abspath target/debug/terlc)"

ndarray-dlpack-interop-check: c-abi-binding-generator-check
	$(RUST_TEST) -p terlan --lib native_exchange_ -- --nocapture
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" dlpack-check \
		TERLC="$(abspath target/debug/terlc)"

terlan-ndarray-release-check: c-abi-binding-generator-check
	$(RUST_TEST) -p terlan --lib native_exchange_ -- --nocapture
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" release-check \
		TERLC="$(abspath target/debug/terlc)" \
		POLARS="$(abspath $(if $(TERLAN_POLARS_DIR),$(TERLAN_POLARS_DIR),../terlan-polars))" \
		PYTORCH="$(abspath $(if $(TERLAN_PYTORCH_DIR),$(TERLAN_PYTORCH_DIR),../terlan-pytorch))" \
		LIBTORCH="$(abspath $(if $(TERLAN_PYTORCH_LIBTORCH),$(TERLAN_PYTORCH_LIBTORCH),../terlan-pytorch/vendor/libtorch-2.13.0+cpu/libtorch))"

.PHONY: libpq-c-abi-check
libpq-c-abi-check: c-abi-binding-generator-check
	$(CARGO) test -p terlan-libpq --all-targets --offline
	$(CARGO) test -p terlan --lib runtime::native::postgres::libpq::libpq_test --no-default-features --features native-codegen
	$(CARGO) check -p terlan --bin terlan-vm --no-default-features --features native-codegen

cuda-package-availability-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools cuda_package_availability::tests
	$(TERLAN_QUALITY) cuda-package-availability
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cuda-package-availability-check --state passed --reason-code none

cuda-package-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools cuda_package_availability::tests
	$(TERLAN_QUALITY) cuda-package-check
	TERLAN_CUDA_PACKAGE_DIR="$(abspath $(TERLAN_CUDA_DIR))" \
	TERLAN_CUDA_TERLC="$(CURDIR)/target/debug/terlc" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/CudaPackageContractTest.terl
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cuda-package-check --state passed --reason-code none

accelerator-hard-contract-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) accelerator-hard-contract

accelerator-boundary-baseline-check:
	$(MAKE) -C "$(TERLAN_CUDA_DIR)" cuda-baseline-freeze-check \
		TERLC="$(CURDIR)/target/debug/terlc"
	TERLAN_ACCELERATOR_PACKAGE_DIR="$(TERLAN_CUDA_DIR)" \
	TERLAN_ACCELERATOR_BOUNDARY_OUTPUT="$(CURDIR)/target/quality/accelerator-boundary-baseline.json" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/AcceleratorBoundaryBaselineTest.terl

accelerator-package-metadata-check:
	$(RUST_TEST) -p terlan --lib accelerator
	TERLAN_ACCELERATOR_PACKAGE_DIR="$(TERLAN_CUDA_DIR)" \
	TERLAN_COMPILER="$(CURDIR)/target/debug/terlc" \
	TERLAN_ACCELERATOR_METADATA_OUTPUT="$(CURDIR)/target/quality/accelerator-package-metadata.json" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/AcceleratorPackageMetadataTest.terl

accelerator-value-contract-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib accelerator
	target/debug/terlan-accelerator-value-contract \
		"$(CURDIR)/target/quality"
	$(CURDIR)/target/debug/terlc check \
		"$(CURDIR)/target/quality/generated/AcceleratorValue.terl"
	target/debug/terlan-accelerator-value-contract \
		"$(CURDIR)/target/quality/cuda-generated" \
		--module cuda.AcceleratorValue \
		--descriptor "$(TERLAN_CUDA_DIR)/accelerator.toml"
	cmp \
		"$(CURDIR)/target/quality/cuda-generated/cuda/AcceleratorValue.terl" \
		"$(TERLAN_CUDA_DIR)/src/cuda/AcceleratorValue.terl"
	cmp \
		"$(CURDIR)/target/quality/cuda-generated/generated/accelerator_value.rs" \
		"$(TERLAN_CUDA_DIR)/native/rust/src/generated/accelerator_value.rs"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorValueContractTest.terl

accelerator-target-admission-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib accelerator
	target/debug/terlan-accelerator-target-admission \
		"$(CURDIR)/target/quality/accelerator-target-plan.json" \
		"$(TERLAN_CUDA_DIR)/accelerator.toml"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorTargetAdmissionTest.terl

accelerator-ir-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::ir
	target/debug/terlan-accelerator-ir \
		"$(CURDIR)/target/quality/accelerator-ir-report.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorIrTest.terl

accelerator-aot-backend-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::aot
	target/debug/terlan-accelerator-aot-backend \
		"$(CURDIR)/target/quality/accelerator-aot-backend.json" \
		"$${TERLAN_ACCELERATOR_LLC:-/usr/bin/llc}"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorAotBackendTest.terl

accelerator-placement-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::placement
	target/debug/terlan-accelerator-placement \
		"$(CURDIR)/target/quality/accelerator-placement-report.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorPlacementTest.terl

accelerator-fusion-check: accelerator-placement-check

accelerator-transfer-elision-check: accelerator-placement-check

accelerator-vm-integration-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib runtime::vm::accelerator_operation
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::package_native_helper::package_native_helper_test::package_helper_projects_cuda_handles_into_canonical_vm_resources -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_resource_transition_registers_owned_handle_and_cleans_up_on_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_disposes_late_result_resources_with_bounded_credit -- --exact
	target/debug/terlan-accelerator-vm-integration \
		"$(CURDIR)/target/quality/accelerator-vm-integration.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorVmIntegrationTest.terl

accelerator-specialized-artifact-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::assembly
	target/debug/terlan-accelerator-specialized-artifact \
		"$(CURDIR)/target/quality/accelerator-specialized-artifact.json"
	! strings "$(CURDIR)/target/quality/cpu-only.bin" | rg -i \
		'cuda-driver|terlan-cuda|vm-capability-worker-event-pump|\\.ptx|accelerator\\.execute'
	strings "$(CURDIR)/target/quality/accelerator-selected.bin" | rg \
		'cuda-driver|terlan-cuda|vm-capability-worker-event-pump|\\.ptx|accelerator\\.execute'
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorSpecializedArtifactTest.terl

.PHONY: external-package-execution-matrix-check
external-package-execution-matrix-check:
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) self-test
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) generate \
		--profile "$${TERLAN_EXTERNAL_PACKAGE_PROFILE:-baseline}"

native-boundary-runtime-adversarial-check:
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::capability_wire::capability_wire_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib native_worker::protocol::protocol_test::framing_rejects_oversized_input_and_output -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_disposed_handles_through_terms -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_duplicate_dispose_as_stale_handle -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_disposal -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_enforces_postgres_capability_before_adapter_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_cancel_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_completion_wins_cancellation_race_without_request_id_reuse -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_lifecycle_events_write_native_boundary_report -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_lifecycle_event_history_is_bounded -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_timeout_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_duplicate_dispose_returns_stale_handle_and_releases_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact

.PHONY: vm-native-boundary-contract-check
vm-native-boundary-contract-check: lean-proof-track-check
	test -f target/quality/vm-native-boundary-report.json

.PHONY: vm-sql-macro-validation-check
vm-sql-macro-validation-check: vm-db-migration-command-check
	$(CURDIR)/target/debug/terlc test scripts/self_validation/SqlFormBoundaryTest.terl
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features quality-tools sql
	$(TERLAN_QUALITY) vm-sql-macro-validation
	test -s target/quality/vm-sql-macro-validation-report.json

.PHONY: static-route-boundary-check
static-route-boundary-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/StaticRouteBoundaryTest.terl

.PHONY: html-boundary-check
html-boundary-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/HtmlBoundaryTest.terl

.PHONY: vm-db-migration-command-check
vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::db::live_test::run_db_migration_and_snapshot_lifecycle_against_docker_postgres -- --ignored --exact --nocapture
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features quality-tools vm_db_migration_command
	$(TERLAN_QUALITY) vm-db-migration-command
	test -s target/quality/vm-db-migration-report.json

.PHONY: vm-dev-dependency-orchestration-check
vm-dev-dependency-orchestration-check:
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib commands::dev_dependencies
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features quality-tools vm_dev_dependency_orchestration
	$(TERLAN_QUALITY) vm-dev-dependency-orchestration
	test -s target/quality/vm-dev-dependency-report.json

.PHONY: vm-postgres-runtime-check
vm-postgres-runtime-check: vm-sql-macro-validation-check vm-native-boundary-contract-check no-default-tokio-runtime-check libpq-c-abi-check
	test -s target/quality/vm-postgres-runtime-report.json

native-boundary-postgres-baseline-benchmark:
	$(TERLAN_BENCHMARK) native-boundary-postgres-baseline

native-boundary-http-baseline-benchmark:
	$(TERLAN_BENCHMARK) native-boundary-http-baseline

vm-performance-baseline-check: achamp-adversarial-coverage-check
	$(EXACT_CARGO_TEST) -p terlan --lib --features benchmark-tools tests::synthetic_helper_source_contains_requested_workload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features benchmark-tools tests::vm_performance_skipped_tracks_match_required_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features benchmark-tools tests::map_benchmark_tracks_cover_otp_threshold_sizes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features benchmark-tools tests::otp_map_benchmark_eval_uses_native_map_assertions -- --exact
	$(TERLAN_BENCHMARK) vm-performance-baseline

achamp-adversarial-coverage-check: vm-memory-heap-pressure-check
	$(RUST_TEST) -p terlan --lib --features quality-tools achamp_adversarial_coverage_test
	$(TERLAN_QUALITY) achamp-adversarial-coverage

executable-docs-vm-check: | terlan-repository-validation-bootstrap
	$(RUST_TEST) -p terlan --lib --features quality-tools executable_docs_vm_test
	$(TERLAN_QUALITY) executable-docs-vm
	$(EXACT_CARGO_TEST) -p terlan --lib tests::doc_test::readme_hello_world_terlan_block_compiles -- --exact
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
	TERLAN_REPOSITORY_VALIDATION_TERLC="$(CURDIR)/target/debug/terlc" \
		$(TERLAN_REPOSITORY_VALIDATION) readme-hello-world

docs-codeblock-executable-check: executable-docs-vm-check


terlan-vm-compiler-bridge-check: cli-terlan-vm-compiler-bridge-check

terlc-build-executable-check: cli-terlc-build-executable-check

http-runtime-stack-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/HttpRuntimeStackTest.terl
	$(TERLC_EXACT_TEST) commands::serve::serve_test::arguments_and_fixtures::hyper_request_handler_serves_static_get_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::arguments_and_fixtures::hyper_request_handler_serves_static_file_with_query_string -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_omits_static_head_response_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_static_parent_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_unmatched_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_streams_reload_sse_events -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_heads_reload_sse_without_opening_stream -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::static_fallbacks::hyper_request_handler_rejects_reload_sse_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::run_serve_check_rejects_dynamic_handlers_missing_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::observability_and_packages::package_validation_test::run_serve_check_rejects_dynamic_handlers_with_removed_beam_runtime -- --exact

vm-http-vs-axum-check: tvm-http-paired-performance-check
	TERLAN_VM_BENCHMARK_ROOT="$(CURDIR)" \
	TERLAN_VM_BENCHMARK_GATE=vm-http-vs-axum-check \
		target/debug/terlc test scripts/self_validation/VmBenchmarkFamilyPlanTest.terl
	TERLAN_PROTOCOL_BENCHMARK_MODE=validate \
	TERLAN_PROTOCOL_BENCHMARK_ANCHOR=binary_protocol_concurrency_benchmark \
		target/debug/terlc test scripts/benchmarks/protocol/ProtocolBenchmarkTest.terl \
			--bench --warmup 0 --samples 1 --name repository_protocol_benchmark_gate

vm-http-concurrency-investigation-check: \
	terlan-vm-erl-suite-audit-check \
	vm-tcp-stream-check \
	terlan-vm-http-lane-check \
	vm-scheduler-fairness-check \
	vm-http-vs-axum-check | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) vm-http-aot-concurrency-self-test
	$(TERLAN_TVM_PLATFORM_MATRIX) vm-http-aot-concurrency-check

vm-http-benchmark-comparability-check: $(VM_HTTP_BENCHMARK_COMPARABILITY_DEPS)
	$(TERLAN_QUALITY) vm-http-benchmark-comparability

vm-http-runtime-attribution-check: vm-http-benchmark-comparability-check
	$(TERLAN_QUALITY) vm-http-runtime-attribution

vm-http-soak-stability-check: vm-http-runtime-attribution-check vm-timer-deadline-check
	test -s $(HTTP_SOAK_REPORT)

vm-semantics-vs-otp-check: binary-bitstring-processing-check
	TERLAN_VM_BENCHMARK_ROOT="$(CURDIR)" \
	TERLAN_VM_BENCHMARK_GATE=vm-semantics-vs-otp-check \
		target/debug/terlc test scripts/self_validation/VmBenchmarkFamilyPlanTest.terl
	TERLAN_PROTOCOL_BENCHMARK_MODE=validate \
	TERLAN_PROTOCOL_BENCHMARK_ANCHOR=binary_protocol_benchmark \
		target/debug/terlc test scripts/benchmarks/protocol/ProtocolBenchmarkTest.terl \
			--bench --warmup 0 --samples 1 --name repository_protocol_benchmark_gate

runtime-release-dependency-self-test: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) runtime-release-dependency-self-test

angular-ts-terlan-integration-check: angular-ts-namespace-generation-check angular-ts-terlan-app-ownership-check
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=integration \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds

angular-ts-namespace-generation-check:
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=namespace \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds

angular-ts-terlan-app-ownership-check:
	TERLAN_ANGULAR_TS_ROOT="$(CURDIR)" \
	TERLAN_ANGULAR_TS_MODE=app-ownership \
		target/debug/terlc test scripts/self_validation/AngularTsIntegrationTest.terl \
			--name selected_angular_ts_integration_holds



changelog-public-scope-check:
	target/debug/terlc test scripts/self_validation/ChangelogPublicScopeTest.terl

internal-docs-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools internal_docs_test
	$(TERLAN_QUALITY) internal-docs

module-readme-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools module_readmes_test
	$(TERLAN_QUALITY) module-readmes

rustdoc-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools rustdoc_
	$(TERLAN_QUALITY) rust-docs

release-artifact-current: terlan-release-promotion-bootstrap release-boundary-check release-version-metadata-check source-extension-check vm-release-artifact-matrix-check
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) seal $(if $(VERSION),--version "$(VERSION)",)

release-artifact-linux: export TERLAN_RELEASE_OS = Linux
release-artifact-linux: export TERLAN_RELEASE_ARCH = x86_64
release-artifact-linux: release-artifact-current

release-artifact-smoke: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-smoke

release-artifact-installer-smoke: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-installer-smoke

publish-preflight:
	@echo "Preparing Terlan $(VERSION) publication preflight"
	@if [ -n "$$(git status --porcelain)" ]; then \
		changed_count=$$(git status --porcelain | wc -l | tr -d ' '); \
		echo "publish-preflight failed: working tree is not clean"; \
		echo "changed files: $$changed_count"; \
		echo "first changed files:"; \
		git status --short | sed -n '1,20p'; \
		if [ "$$changed_count" -gt 20 ]; then \
			echo "... $$((changed_count - 20)) more changed files omitted"; \
		fi; \
		echo "next step: review and commit the release contents, then rerun make publish"; \
		exit 1; \
	fi
	@command -v gh >/dev/null 2>&1 || { \
		echo "publish requires GitHub CLI: install gh and run gh auth login"; \
		exit 127; \
	}
	@gh auth status >/dev/null 2>&1 || { \
		echo "publish requires authenticated GitHub CLI: run gh auth login"; \
		exit 1; \
	}
	@branch=$$(git branch --show-current); \
	if [ "$$branch" != "main" ]; then \
		echo "publication must run from main; current branch is $$branch"; \
		exit 1; \
	fi
	$(MAKE) --no-print-directory release-version-metadata-check VERSION="$(VERSION)"
	@git fetch --quiet origin main; \
	if ! git merge-base --is-ancestor origin/main HEAD; then \
		echo "origin/main is not an ancestor of HEAD; publication would require a non-fast-forward push"; \
		exit 1; \
	fi
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null \
		&& git ls-remote --exit-code --tags origin "refs/tags/v$(VERSION)" >/dev/null 2>&1; then \
		git fetch --quiet origin "refs/tags/v$(VERSION):refs/tags/v$(VERSION)"; \
	fi
	@if git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		tag_type=$$(git cat-file -t "refs/tags/v$(VERSION)"); \
		if [ "$$tag_type" != tag ]; then \
			echo "local tag v$(VERSION) must be annotated; found $$tag_type"; \
			exit 1; \
		fi; \
		tag_sha=$$(git rev-parse "refs/tags/v$(VERSION)^{commit}"); \
		head_sha=$$(git rev-parse HEAD); \
		if [ "$$tag_sha" != "$$head_sha" ]; then \
			echo "local tag v$(VERSION) already exists at $$tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "local tag v$(VERSION) already exists at HEAD; continuing"; \
	fi
	@if remote_tag_line=$$(git ls-remote --tags origin "refs/tags/v$(VERSION)" 2>/dev/null) && [ -n "$$remote_tag_line" ]; then \
		remote_tag_object_sha=$$(printf '%s\n' "$$remote_tag_line" | awk 'NR == 1 { print $$1 }'); \
		remote_tag_sha=$$(git ls-remote --tags origin "refs/tags/v$(VERSION)^{}" 2>/dev/null | awk 'NR == 1 { print $$1 }'); \
		if [ -z "$$remote_tag_sha" ]; then \
			echo "remote tag v$(VERSION) must be annotated"; \
			exit 1; \
		fi; \
		local_tag_object_sha=$$(git rev-parse "refs/tags/v$(VERSION)"); \
		if [ "$$remote_tag_object_sha" != "$$local_tag_object_sha" ]; then \
			echo "local and remote annotated tag objects differ for v$(VERSION)"; \
			exit 1; \
		fi; \
		head_sha=$$(git rev-parse HEAD); \
		if [ "$$remote_tag_sha" != "$$head_sha" ]; then \
			echo "remote tag v$(VERSION) already exists at $$remote_tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "remote tag v$(VERSION) already exists at HEAD; release upload can be retried"; \
	fi
	bash scripts/download_validated_release_artifacts.sh "$$(git rev-parse HEAD)"
	$(MAKE) --no-print-directory publish-evidence-plan-check
	$(MAKE) --no-print-directory publish-evidence-refresh-plan-check
	@if ! $(MAKE) --no-print-directory publish-evidence-check; then \
		echo '[publish] candidate evidence is absent or stale; refreshing its owners once'; \
		$(MAKE) --no-print-directory publish-evidence-refresh; \
	fi
	$(MAKE) --no-print-directory publish-evidence-check
	$(MAKE) release-boundary-check
	$(MAKE) source-extension-check
	$(MAKE) terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) seal --version "$(VERSION)"
	$(MAKE) release-staged-distribution-verification-refresh
	$(MAKE) release-preflight RELEASE_VERSION="$(VERSION)"

publish-evidence-refresh:
	$(MAKE) --no-print-directory \
		terlan-self-validation-bootstrap \
		terlan-quality-tools-bootstrap \
		terlan-native-worker-bootstrap \
		terlan-benchmark-release-bootstrap \
		terlan-serve-runtime-bootstrap \
		terlan-http-benchmark-release-bootstrap
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
	TERLAN_RELEASE_BINARIES_PREBUILT=1 \
		$(MAKE) --no-print-directory vm-multicore-publish-evidence-refresh
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
	TERLAN_RELEASE_BINARIES_PREBUILT=1 \
		$(MAKE) --no-print-directory tvm-managed-list-profile-benchmark-check
	TERLAN_RUST_SUITE_ALREADY_RUN=1 \
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
	TERLAN_RELEASE_BINARIES_PREBUILT=1 \
		$(MAKE) --no-print-directory tvm-aot-release-closeout-check TERLAN_MULTICORE_CLOSEOUT_ALREADY_RUN=1
	TERLAN_VALIDATION_BOOTSTRAPPED=1 \
	TERLAN_RELEASE_BINARIES_PREBUILT=1 \
		$(MAKE) --no-print-directory release-evidence-compose

publish-evidence-check:
	$(MAKE) --no-print-directory vm-multicore-publish-check
	$(MAKE) --no-print-directory tvm-aot-publish-evidence-check

publish-evidence-plan-check:
	@set -eu; \
	plan=$$(mktemp); \
	trap 'rm -f "$$plan"' EXIT; \
	env -u TERLAN_VALIDATION_BOOTSTRAPPED \
		-u TERLAN_BUILD_ARTIFACTS_PREBUILT \
		-u TERLAN_RUST_SUITE_ALREADY_RUN \
		-u TERLAN_RELEASE_BINARIES_PREBUILT \
		$(MAKE) --no-print-directory -n publish-evidence-check >"$$plan"; \
	if rg -n '(^|[[:space:]])cargo([[:space:]]|$$)|run_exact_cargo_test|terlc (test|build)' "$$plan"; then \
		echo 'error[publish.evidence.plan]: verification replays build or test work' >&2; \
		exit 1; \
	fi; \
	echo '[publish-evidence-plan] zero build and test replays'

publish-evidence-refresh-plan-check:
	@set -eu; \
	plan=$$(mktemp); \
	trap 'rm -f "$$plan"' EXIT; \
	env -u TERLAN_VALIDATION_BOOTSTRAPPED \
		-u TERLAN_BUILD_ARTIFACTS_PREBUILT \
		-u TERLAN_RUST_SUITE_ALREADY_RUN \
		-u TERLAN_RELEASE_BINARIES_PREBUILT \
		$(MAKE) --no-print-directory -Bn publish-evidence-refresh >"$$plan"; \
	cargo_count=$$(rg -c '^cargo --locked ' "$$plan" || true); \
	exact_count=$$(rg -c 'run_exact_cargo_test' "$$plan" || true); \
	duplicate_cargo=$$(rg '^cargo --locked ' "$$plan" | sort | uniq -d || true); \
	if test "$$cargo_count" -gt 6; then \
		echo "error[publish.evidence.refresh_plan]: $$cargo_count Cargo invocations exceed the six-invocation budget" >&2; \
		exit 1; \
	fi; \
	if test "$$exact_count" -gt 2; then \
		echo "error[publish.evidence.refresh_plan]: $$exact_count exact Cargo selectors exceed the two isolated-benchmark budget" >&2; \
		exit 1; \
	fi; \
	if test -n "$$duplicate_cargo"; then \
		echo 'error[publish.evidence.refresh_plan]: duplicate equivalent Cargo build:' >&2; \
		echo "$$duplicate_cargo" >&2; \
		exit 1; \
	fi; \
	echo "[publish-evidence-refresh-plan] cargo=$$cargo_count exact-isolated=$$exact_count duplicate-builds=0"

publish: publish-preflight
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		git tag --annotate "v$(VERSION)" --message "Terlan v$(VERSION)"; \
	fi
	git push origin main
	git push origin "v$(VERSION)"
	$(MAKE) publish-release-from-dist VERSION=$(VERSION)

publish-release-from-dist:
	bash scripts/publish_release_from_dist.sh "$(VERSION)"

release-promotion-pipeline-check: terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) self-test --report
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) contract

release-promotion-dry-run: terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) dry-run $(if $(VERSION),--version "$(VERSION)",)

clean: cli-clean

.PHONY: web-service-foundation-contract-check web-service-foundation-runtime-check web-service-foundation-adapter-check web-service-foundation-pruning-check web-service-foundation-check

web-service-foundation-contract-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan-service-foundation
	target/debug/terlc check std/service/ServiceFoundationTest.terl
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- contract

web-service-foundation-runtime-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan --lib service_foundation::tests
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- runtime

web-service-foundation-adapter-check: terlan-compiler-bootstrap
	$(CARGO) test -p terlan-foundations-adapter
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- adapter

web-service-foundation-pruning-check: terlan-compiler-bootstrap
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- pruning

web-service-foundation-check: web-service-foundation-contract-check web-service-foundation-runtime-check web-service-foundation-adapter-check web-service-foundation-pruning-check
	target/debug/terlc run scripts/self_validation/WebServiceFoundation.terls -- report
