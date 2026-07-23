CARGO := cargo
RUST_TEST := $(CARGO) test
EXACT_CARGO_TEST := bash scripts/run_exact_cargo_test.sh
PYTHON := python3 -B
TERLAN_NDARRAY_DIR ?= ../terlan-ndarray
TERLAN_POLARS_DIR ?=
TERLAN_POLARS_SOURCE ?=
TERLAN_POLARS_REV ?=
TERLAN_POLARS_CACHE_DIR ?= $(CURDIR)/target/package-cache/terlan-polars
TERLAN_PYTORCH_DIR ?=
TERLAN_PYTORCH_REPOSITORY ?= https://github.com/terlan-lang/terlan-pytorch.git
TERLAN_PYTORCH_REV ?= c400aee030e1249d4e50ea8f5e7da50b719bccea
TERLAN_PYTORCH_LIBTORCH ?=
export PYTHONDONTWRITEBYTECODE := 1
export PYTHONPYCACHEPREFIX := /tmp/terlan-python-cache
export TERLAN_POLARS_DIR TERLAN_POLARS_SOURCE TERLAN_POLARS_REV TERLAN_POLARS_CACHE_DIR
SHELL := bash
.SHELLFLAGS := -eo pipefail -c

.PHONY: tvm-native-image-format-check tvm-direct-aot-backend-check tvm-aot-application-closure-check tvm-aot-case-lowering-check tvm-aot-higher-order-specialization-check tvm-aot-lowering-coverage-check tvm-aot-managed-continuation-check tvm-aot-owned-closure-representation-check tvm-aot-static-callable-check tvm-aot-thread-neutral-continuation-check tvm-aot-typed-lifecycle-check tvm-aot-typed-mailbox-check tvm-managed-memory-check tvm-native-image-loader-check tvm-aot-consumer-check tvm-aot-test-consumer-check tvm-aot-repl-consumer-check tvm-aot-debugger-consumer-check tvm-aot-hot-reload-consumer-check tvm-aot-package-install-consumer-check tvm-aot-support-crash-metadata-check tvm-aot-platform-target-check tvm-aot-platform-matrix-check tvm-aot-http-managed-cycle-check tvm-aot-http-request-accessor-check tvm-aot-http-response-mutation-check tvm-aot-http-typed-metadata-check tvm-aot-http-router-callable-check tvm-aot-http-managed-error-check tvm-aot-http-template-check tvm-aot-http-template-render-plan-check tvm-aot-http-template-expression-check tvm-aot-http-body-json-check tvm-aot-http-session-check tvm-aot-http-managed-boundary-check tvm-aot-http-channel-plan-check tvm-aot-http-persistent-shard-check tvm-aot-http-native-invocation-check tvm-aot-http-websocket-invocation-check tvm-aot-http-sse-invocation-check tvm-aot-http-generation-lifetime-check tvm-aot-http-channel-transport-check tvm-aot-http-cleanup-check tvm-aot-http-lifecycle-inventory-check tvm-aot-http-checked-coreir-reference-record tvm-aot-http-performance-check tvm-single-image-artifact-check tvm-aot-runtime-transition-check tvm-aot-runtime-transition-focused-check tvm-aot-compilation-benchmark-check tvm-aot-compilation-time-check tvm-aot-capability-worker-check
.PHONY: tvm-aot-application-conformance-check tvm-aot-c-abi-boundary-check tvm-aot-closure-dispatch-check tvm-aot-crash-injection-check tvm-aot-image-lifetime-check tvm-aot-multicore-readiness-check tvm-aot-thread-sanitizer-check
.PHONY: std-vm-parity-matrix-check vm-distribution-suite-parity-check vm-multicore-invariant-inventory-check
.PHONY: tvm-aot-managed-field-projection-check tvm-aot-platform-matrix-contract-check tvm-aot-thread-sanitizer-contract-check tvm-aot-roadmap-reconciliation-check tvm-aot-release-closeout-contract-check tvm-aot-release-closeout-check
.PHONY: no-tvm-json-runtime-check no-vmir-interpreter-check runtime-aot-only-check
.PHONY: vm-debug-key-compatibility-check
.PHONY: vm-latin1-source-policy-check
.PHONY: vm-compiler-transform-retirement-check
.PHONY: vm-source-column-ownership-check
.PHONY: vm-source-provenance-artifact-check
.PHONY: vm-call-dependency-artifact-check
.PHONY: vm-executable-source-span-artifact-check
.PHONY: terlan-pytorch-package-check
.PHONY: terlan-ndarray-abi-check terlan-ndarray-operations-check
.PHONY: vm-source-hot-reload-check
.PHONY: vm-distributed-scheduling-check
.PHONY: vm-distributed-state-check
.PHONY: vm-supervision-restart-check
.PHONY: vm-timer-deadline-check
.PHONY: vm-scheduler-fairness-check vm-actor-mutator-ownership-check vm-multicore-mailbox-publication-check vm-multicore-fixed-placement-check tvm-aot-multicore-migration-check vm-multicore-work-stealing-policy-check vm-multicore-work-stealing-check vm-multicore-runtime-cleanup-check tvm-aot-multicore-io-epoch-check vm-multicore-timer-epoch-check vm-multicore-timer-scheduler-check vm-multicore-protocol-reactor-check vm-multicore-capability-worker-check vm-multicore-capability-completion-check vm-multicore-capability-event-pump-check vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-multicore-runtime-integration-check vm-multicore-replay-observability-check vm-multicore-performance-check vm-multicore-memory-model-check vm-multicore-thread-sanitizer-contract-check vm-multicore-thread-sanitizer-check vm-multicore-mc9-evidence-contract-check vm-multicore-mc9-evidence-check vm-multicore-release-contract-check vm-multicore-release-check
.PHONY: tvm-http-axum-performance-record
.PHONY: vm-memory-heap-pressure-check
.PHONY: std-generated-metadata-check
.PHONY: std-test-honesty-check
.PHONY: std-test-table-check
.PHONY: std-test-property-check
.PHONY: std-range-check
.PHONY: std-random-check
.PHONY: std-regex-check
.PHONY: rust-build-feature-shipping-check
.PHONY: mobile-boundary-check
.PHONY: language-feature-coverage-100-check operator-coverage-100-check pattern-matching-support-check string-pattern-matching-check string-pattern-long-tail-check binary-bitstring-processing-check binary-syntax-scaffold-check binary-runtime-suite-check binary-descriptor-check binary-descriptor-contract-check binary-error-taxonomy-check binary-protocol-helper-check binary-protocol-benchmark-check
.PHONY: core-type-contracts-check
.PHONY: type-alias-shorthand-check
.PHONY: compiler-purity-metadata-check
.PHONY: comprehension-guards-check
.PHONY: lean-proof-track-check lean-proof-track-gap-hygiene-check lean-proof-feature-cull-check proof-repro-check proof_repro_check lean-proof-track-pr-gate lean-proof-track-regression-check lean-proof-track-runtime-check lean-proof-track-release-closeout-check release-0-0-7-preflight
.PHONY: function-head-pattern-parameters-check function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-migration-docs-check function-head-pattern-0-0-7-handoff-check function-head-pattern-parameters-hardening-check
.PHONY: syntax-contract-check shape-implications-check
.PHONY: shape-synonyms-check
.PHONY: wasm-coreir-lowering-check wasm-runtime-exec-check wasm-contract-discovery-check
.PHONY: flexible-shape-guards-check
.PHONY: battleship-external-vm-contract-check
.PHONY: roadmap-legacy-runtime-cleanup-check
.PHONY: roadmap-gate-integrity-check
.PHONY: callable-syntax-cleanup-check
.PHONY: release-version-channel-check release-version-bump
.PHONY: editor-definition-navigation-check editor-code-action-auto-import-check
.PHONY: terlan-lint-style-profile-check terlan-lint-pipe-canonicalization-check
.PHONY: upgrade-local update-terlc
.PHONY: dormant-runtime-code-check
.PHONY: vm-http-stack-check vm-http-in-memory-transport-check vm-in-memory-stream-check vm-tcp-framing-check vm-http-static-streaming-check vm-http-concurrency-hot-reload-check
.PHONY: vm-http-router-middleware-check vm-http-sse-check vm-http-websocket-source-check vm-http-websocket-upgrade-check vm-http-websocket-queue-check vm-http-websocket-policy-check vm-http-websocket-tls-check vm-http-websocket-termination-check vm-http-live-channel-source-check
.PHONY: vm-native-worker-runtime-check vm-io-reactor-runtime-check
.PHONY: vm-http-handler-scheduler-fairness-check vm-http-stateful-actor-session-check vm-live-template-stream-check vm-live-template-client-protocol-check
.PHONY: cpp-binding-metadata-extractor-check cpp-binding-metadata-extractor-live-check cpp-binding-build-plan-check cpp-binding-value-record-check cpp-binding-copied-containers-check cpp-binding-enum-check cpp-binding-exception-check cpp-package-consumer-check
.PHONY: vm-http-benchmark-comparability-check vm-http-runtime-attribution-check vm-http-soak-stability-check
.PHONY: vm-otp-abstractions-terlan-stdlib-check

ifeq ($(TERLAN_RUST_SUITE_ALREADY_RUN),1)
RUST_TEST := @true
EXACT_CARGO_TEST := @true
CANONICAL_RUST_SUITE_OWNER :=
else
CANONICAL_RUST_SUITE_OWNER := rust-test-suite
endif

include crates/terlan/cli.mk
include std/stdlib.mk
include editors/editor.mk

# Completed slices no longer own bespoke Cargo invocations. Their focused
# repository checks remain below, while Rust coverage is owned by one suite.
COMPLETED_SLICE_RUST_GATES := \
	compiler-incremental-cache-check \
	compiler-purity-metadata-check \
	core-type-contracts-check \
	device-target-planner-check \
	editor-code-action-auto-import-check \
	editor-completion-signature-check \
	editor-definition-navigation-check \
	function-head-migration-diagnostic-policy-check \
	function-head-migration-lint-check \
	function-head-pattern-0-0-7-handoff-check \
	function-head-pattern-migration-assist-check \
	function-head-pattern-migration-benchmark-check \
	function-head-pattern-migration-docs-check \
	function-head-pattern-parameters-check \
	native-no-std-target-feasibility-check \
	package-api-compatibility-check \
	package-build-artifact-isolation-check \
	package-cache-integrity-check \
	package-capability-contract-check \
	package-cli-workflow-check \
	package-editor-integration-check \
	package-registry-publish-check \
	package-release-test-matrix-check \
	package-resolver-reproducibility-check \
	package-test-exec-check \
	package-workspace-graph-check \
	release-code-hygiene-check \
	release-failure-reproduction-check \
	release-flake-detection-check \
	release-gate-duration-budget-check \
	release-gate-report-schema-check \
	release-gate-shard-resume-check \
	repeated-let-syntax-check \
	source-map-debug-info-check \
	string-pattern-long-tail-check \
	terlan-vm-http-lane-check \
	typed-template-interpolation-tooling-check \
	typed-template-render-mode-check \
	vm-distributed-scheduling-check \
	vm-distributed-transport-check \
	vm-distributed-state-check \
	vm-http-acme-cache-custody-check \
	vm-http-acme-renewal-rotation-check \
	vm-http-acme-worker-migration-check \
	vm-http-handler-dispatch-check \
	vm-http-handler-scheduler-fairness-check \
	vm-http-runtime-attribution-check \
	vm-http-soak-stability-check \
	vm-http-stateful-actor-session-check \
	vm-io-reactor-runtime-check \
	vm-live-template-client-protocol-check \
	vm-live-template-stream-check \
	vm-memory-heap-pressure-check \
	vm-model-sync-store-check \
	vm-native-boundary-contract-check \
	vm-native-worker-runtime-check \
	vm-persistent-actor-adapter-conformance-check \
	vm-persistent-actor-compaction-check \
	vm-persistent-actor-performance-budget-check \
	vm-persistent-actor-policy-check \
	vm-persistent-actor-restore-check \
	vm-persistent-actor-schema-check \
	vm-persistent-actor-store-check \
	vm-persistent-actor-telemetry-check \
	vm-postgres-runtime-check \
	vm-timer-deadline-check \
	vm-web-config-secret-boundary-check \
	vm-web-deployment-profile-check \
	vm-web-lifecycle-health-check \
	vm-web-observability-check \
	vm-web-route-schema-client-check \
	vm-web-security-policy-check \
	wasm-contract-discovery-check \
	wasm-coreir-lowering-check \
	wasm-runtime-exec-check \
	watch-mode-hot-reload-check \
	web-asset-pipeline-check

.PHONY: $(COMPLETED_SLICE_RUST_GATES)
# Paused during the hard AOT cutover: completed non-AOT slices no longer pull
# the canonical workspace suite into focused native-image checks.
# $(COMPLETED_SLICE_RUST_GATES): $(CANONICAL_RUST_SUITE_OWNER)

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
ifndef VERSION
$(error VERSION is required. Use: make $(firstword $(MAKECMDGOALS)) VERSION=<release-version>)
endif
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

# CHECK_GATES := \
# 	release-boundary-check \
# 	single-root-contract-check \
# 	diff-whitespace-check \
# 	workspace-version-check \
# 	release-version-metadata-check \
# 	source-extension-check \
# 	rust-warnings-check \
# 	rust-quality-check \
# 	safe-rust-runtime-check \
# 	test-hierarchy-check \
# 	dev-fast-feedback-profile-check \
# 	std-source-naming-check \
# 	std-generated-metadata-check \
# 	std-test-honesty-check \
# 	js-type-emission-contract-check \
# 	callable-syntax-cleanup-check \
# 	value-lifecycle-contract-check \
# 	terlan-lint-pipe-canonicalization-check \
# 	core-typing-spec-check \
# 	core-type-contracts-check \
# 	type-alias-shorthand-check \
# 	target-inference-contract-check \
# 	target-inference-default-vm-check \
# 	shared-helper-check \
# 	installer-contract-check \
# 	rust-build-feature-shipping-check \
# 	oxc-boundary-check \
# 	no-terlan-vm-erts-rust-dependency-check \
# 	terlc-build-executable-check \
# 	terlan-vm-run-command-check \
# 	terlan-vm-http-lane-check \
# 	vm-runtime-semantics-check \
# 	vm-diagnostics-quality-check \
# 	vm-performance-baseline-check \
# 	executable-docs-vm-check \
# 	compiler-purity-metadata-check \
# 	comprehension-guards-check \
# 	lean-proof-track-check \
# 	function-head-pattern-parameters-hardening-check \
# 	string-pattern-matching-check \
# 	string-pattern-long-tail-check \
# 	binary-bitstring-processing-check \
# 	shape-implications-check \
# 	language-feature-coverage-100-check \
# 	shape-synonyms-check \
# 	wasm-coreir-lowering-check \
# 	wasm-runtime-exec-check \
# 	wasm-contract-discovery-check \
# 	all-terlan-tests-vm-check \
# 	std-package-coverage-100-check \
# 	terlc-doctor-vm-pivot-check \
# 	mobile-boundary-check \
# 	mobile-target-diagnostic-check \
# 	mobile-shell-profile-check \
# 	mobile-bridge-typecheck \
# 	mobile-bridge-runtime-check \
# 	mobile-reactive-process-check \
# 	mobile-android-shell-smoke \
# 	mobile-ios-shell-smoke \
# 	no-default-beam-runtime-check \
# 	no-default-tokio-runtime-check \
# 	otp-runtime-exit-check \
# 	terlan-vm-erl-suite-audit-check \
# 	roadmap-legacy-runtime-cleanup-check \
# 	roadmap-gate-integrity-check \
# 	std-vm-surface-classification-check \
# 	vm-otp-abstractions-terlan-stdlib-check \
# 	vm-ownership-classification-check \
# 	vm-runtime-concept-inventory-check \
# 	tvm-aot-pivot-inventory-check \
# 	terlan-runtime-conformance-check \
# 	release-failure-reproduction-check \
# 	package-test-exec-check \
# 	terlan-vm-internal-crate-check \
# 	native-boundary-terminology-check \
# 	native-boundary-security-check \
# 	cpp-binding-generator-check \
# 	c-abi-binding-generator-check \
# 	terlan-polars-package-check \
# 	libpq-c-abi-check \
# 	cuda-package-availability-check \
# 	native-boundary-runtime-adversarial-check \
# 	release-hardening-check \
# 	http-tls-check \
# 	http-runtime-stack-check \
# 	typed-template-interpolation-tooling-check \
# 	typed-template-interpolation-backend-check \
# 	vm-http-concurrency-investigation-check \
# 	vm-http-vs-axum-check \
# 	vm-http-benchmark-comparability-check \
# 	vm-http-runtime-attribution-check \
# 	vm-http-soak-stability-check \
# 	vm-semantics-vs-otp-check \
# 	function-language-surface-check \
# 	runtime-release-dependency-self-test \
# 	angular-ts-terlan-app-ownership-check \
# 	angular-ts-terlan-integration-check \
# 	angular-ts-namespace-generation-check \
# 	changelog-public-scope-check \
# 	internal-docs-check \
# 	module-readme-check \
# 	rustdoc-check \
# 	cli-check \
# 	stdlib-check \
# 	tree-sitter-cli-check \
# 	editor-check \
# 	api-schema-check \
# 	validate-ebnf
#
CHECK_GATES := \
	runtime-aot-only-check \
	terlan-vm-artifact-format-check \
	tvm-native-image-format-check \
	tvm-direct-aot-backend-check \
	tvm-aot-application-closure-check \
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
	no-tvm-json-runtime-check \
	no-vmir-interpreter-check

check: check-gates

check-gates: $(CHECK_GATES)

.PHONY: value-lifecycle-contract-check
value-lifecycle-contract-check:
	$(RUST_TEST) -p terlan --bin terlc value_lifecycle_test -- --nocapture
	$(RUST_TEST) -p terlan --bin terlc const_eval_test -- --nocapture
	$(RUST_TEST) -p terlan --bin terlc expression_macro -- --nocapture
	$(RUST_TEST) -p terlan --bin terlc value_lifecycle_ -- --nocapture
	$(RUST_TEST) -p terlan --bin terlan-vm value_lifecycle_test -- --nocapture
	$(RUST_TEST) -p terlan --bin terlan-vm const_eval_test -- --nocapture
	$(RUST_TEST) -p terlan --bin terlan-vm match_assertion_is_structured_and_transactional -- --nocapture
	$(RUST_TEST) -p terlan --bin terlc repl_rejects_local_constants_but_accepts_constant_imports -- --nocapture
	$(RUST_TEST) -p terlan --bin terlan-lsp --features editor-lsp completion_items_include_local_and_imported_shapes_and_functions -- --nocapture
	$(RUST_TEST) -p terlan --bin terlan-lsp --features editor-lsp value_lifecycle_ -- --nocapture
	$(TERLC) test tests/language/ValueLifecycleTest.terl --target terlan-vm
	$(TERLC) --target-profile js.shared test tests/language/ValueLifecycleTest.terl --target js
	$(MAKE) --no-print-directory validate-ebnf tree-sitter-cli-check editor-check

test: cli-test

rust-test-suite:
	@if [ "$(TERLAN_RUST_SUITE_ALREADY_RUN)" = "1" ]; then \
		echo "[rust-test-suite] canonical owned Rust suite already passed."; \
	else \
		$(CARGO) build --locked --bin terlc --bin terlan-vm --bin terlan-native-worker --bin terlan-test-orchestrator; \
		target/debug/terlan-test-orchestrator; \
	fi

test-release: cli-test-release terlan-release-train-check $(TEST_RELEASE_STDLIB_TARGET)

dev-check:
	$(MAKE) rust-warnings-check
	$(MAKE) std-test-honesty-check
	$(MAKE) terlan-lint-style-profile-check
	$(MAKE) cli-exact-selector-check

dev-vm-check:
	$(MAKE) terlan-vm-run-command-check
	$(MAKE) vm-diagnostics-quality-check
	$(MAKE) vm-runtime-concept-inventory-check

dev-web-check:
	$(MAKE) tree-sitter-cli-check
	$(MAKE) editor-debugger-surface-check
	$(MAKE) angular-ts-namespace-generation-check

build:
	$(MAKE) cli-build

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

validate-ebnf:
	$(PYTHON) tools/validate_ebnf.py --strict

workspace-version-check:
	bash scripts/check_workspace_version_inheritance.sh

release-version-metadata-check:
	bash scripts/check_release_version_metadata.sh

release-version-channel-check:
	$(PYTHON) -m unittest scripts/check_release_version_channel_test.py
	$(PYTHON) scripts/check_release_version_channel.py --channel dev

release-version-bump:
	@test -n "$(VERSION)" || (echo "VERSION is required" >&2; exit 1)
	$(PYTHON) scripts/check_release_version_channel.py "$(VERSION)" --write --channel dev

source-extension-check:
	bash scripts/check_terlan_source_extensions.sh

release-boundary-check:
	bash scripts/check_release_boundary.sh

single-root-contract-check:
	$(PYTHON) tools/check_single_root_contract.py

diff-whitespace-check:
	git diff --check

rust-warnings-check:
	RUSTFLAGS='-D warnings' $(CARGO) check --locked -p terlan --bins

rust-quality-check: dormant-runtime-code-check vm-deterministic-hashmap-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-quality

dormant-runtime-code-check:
	$(RUST_TEST) -p terlan --bin terlan-quality dormant_runtime_code_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- dormant-runtime-code

vm-deterministic-hashmap-check:
	$(RUST_TEST) -p terlan --bin terlan-quality vm_deterministic_hashmap_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-deterministic-hashmap

safe-rust-runtime-check:
	$(PYTHON) tools/check_safe_rust_runtime.py

test-hierarchy-check:
	$(RUST_TEST) -p terlan --bin terlan-quality test_hierarchy_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- test-hierarchy

dev-fast-feedback-profile-check:
	$(RUST_TEST) -p terlan --bin terlan-quality dev_fast_feedback_profile_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- dev-fast-feedback-profile

std-source-naming-check:
	$(RUST_TEST) -p terlan --bin terlan-quality std_source_naming_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- std-source-naming

std-generated-metadata-check:
	$(RUST_TEST) -p terlan --bin terlan-quality std_generated_metadata_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- std-generated-metadata

cli-exact-selector-check:
	$(RUST_TEST) -p terlan --bin terlan-quality cli_exact_selectors_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- cli-exact-selectors

core-typing-spec-check:
	$(RUST_TEST) -p terlan --bin terlan-quality core_typing_spec_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- core-typing-spec

target-inference-contract-check:
	$(RUST_TEST) -p terlan --bin terlc target_profile_inference
	$(RUST_TEST) -p terlan --bin terlc accepts_asset_import_resolution_for_browser_target_profile
	$(RUST_TEST) -p terlan --bin terlc build_command_infers_js_browser_target_from_asset_imports
	$(RUST_TEST) -p terlan --bin terlc build_command_rejects_explicit_vm_target_for_js_evidence
	$(RUST_TEST) -p terlan --bin terlc run_check_single_file_infers_js_shared_profile_from_js_import
	$(RUST_TEST) -p terlan --bin terlc run_check_single_file_rejects_explicit_core_v0_profile_for_js_import
	$(RUST_TEST) -p terlan --bin terlc run_check_dir_infers_js_shared_profile_from_js_import
	$(RUST_TEST) -p terlan --bin terlc run_check_dir_rejects_explicit_core_v0_profile_for_js_import
	$(RUST_TEST) -p terlan --bin terlc run_check_dir_rejects_map_for_core_v0_target_profile
	$(RUST_TEST) -p terlan --bin terlc run_command_rejects_js_target_evidence_before_build
	$(RUST_TEST) -p terlan --bin terlc run_command_rejects_explicit_js_profile_for_vm_source
	$(RUST_TEST) -p terlan --bin terlc repl_seed_target_inference_rejects_js_source_evidence
	$(RUST_TEST) -p terlan --bin terlc repl_seed_target_inference_rejects_explicit_js_profile_for_vm_source
	$(RUST_TEST) -p terlan --bin terlc validate_web_package_rejects_non_browser_target_profile

.PHONY: target-inference-default-vm-check
target-inference-default-vm-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc target_profile_inference
	$(EXACT_CARGO_TEST) -p terlan --bin terlc parse_build_args_defaults_to_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc parse_build_args_rejects_explicit_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_defaults_to_terlan_vm_artifact_without_erlang_or_beam
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_defaults_project_directory_to_terlan_vm_artifacts
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_infers_js_browser_target_from_asset_imports
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_rejects_explicit_vm_target_for_js_evidence
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_infers_wasm_core_target_from_i32_abi_import
	$(EXACT_CARGO_TEST) -p terlan --bin terlc package_metadata_defaults_to_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc validate_run_args_defaults_to_vm_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc validate_run_args_accepts_vm_and_rejects_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc build_command_for_run_appends_default_vm_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc run_command_rejects_js_target_evidence_before_build
	$(EXACT_CARGO_TEST) -p terlan --bin terlc run_command_rejects_explicit_js_profile_for_vm_source
	$(EXACT_CARGO_TEST) -p terlan --bin terlc parse_test_args_accepts_default_terlan_vm_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc parse_test_args_rejects_explicit_erlang_target
	$(EXACT_CARGO_TEST) -p terlan --bin terlc run_test_defaults_to_terlan_vm_execution
	$(EXACT_CARGO_TEST) -p terlan --bin terlc repl_runtime_selects_effective_target_profile
	$(EXACT_CARGO_TEST) -p terlan --bin terlc repl_seed_target_inference_rejects_js_source_evidence
	$(EXACT_CARGO_TEST) -p terlan --bin terlc repl_seed_target_inference_rejects_explicit_js_profile_for_vm_source

shared-helper-check:
	$(PYTHON) tools/check_shared_helpers.py --self-test
	$(PYTHON) tools/check_shared_helpers.py

release-code-hygiene-check: \
	rust-warnings-check \
	rust-quality-check \
	shared-helper-check \
	terlan-lint-pipe-canonicalization-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-code-hygiene


installer-contract-check:
	$(PYTHON) tools/check_installer_contract.py

vm-release-install-validation-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-release-install-validation

vm-release-artifact-matrix-check: \
	vm-release-install-validation-check \
	cli-release-artifact-current \
	release-artifact-smoke \
	release-artifact-installer-smoke
	$(PYTHON) tools/check_release_artifact_matrix.py



rust-build-feature-shipping-check:
	$(RUST_TEST) -p terlan --bin terlan-quality rust_build_feature_shipping_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-build-feature-shipping

mobile-boundary-check:
	$(RUST_TEST) -p terlan --bin terlan-quality mobile_boundary_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- mobile-boundary

language-feature-coverage-100-check: comprehension-guards-check shape-implications-check
	$(RUST_TEST) -p terlan --bin terlan-quality language_feature_coverage_100_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- language-feature-coverage-100
	$(TERLC) test tests/language/LanguageFeatureCoverageTest.terl

operator-coverage-100-check:
	$(RUST_TEST) -p terlan --bin terlan-quality operator_coverage_100_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- operator-coverage-100
	$(TERLC) test tests/operator/OperatorCoverageTest.terl
	$(TERLC) test tests/comparison/ComparisonTest.terl

pattern-matching-support-check:
	$(RUST_TEST) -p terlan --bin terlan-quality pattern_matching_support_test
	$(TERLC_EXACT_TEST) --bin terlc runtime::vm::record_test -- --nocapture
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- pattern-matching-support
	$(TERLC) test tests/pattern/PatternMatchingTest.terl

string-pattern-matching-check:
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_string_capture_pattern_in_case_clause -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_string_capture_pattern_in_let_binding -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_string_capture_pattern_in_function_clause -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_string_capture_pattern_in_lambda_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_elixir_style_string_capture_pattern -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_adjacent_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_unterminated_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_empty_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_string_capture_pattern_in_nested_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::syntax_output_pattern_test::tests::syntax_output_marks_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_binds_explicit_type -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_defaults_untyped_capture_to_string -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_rejects_duplicate_capture_names -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_rejects_invalid_capture_annotation -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::core_lowering_test::syntax_output_lowering_to_core_pattern_coverage_includes_string_capture_payload -- --exact

string-pattern-long-tail-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- pattern-matching-support
	@tmp_home="$$(mktemp -d)"; trap 'rm -rf "$$tmp_home"' EXIT; HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check && HOME="$$tmp_home" npm --prefix tree-sitter-terlan run check:cli
	$(TERLC) test tests/pattern/StringPatternLongTailTest.terl

binary-bitstring-processing-check: binary-descriptor-check binary-syntax-scaffold-check binary-error-taxonomy-check binary-protocol-helper-check binary-protocol-benchmark-check vm-tcp-framing-check

binary-syntax-scaffold-check: tree-sitter-package-check tree-sitter-cli-check
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_binary_layout_expression_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::expands_binary_layout_shape_captures_without_rewriting_descriptors -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_structural_arguments_for_binary_layout_shape_captures -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::test::test_shape_import_test::project_vm_tests_execute_imported_binary_layout_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_binary_layout_function_head_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_binary_layout_case_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_binary_layout_lambda_pattern_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_binary_layout_unknown_endian_policy -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_binary_layout_duplicate_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_binary_layout_non_terminal_rest -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_binary_layout_multiple_rest_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_empty_binary_layout_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_binary_layout_unknown_descriptor -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::parses_binary_layout_unicode_scalar_descriptors -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::formal_binary_segments_are_rejected_as_erlang_source_syntax -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_splits_long_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::syntax_output_expr_test::tests::syntax_output_includes_binary_layout_scaffold -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_accepts_fixed_integer_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_accepts_exact_bytes_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_accepts_exact_bits_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_accepts_terminal_rest_binary_layout_constructor -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_accepts_unicode_binary_layout_constructors -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_non_integer_unicode_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_non_integer_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_non_bytes_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_non_bitstring_binary_layout_constructor_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_non_bytes_terminal_rest_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_oversized_binary_layout_integer_width -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_oversized_binary_layout_byte_width -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_rejects_unbound_binary_layout_field_value -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_fixed_integer_binary_layout -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_exact_bytes_binary_layout -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_exact_bits_binary_layout -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_terminal_rest_binary_layout -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::binary_layout_test::syntax_output_lowering_to_core_builds_unicode_binary_layouts -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_accepts_typed_binary_layout_case_pattern -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_accepts_typed_binary_layout_function_head_pattern -- --exact
	$(TERLC_EXACT_TEST) --bin terlc validation::target_profile::target_profile_test::tests::binary_pattern_test::target_profile_allows_binary_pattern_for_vm_profile -- --exact
	$(TERLC_EXACT_TEST) --bin terlc validation::target_profile::target_profile_test::tests::binary_pattern_test::target_profile_rejects_binary_pattern_for_js_profiles -- --exact
	$(TERLC) test tests/binary/BinaryConstructionTest.terl
	$(TERLC) test tests/binary/BinaryPatternTest.terl
	$(TERLC) test tests/binary/BinaryPropertyTest.terl

	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/binary/Binary.terl
	target/debug/terlc check std/binary/BinaryTest.terl
	target/debug/terlc test std/binary
	target/debug/terlc test tests/binary/BinaryDynamicSizeTest.terl
	target/debug/terlc check std/vm/BitString.terl
	target/debug/terlc test std/vm/BitStringTest.terl
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::core_intrinsic_test::vm_bitstring_intrinsics_have_closed_ids_and_return_types -- --exact
	$(TERLC_EXACT_TEST) --bin terlc runtime::vm::bitstring::bitstring_test -- --nocapture
	$(TERLC_EXACT_TEST) --bin terlc runtime::vm::memory::memory_test::memory_logical_value_size_accounts_nested_structural_values_exactly -- --exact
	$(TERLC_EXACT_TEST) --bin terlc runtime::vm::term_format::term_format_test::tetf_encodes_bitstrings_with_exact_logical_length -- --exact

binary-descriptor-check: binary-descriptor-contract-check binary-runtime-suite-check

binary-descriptor-contract-check:
	$(CARGO) run -p terlan --bin terlan-quality -- binary-descriptor-contract

binary-error-taxonomy-check: binary-runtime-suite-check

binary-protocol-helper-check: binary-runtime-suite-check
	target/debug/terlc test tests/pattern/PatternMatchingTest.terl

binary-protocol-benchmark-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm framing_benchmark::tests::truncated_framing_benchmark_reports_expected_typed_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm framing_benchmark::tests::adversarial_framing_matrix_reports_typed_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm framing_benchmark::tests::framing_workload_parser_rejects_unknown_names -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm framing_benchmark::tests::framing_percentiles_use_nearest_rank_for_tail_samples -- --exact
	$(RUST_TEST) -p terlan --bin terlan-benchmark binary_protocol::
	$(PYTHON) scripts/benchmarks/protocol/protocol_benchmark_test.py
	@if [ "$(TERLAN_PROTOCOL_BENCHMARK_ALREADY_RUN)" = "1" ]; then \
		$(PYTHON) scripts/benchmarks/protocol/protocol_benchmark.py --validate-only; \
	else \
		$(PYTHON) scripts/benchmarks/protocol/protocol_benchmark.py --run; \
	fi

core-type-contracts-check:
	$(MAKE) --no-print-directory tree-sitter-package-check tree-sitter-cli-check

type-alias-shorthand-check: $(CANONICAL_RUST_SUITE_OWNER) tree-sitter-cli-check
	target/debug/terlc test std/core/AtomTest.terl

compiler-purity-metadata-check:
	$(TERLC) test std/core/EffectTest.terl
	$(TERLC) test tests/language/PurityEffectsTest.terl
	$(TERLC) test tests/fixtures/purity_template/PurityTemplateTest.terl

lean-proof-track-check:
	@set -e; \
	trap 'rm -rf proofs/lean/.lake' EXIT; \
	$(MAKE) lean-proof-feature-cull-check; \
	$(MAKE) lean-proof-track-runtime-check; \
	$(MAKE) proof-repro-check; \
	$(MAKE) lean-proof-track-pr-gate; \
	$(MAKE) lean-proof-track-regression-check

lean-proof-feature-cull-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-feature-cull
	@cd proofs/lean && ELAN_NO_UPDATE_CHECK=1 lake env lean Terlan/FeatureCull/LegacyBoundaries.lean

proof-repro-check:
	$(RUST_TEST) -p terlan --bin terlan-quality lean_proof_track
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-track
	@awk -F '\t' 'NR > 1 && $$2 == "current" { print "[proof-repro] " $$1 " " $$7 " reproducibility=pass" }' proofs/lean/ci/lean-proof-artifacts.tsv

proof_repro_check: proof-repro-check

lean-proof-track-pr-gate:
	$(RUST_TEST) -p terlan --bin terlan-quality lean_proof_pr_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-pr

lean-proof-track-runtime-check:
	$(RUST_TEST) -p terlan --bin terlan-quality lean_proof_runtime_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-runtime

lean-proof-track-regression-check:
	$(RUST_TEST) -p terlan --bin terlan-quality lean_proof_regression_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-regression

lean-proof-track-gap-hygiene-check:
	$(RUST_TEST) -p terlan --bin terlan-quality lean_proof_gap_hygiene
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- lean-proof-gap-hygiene

lean-proof-track-release-closeout-check: $(LEAN_PROOF_CLOSEOUT_DEPS) lean-proof-track-gap-hygiene-check
	$(RUST_TEST) --locked -p terlan --bin terlan-lean-proof-closeout
	$(CARGO) run --locked -p terlan --bin terlan-lean-proof-closeout --quiet

release-0-0-7-preflight: HTTP_SOAK_PROFILE := release
release-0-0-7-preflight: vm-http-runtime-attribution-check vm-http-soak-stability-check release-version-channel-check lean-proof-track-release-closeout-check release-promotion-pipeline-check
	@echo "[release-0-0-7-preflight] version/channel and mandatory Lean proof closeout passed"

release-candidate-check:
	$(MAKE) check-gates

function-head-migration-diagnostic-policy-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-migration-diagnostic-policy

function-head-migration-lint-check: function-head-migration-diagnostic-policy-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-migration-lint

function-head-pattern-migration-assist-check: function-head-migration-lint-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-pattern-migration-assist

function-head-pattern-migration-benchmark-check: function-head-pattern-migration-assist-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-pattern-migration-benchmark

function-head-pattern-migration-docs-check: function-head-migration-diagnostic-policy-check function-head-pattern-parameters-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-pattern-migration-docs

function-head-pattern-0-0-7-handoff-check: function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-parameters-check function-head-pattern-migration-docs-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-pattern-handoff

function-head-pattern-parameters-hardening-check: function-head-pattern-0-0-7-handoff-check

function-head-pattern-parameters-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- function-head-observability
	grep -F 'pub add({left, right}: {Int, Int}): Int ->' docs/grammar/README.md
	grep -F 'pub describe({status, body}: Dynamic): String.' docs/grammar/README.md
	grep -F 'pub full_name({name, family_name} = user: User): String ->' docs/grammar/README.md

shape-synonyms-check: shape-implications-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- pattern-matching-support
	grep -F 'shape OkResponse(body)' docs/grammar/README.md
	$(PYTHON) tools/validate_ebnf.py --strict
	$(MAKE) --no-print-directory tree-sitter-package-check tree-sitter-cli-check
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_pattern_test::tests::parses_nullary_constructor_pattern_call -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_public_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::rejects_lowercase_shape_synonym_declaration -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_interface_shape_synonym_declaration_as_structured_decl -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_shape_synonym_raw_declarations -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::expands_local_shape_calls_in_case_and_function_head_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_local_shape_arity_mismatch -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_local_shape_called_as_runtime_value -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_recursive_local_shape_expansion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_local_shape_parameters_and_names -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_in_shape_bodies -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_created_by_shape_expansion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_distinct_shapes_with_equivalent_case_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_distinct_shapes_with_equivalent_function_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_guarded_or_structurally_distinct_shape_clauses -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_shape_subsumed_by_earlier_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_shape_subsumed_by_earlier_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_specific_shape_before_broad_fallback_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_map_shape_with_stricter_required_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_partially_overlapping_shapes_when_both_are_useful -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_guarded_later_case_shape_shadowed_by_unguarded_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_guarded_later_function_shape_shadowed_by_unguarded_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_guarded_broad_shape_before_unguarded_fallback -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_alpha_equivalent_guarded_case_shapes -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_alpha_equivalent_guarded_function_shapes -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_equivalent_shape_patterns_with_distinct_guards -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_shape_with_contained_integer_range -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_shape_with_stricter_integer_bound -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_equality_guard_contained_by_reversed_integer_bound -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_disjunction_when_every_branch_is_contained -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_range_contained_by_disjunction_of_conjunctions -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_disjunctive_guard_when_later_range_crosses_a_gap -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_guard_implication_beyond_branch_budget_conservatively -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_case_guard_with_implied_variable_relation -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::rejects_later_function_guard_with_implied_variable_equality -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_distinct_variable_relations_on_equivalent_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_guard_with_transitive_strict_relation -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_function_guard_with_transitive_equality -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_contradictory_later_relation_guard -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::rejects_later_guard_with_equality_inequality_conflict -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_relation_transitive_test::accepts_non_strict_chain_when_earlier_guard_requires_strict_order -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_case_guard_repeating_predicate_with_stronger_constraint -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_function_guard_implying_predicate_disjunction -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_distinct_local_predicates_with_equivalent_visible_bodies -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_distinct_local_predicate_body_that_implies_earlier_body -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_distinct_predicates_with_call_bearing_bodies_conservatively -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::does_not_use_non_bool_function_bodies_as_predicate_proofs -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_later_predicate_when_earlier_guard_requires_extra_evidence -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_same_predicate_with_distinct_arguments -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_later_guard_repeating_negated_predicate -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_contradictory_positive_and_negated_predicate_guard -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_double_negation_as_positive_predicate_evidence -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_opposing_predicate_polarities_as_distinct_guards -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_compound_negation_equivalent_to_explicit_de_morgan_guard -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_conjunction_equivalent_to_negative_disjunction -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_compound_negation_contradicted_by_positive_predicate -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_partial_negative_evidence_for_earlier_negative_conjunction -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_comparison_equivalent_to_inverted_operator -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_integer_equality_equivalent_to_inequality -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_integer_inequality_equivalent_to_equality -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_variable_relation_equivalent_to_inverse_relation -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_variable_equality_equivalent_to_inequality -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::rejects_negated_reversed_comparison_after_operator_inversion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_predicate_implication_test::accepts_inverted_comparison_that_does_not_imply_earlier_range -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shape_overlap_test::accepts_narrow_guard_before_broader_guard_fallback -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::composes_guarded_shape_with_explicit_clause_guard -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::expands_nested_shape_guards_and_substitutes_parameters -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_guard_parameter_substitution_from_non_value_pattern -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::composes_guarded_shape_with_comprehension_filter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::expands_guarded_shape_in_let_pattern_as_case_assertion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::carries_guarded_shapes_into_grouped_let_success_guards -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::let_else_test::guarded_shape_grouped_let_typechecks_and_lowers_success_guards -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::let_else_test::guarded_shape_grouped_let_keeps_fallback_outside_success_scope -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::gives_each_private_shape_binding_a_distinct_compiler_name -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::substitutes_string_capture_parameters_in_text_and_binding_metadata -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::gives_private_string_captures_compiler_names -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::shapes_test::tests::rejects_non_binding_string_capture_arguments -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_binds_explicit_type -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_rejects_duplicate_capture_names -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::pattern_test::syntax_output_string_capture_pattern_rejects_invalid_capture_annotation -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_adjacent_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_unterminated_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_empty_string_capture_patterns -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::shapes_test::imported_shape_expansion_normalizes_selected_alias_and_nested_guard -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::shapes_test::imported_shape_expansion_supports_wildcard_imports -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::shapes_test::imported_shape_expansion_rejects_alias_called_as_runtime_value -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::shapes_test::imported_shape_expansion_rejects_ambiguous_local_aliases -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::shapes_test::imported_shape_expansion_rejects_recursive_provider_shapes -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::test::test_shape_import_test::project_vm_tests_execute_selected_imported_shape_alias -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_record_case -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::emit_js::direct_ast_test::emit_core_module_with_direct_oxc_ast_handles_constructor_case -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_literal_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_tuple_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_nested_literal_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_map_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_record_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_constructor_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_imported_zero_arity_constructor_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_emits_guarded_imported_shape_for_js_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::diagnostic_test::syntax_output_accepts_local_unguarded_shape_synonym_after_expansion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::diagnostic_test::syntax_output_typechecks_composed_shape_guards -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::diagnostic_test::syntax_output_rejects_non_bool_shape_guard_after_expansion -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::shape_purity_test::syntax_output_rejects_effectful_helper_in_shape_guard_after_expansion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::lib_test::lsp_document_accepts_shape_synonym_declarations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::lib_test::document_symbols_include_raw_shape_declarations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::lib_test::completion_items_include_local_and_imported_shapes_and_functions -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::hover::hover_test::hover_returns_same_document_raw_shape_docs -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::hover::hover_test::hover_returns_imported_shape_docs_from_interface -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::doc::render::render_test::renders_public_shape_declarations -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::lib_test::interface_rendering_preserves_public_shape_declarations -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_ordinary_function_named_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_local_binding_named_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_pattern_binding_named_shape -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::old_shape_fat_arrow_spelling_is_not_shape_synonym_surface -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::old_shape_runtime_arrow_spelling_is_not_shape_synonym_surface -- --exact
	$(TERLC) test tests/pattern/ShapeSynonymTest.terl

syntax-contract-check:
	$(PYTHON) tools/validate_ebnf.py --strict
	$(MAKE) --no-print-directory tree-sitter-package-check tree-sitter-cli-check
	grep -F 'ImplicationConstraint ::= "=>" StructuralEvidenceShape .' docs/grammar/TERLAN_SYNTAX_SPEC.ebnf
	grep -F 'The implication arrow is accepted only as generic-parameter shorthand' docs/grammar/README.md
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_structural_implication_in_function_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_structural_generic_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_implication_arrow_as_runtime_expression_operator -- --exact

shape-implications-check: syntax-contract-check
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_structural_implication_in_function_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_structural_implication_in_struct_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::parses_structural_implication_in_type_alias_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::syntax_output::syntax_output_decl_test::tests::syntax_output_preserves_structural_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::rejects_non_structural_generic_implication_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::rejects_empty_structural_generic_implication_target -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::rejects_negative_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::positive_structural_implication_remains_supported -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::rejects_duplicate_structural_implication_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::rejects_duplicate_nested_structural_implication_fields -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::rejects_duplicate_structural_implication_fields_inside_generic_types -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::nested_structural_implication_fields_use_independent_scopes -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::rejects_duplicate_record_fields_in_implication_evidence_aliases -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::type_params_test::record_type_alias_fields_use_independent_nested_scopes -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_structural_generic_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_structural_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_structural_generic_type_alias_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::formatter::formatter_test::formatter_preserves_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_preserves_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_imported_generic_trait_impl_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::lib_test::interface_rendering_preserves_generic_trait_impl_implications -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::doc::render::render_test::renders_generic_trait_impl_implications -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::type_model_test::type_aliases_preserve_structural_implication_bounds -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_forwarded_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_imported_type_alias_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_in_struct_field -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_forwarded_type_alias_in_generic_struct_field -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_struct_field -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_alias_body -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_in_constructor_signature -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_constructor_signature -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_constructor_return -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_in_template_prop -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_template_prop -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_in_trait_signature -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_trait_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_trait_return -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_forwarded_type_alias_in_generic_trait_method -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_generic_trait_method -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_proven_type_alias_in_explicit_impl_signature -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_explicit_impl_parameter -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_type_alias_in_explicit_impl_return -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_nested_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_forwarded_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_missing_structural_implication_field -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_wrong_structural_implication_field_type -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_dynamic_structural_implication_evidence -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_open_map_structural_implication_evidence -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_forwarded_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_field_outside_structural_implication_scope -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_private_structural_implication_field -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_imported_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::hir::lib_test::interface_rendering_preserves_generic_struct_implications -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_imported_generic_struct_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_imported_generic_struct_projection -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_accepts_imported_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::typeck::implication_test::syntax_output_rejects_unproven_imported_receiver_method_structural_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_implication_arrow_as_runtime_expression_operator -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_implication_arrow_in_lambda_body -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_implication_arrow_in_case_branch_body -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::rejects_implication_arrow_on_struct_field_declaration -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_decl_test::tests::rejects_implication_arrow_in_ordinary_type_alias_body -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::test::test_shape_import_test::project_vm_tests_execute_imported_receiver_method_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::test::test_shape_import_test::project_vm_tests_execute_imported_generic_struct_implication -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::build::build_test::tests::shape_js_test::build_command_executes_structural_implication_for_js_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::hover::hover_test::hover_preserves_local_structural_implication_signature -- --exact
	$(EXACT_CARGO_TEST) -p terlan --features editor-lsp --bin terlan-lsp terlan_lsp::hover::hover_test::editor_surfaces_preserve_imported_structural_implication -- --exact
	$(RUST_TEST) -p terlan --bin terlan-quality shape_implications_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- shape-implications
	@cd proofs/lean && ELAN_NO_UPDATE_CHECK=1 lake env lean Terlan/Type/ShapeImplication.lean
	$(TERLC) test tests/language/ShapeImplicationTest.terl
	$(TERLC) test std/binary/BinaryTest.terl --name protocol_name_uses_structural_evidence_across_metadata_types

typed-template-interpolation-check: shape-implications-check tree-sitter-package-check tree-sitter-cli-check

wasm-coreir-lowering-check:

wasm-runtime-exec-check:

wasm-contract-discovery-check:

oxc-boundary-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- oxc-boundary

adversarial-check:
	$(RUST_TEST) --locked -p terlan adversarial -- --nocapture
	$(PYTHON) tools/check_release_packaging_adversarial.py

coverage-check:
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
		echo "coverage-check requires cargo-llvm-cov; install with: cargo install cargo-llvm-cov --locked"; \
		exit 127; \
	}
	$(CARGO) build --quiet --locked -p terlan --bin terlan-vm
	TERLAN_VM_RUNNER=$(COVERAGE_VM_RUNNER) $(CARGO) llvm-cov --quiet --locked -p terlan --bin terlc --ignore-filename-regex '$(COVERAGE_IGNORE_FILENAME_REGEX)' --fail-under-lines $(COVERAGE_MIN)

release-hardening-check: adversarial-check coverage-check

erlang-backend-classification-check:
	$(RUST_TEST) -p terlan --bin terlan-quality erlang_backend_classification_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- erlang-backend-classification

terlan-vm-artifact-format-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm native_image_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-artifact-format

tvm-native-image-format-check: terlan-vm-artifact-format-check

tvm-direct-aot-backend-check: terlan-vm-artifact-format-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::direct_backend::direct_backend_test
	$(RUST_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact

tvm-aot-lowering-coverage-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::lowering_coverage_test

tvm-aot-application-closure-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::application_admission_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::static_callable_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::higher_order_specialization_test

tvm-aot-case-lowering-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::case_lowering_test

tvm-aot-managed-field-projection-check:
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::field_projection_test

tvm-aot-owned-closure-representation-check:
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::managed_closure_test

tvm-aot-closure-dispatch-check:
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::managed_closure_dispatch_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::closure_conversion_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::cranelift::managed_callback_test
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::native_descriptor_test::lifted_callable_descriptor_separates_captures_from_call_arguments -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::managed_execution_test::executable_metadata_installs_generation_scoped_closure_dispatch -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlc runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlc runtime::native_image::native_image_test::descriptor_rejects_invalid_abi_ids_and_boundary_types -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlc commands::build::build_test::tests::deterministic_artifact_test::deterministic_module_emits_reproducible_vm_artifact_bytes -- --exact

tvm-aot-typed-mailbox-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::core_intrinsic_process_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::expression_test::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::trait_negative_test::syntax_output_rejects_actor_message_with_denied_delivery -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::trait_negative_test::syntax_output_accepts_actor_and_node_values_with_default_delivery -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::cranelift::managed_type_test

tvm-aot-typed-lifecycle-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_erases_typed_lifecycle_descriptors -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::expression_test::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::expression_test::syntax_output_accepts_explicit_typed_process_lifecycle_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::expression_test::syntax_output_rejects_scalar_process_lifecycle_arguments -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::cranelift::managed_type_test::typed_public_lifecycle_operations_lower_to_existing_vm_transitions -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::lowering_coverage_test::intrinsic_families_have_explicit_lowering_dispositions -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test

tvm-aot-managed-continuation-check:
	$(RUST_TEST) --locked -p terlan --test managed_continuation_aot

tvm-aot-thread-neutral-continuation-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::thread_neutral::thread_neutral_test::parked_native_continuation_is_send_sync_and_static -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::pure_native_transport_test::parked_native_continuation_resumes_after_thread_transfer -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::direct_backend::direct_backend_test::direct_backend_parked_state_is_send_sync_and_static -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test

tvm-aot-multicore-readiness-check: tvm-aot-thread-neutral-continuation-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::multicore_model_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::actor_continuations_interleave_reentrantly_on_one_shard -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::empty_shard_forks_execute_concurrently_without_shared_state -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::direct_backend::direct_backend_test::execution_runtime_interleaves_owner_scoped_continuations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::memory::memory_test::memory_accounted_mailbox_send_receive_and_pressure_are_atomic -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_message_wakeup_is_deduplicated_and_missing_target_is_side_effect_free -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::native_image::managed::managed_test::actor_local_collection_budget_cannot_pause_or_mutate_another_heap -- --exact
	$(RUST_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact
	@rg -q 'struct PureNativeExecutionContext' crates/terlan/src/runtime/vm/pure_native.rs
	@rg -q 'struct PureNativeExecutionRuntime' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'continuations: BTreeMap<u64, PendingNativeContinuation>' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'struct NativeContinuationClaim' crates/terlan/src/runtime/vm/pure_native/execution_runtime.rs
	@rg -q 'claim_continuation' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'struct VmMailboxPublication' crates/terlan/src/runtime/vm/memory/publication.rs
	@rg -q 'accepted accounted actor send must produce a publication receipt' crates/terlan/src/runtime/vm/actor_impl.rs
	@rg -q 'execution: PureNativeExecutionRuntime' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'context: &mut PureNativeExecutionContext' crates/terlan/src/runtime/vm/pure_native/direct_backend.rs
	@rg -q 'execution_context_rejects_foreign_actor_before_transition_service' crates/terlan/src/runtime/vm/pure_native_transport_test.rs
	@rg -q 'image: PureNativeExecutionImage' crates/terlan/src/commands/serve/handler_cache.rs
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

tvm-aot-thread-sanitizer-check:
	$(PYTHON) tools/check_tvm_aot_thread_sanitizer.py self-test
	@if rustup target list --installed | rg -qx 'x86_64-unknown-linux-gnutsan'; then \
		$(PYTHON) tools/check_tvm_aot_thread_sanitizer.py run; \
	else \
		echo 'TVM AOT ThreadSanitizer executable lane unavailable locally; contract passed'; \
	fi

tvm-aot-thread-sanitizer-contract-check:
	$(PYTHON) tools/check_tvm_aot_thread_sanitizer.py self-test

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
	tvm-aot-thread-sanitizer-check \
	tvm-aot-c-abi-boundary-check \
	tvm-aot-compilation-time-check \
	tvm-single-image-artifact-check \
	no-tvm-json-runtime-check \
	no-vmir-interpreter-check \
	rust-quality-check \
	roadmap-gate-integrity-check

tvm-aot-roadmap-reconciliation-check:
	$(PYTHON) tools/check_tvm_aot_roadmap_reconciliation.py self-test
	$(PYTHON) tools/check_tvm_aot_roadmap_reconciliation.py check

tvm-aot-release-closeout-contract-check: tvm-aot-thread-sanitizer-contract-check tvm-aot-platform-matrix-contract-check
	$(PYTHON) tools/check_tvm_aot_release_closeout.py self-test

tvm-aot-release-closeout-check: tvm-aot-release-closeout-contract-check
	$(MAKE) $(AOT_RELEASE_LOCAL_GATES)
	env -u RUSTFLAGS $(CARGO) check --locked -p terlan
	$(PYTHON) tools/check_tvm_aot_release_closeout.py record-local

tvm-aot-static-callable-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::static_callable_test

tvm-aot-higher-order-specialization-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::higher_order_specialization_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::static_callable_test::source_captured_lambda_lowers_into_native_application -- --exact

tvm-aot-application-conformance-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::aot3_conformance_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::expression::free_variable_analysis_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::closure_conversion_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::generic_specialization_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::cast_lowering_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::collection_values_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::structured_case_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::try_lowering_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::capability_transition_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::constructor_lowering_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::template_values_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::http_values_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::native_ir::lowering_coverage_test

tvm-managed-memory-check:
	$(RUST_TEST) --locked -p terlan --bin terlc managed_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_atom_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_sequence_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_aggregate_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_list_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_map_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_set_test
	$(RUST_TEST) --locked -p terlan --bin terlc collection_abi_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::collections::collections_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::atom_inventory_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm managed_execution_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm direct_backend_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::constructor_lowering_test
	$(RUST_TEST) --locked -p terlan --bin terlc managed_callback
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::escape_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::scalar_replacement_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_mailbox_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::cranelift::managed_type_test::typed_public_mailbox_operations_lower_to_fixed_native_transition_frames -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::typed_native_
	$(RUST_TEST) --locked -p terlan --bin terlc pure_native_transport
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::cranelift::managed_stack_map_test::managed_reference_live_across_cranelift_safepoint_emits_precise_stack_map -- --exact

tvm-aot-capability-worker-check: tvm-aot-stale-epoch-check
	$(CARGO) build --locked -p terlan --bin terlan-native-worker
	$(RUST_TEST) --locked -p terlan --bin terlan-native-worker main_test
	$(RUST_TEST) --locked -p terlan --bin terlan-native-worker sandbox
	$(RUST_TEST) --locked -p terlan --bin terlan-native-worker capability_wire
	$(RUST_TEST) --locked -p terlan --bin terlan-native-worker protocol::protocol_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm capability_worker
	TERLAN_TEST_CAPABILITY_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::capability_worker::capability_worker_test::capability_worker_process_transport_runs_full_cycle -- --ignored --exact
	TERLAN_TEST_CAPABILITY_WORKER=$(CURDIR)/target/debug/terlan-native-worker $(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::capability_worker::capability_worker_test::capability_worker_sandbox_closes_inherited_descriptor -- --ignored --exact
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
	@if rg -n 'VmCapabilityWorker|TERLAN_NATIVE_WORKER|terlan-native-worker' crates/terlan/src/vm/main/native_image_runner.rs crates/terlan/src/commands/test/vm_runner.rs crates/terlan/src/commands/vm.rs crates/terlan/src/commands/repl/mod_part_002.rs crates/terlan/src/commands/serve/handler_cache.rs; then \
		echo 'error[aot.capability_worker]: ordinary actor execution must not reference capability-worker transport'; \
		exit 1; \
	fi

.PHONY: tvm-managed-list-profile-benchmark-check
tvm-managed-list-profile-benchmark-check:
	TERLAN_MANAGED_LIST_PROFILE_OUTPUT=$(CURDIR)/target/quality/tvm-managed-list-profile.json $(RUST_TEST) --locked --release -p terlan --bin terlc runtime::native_image::managed::lists::managed_list_profile_benchmark_test::managed_list_profiles_emit_stable_benchmark_report -- --exact --nocapture
	test -s target/quality/tvm-managed-list-profile.json

.PHONY: tvm-aot-runtime-workload-benchmark-check
tvm-aot-runtime-workload-benchmark-check:
	TERLAN_BENCH_AOT_RUNTIME_OUTPUT=$(CURDIR)/target/quality/vm-aot-runtime-workloads.json $(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- vm-aot-runtime-workloads
	test -s target/quality/vm-aot-runtime-workloads.json
	@rg -q '"status": "completed"' target/quality/vm-aot-runtime-workloads.json
	@rg -q '"p99_ns":' target/quality/vm-aot-runtime-workloads.json
	@for workload in actor_heap_allocation local_message_round_trip scheduler_yield_cycle actor_local_collection_pause actor_spawn_exit_churn mixed_actor_runtime_tail; do \
		rg -q "\"name\": \"$$workload\"" target/quality/vm-aot-runtime-workloads.json || exit 1; \
	done

tvm-native-image-loader-check: tvm-direct-aot-backend-check

tvm-aot-consumer-check: tvm-native-image-loader-check
	$(RUST_TEST) --locked -p terlan --test tvm_transition_rejection

tvm-aot-test-consumer-check: tvm-aot-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_executes_bool_test -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_fails_false_bool_test -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_writes_runtime_manifests -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::test::test_command_test::native_placeholder_filter_preserves_source_functions_in_mixed_module -- --exact
	@rg -q 'compile_test_native_image' crates/terlan/src/commands/test/mod_part_001.rs
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/test/vm_runner.rs
	@if rg -n 'struct TerlanVm|runtime::vm::TerlanVm|evaluate_[A-Za-z0-9_]*|apply_closure|\.tvm\.json' \
		crates/terlan/src/commands/test --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.test_consumer]: test execution must use one admitted native image without evaluator or serialized-runtime fallback'; \
		exit 1; \
	fi

tvm-aot-repl-consumer-check: tvm-aot-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::scalar_repl_generation_executes_without_resident_core_ir -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::float_repl_generation_executes_without_resident_core_ir -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::managed_repl_generation_executes_without_resident_core_ir -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::unchanged_repl_generation_reuses_active_native_shard -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact
	@rg -q 'compile_repl_native_image' crates/terlan/src/commands/repl/mod_part_002.rs
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/repl/mod_part_002.rs
	@rg -q 'active\.shard\.replace_image\(&native_image\)' crates/terlan/src/commands/repl/mod_part_002.rs
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
	$(RUST_TEST) --locked -p terlan --bin terlc commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::debug::debug_test::native_debug_session_rejects_source_and_renamed_json_targets -- --exact
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/debug/session.rs
	@rg -q 'inspect_tvm_native_debug' crates/terlan/src/commands/debug/session.rs
	@if rg -n 'struct TerlanVm|runtime::vm::TerlanVm|evaluate_[A-Za-z0-9_]*|apply_closure|\.tvm\.json|PureNativeWorker|Evaluator|evaluator' \
		crates/terlan/src/commands/debug --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.debugger_consumer]: debugger admission must use a native image without evaluator or serialized-runtime fallback'; \
		exit 1; \
	fi

tvm-aot-hot-reload-consumer-check: tvm-aot-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test::drain_timeout_quarantines_and_retains_reachable_image -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::vm::pure_native::execution_shard::execution_shard_test::draining_generation_closes_entries_and_preserves_accepted_continuations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::vm::native_image_diagnostics::generation_lifetime_test::generation_reference_snapshot_proves_quiescence_and_orders_diagnostics -- --exact
	@rg -q 'compile_reload_native_image' crates/terlan/src/commands/vm/native_reload.rs
	@rg -q 'publish_native_generation' crates/terlan/src/commands/vm/native_reload.rs
	@rg -q 'replace_image_before_deadline' crates/terlan/src/runtime/vm/source_reload.rs
	@rg -q 'quarantine_drain_timeout' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@if rg -n 'publish_changed_files_with_report' crates/terlan/src/commands/vm/native_reload.rs; then \
		echo 'error[aot.hot_reload_consumer]: native hot reload must not publish code-server-only generations'; \
		exit 1; \
	fi

tvm-aot-package-install-consumer-check: tvm-aot-consumer-check
	$(CARGO) build -p terlan --bin terlc --bin terlan-vm
	$(PYTHON) tools/check_tvm_package_install_consumer.py
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-release-install-validation

tvm-aot-support-crash-metadata-check: tvm-aot-package-install-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm native_image_diagnostics
	$(RUST_TEST) --locked -p terlan --bin terlan-vm fatal_diagnostics
	$(RUST_TEST) --locked -p terlan --bin terlan-vm support_bundle_replay_metadata_binds_native_generation_once
	$(RUST_TEST) --locked -p terlan --bin terlan-vm drain_timeout_quarantines_and_retains_reachable_image
	$(RUST_TEST) --locked -p terlan --bin terlan-vm pure_native::execution_shard::execution_shard_test
	@rg -q 'support-bundle <file.tvm>' crates/terlan/src/vm/main_part_004.rs
	@if rg -n 'CoreExpr|CoreFunction|TvmExecutableDescriptor|executable_bytes|source_path' \
		crates/terlan/src/runtime/vm/native_image_diagnostics.rs \
		crates/terlan/src/runtime/vm/support_bundle.rs \
		crates/terlan/src/runtime/vm/fatal_diagnostics.rs; then \
		echo 'error[aot.support_bundle]: native diagnostic metadata must not retain executable CoreIR, code bytes, or host source paths'; \
		exit 1; \
	fi

tvm-aot-image-lifetime-check: tvm-aot-package-install-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::sealed
	$(RUST_TEST) --locked -p terlan --bin terlc commands::vm::vm_test::admitted_native_generation_survives_source_image_replacement -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::replacement_rejects_duplicate_image_generation_before_drain -- --exact
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
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_boundary::adapter_abi::adapter_abi_test
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::native_image_test::native_inspection_accepts_real_elf_and_rejects_wrong_target_and_abi -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::structured_c_metadata_generates_real_ffi_package -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::bind::cpp_binding_generator::cpp_binding_generator_test::structured_cpp_metadata_generates_real_cxx_package -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::generated_c_adapter_compiles_and_enforces_public_protocol -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::bind::cpp_binding_generator::cpp_binding_generator_test::generated_cxx_adapter_compiles_and_enforces_public_protocol -- --exact
	@rg -q 'PUBLIC_ADAPTER_ABI_VERSION' crates/terlan/src/runtime/native_image/image.rs crates/terlan/src/commands/build/vm_artifact/native_descriptor.rs
	@rg -q 'cache_identity\(&target\.triple, &target\.calling_convention\)' crates/terlan/src/commands/build/vm_artifact/native_image.rs
	@if rg -ni 'TvmRef|actor.?heap|Cranelift|continuation|native.?stack|shard|thread.?identity' \
		crates/terlan/src/runtime/native_boundary/adapter_abi.rs \
		crates/terlan/src/commands/bind/c_abi_binding_generator_part_004.rs \
		crates/terlan/src/commands/bind/cpp_binding_generator_part_004.rs; then \
		echo 'error[aot.public_adapter]: public adapter metadata must not expose the private runtime ABI'; \
		exit 1; \
	fi

tvm-aot-platform-matrix-contract-check:
	$(PYTHON) tools/check_tvm_aot_platform_matrix.py self-test

tvm-aot-platform-target-check: tvm-aot-platform-matrix-contract-check
	$(PYTHON) tools/check_tvm_aot_platform_matrix.py target

tvm-aot-platform-matrix-check: tvm-aot-platform-matrix-contract-check
	$(PYTHON) tools/check_tvm_aot_platform_matrix.py target

tvm-aot-http-managed-cycle-check: tvm-aot-consumer-check
	$(RUST_TEST) --locked -p terlan --bin terlc literal_abi_test
	$(RUST_TEST) --locked -p terlan --bin terlc http_values_test
	$(RUST_TEST) --locked -p terlan --bin terlc response_bridge_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-request-accessor-check: tvm-aot-http-managed-cycle-check
	$(RUST_TEST) --locked -p terlan --bin terlc operation_abi_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::request_accessors_lower_to_checked_managed_operations -- --exact

tvm-aot-http-response-mutation-check: tvm-aot-http-request-accessor-check
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::response_updates_are_persistent_and_preserve_repeated_headers -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::response_status_headers_and_raw_cookies_lower_to_persistent_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler::response_bridge::response_bridge_test::native_repeated_headers_are_validated_and_preserved -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-typed-metadata-check: tvm-aot-http-response-mutation-check
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::http::http_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::typed_cookie_jar_and_security_calls_rewrite_to_managed_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::typed_security_policy_rejects_unknown_marker -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler::response_bridge::response_bridge_test::native_security_headers_are_not_claimed_by_transport_framing -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_uses_source_bridge_for_managed_only_module_without_hyper -- --exact

tvm-aot-http-router-callable-check: tvm-aot-http-typed-metadata-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::router::router_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::case_lowering_test::string_case_patterns_lower_to_managed_value_equality -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::string_equal_operation_is_value_based_and_checked -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::string_append_operation_concatenates_validated_values -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact

tvm-aot-http-managed-error-check: tvm-aot-http-router-callable-check
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::aggregate_scalar_projection_returns_an_unboxed_native_word -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::typed_http_error_constructor_and_accessors_lower_to_managed_values -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::typed_http_error_operations_reject_invalid_arities -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact

tvm-aot-http-template-check: tvm-aot-http-managed-error-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::template_values_test
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::string_list_join
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_executes_managed_template_html_handler -- --exact

tvm-aot-http-template-render-plan-check: tvm-aot-http-template-check
	$(RUST_TEST) --locked -p terlan --bin terlc formal_pipeline::formal_pipeline_test::formal_pipeline_carries_validated_template_render_plans_into_core_ir -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::html_escape_operations_preserve_context_and_validate_arity -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_executes_external_template_render_plan -- --exact

tvm-aot-http-template-expression-check: tvm-aot-http-template-render-plan-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::template_values_test
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::operation_abi_test::typed_template_rendering_enforces_attributes_options_and_urls -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_executes_complete_typed_template_matrix -- --exact

tvm-aot-http-body-json-check: tvm-aot-http-template-expression-check
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_image::managed::operation_abi::json::json_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::body_json_result_case_lowers_to_typed_managed_branches -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_decodes_managed_json_body_result -- --exact

tvm-aot-http-session-check: tvm-aot-http-body-json-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::session_calls_lower_to_vm_owned_managed_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::session_import_installs_complete_managed_boundary_metadata -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::vm::http_session::http_session_test::http_session_adapter_functions_delegate_to_actor_runtime -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_session_state_and_lifecycle_are_vm_owned -- --exact

tvm-aot-http-managed-boundary-check: tvm-aot-http-session-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::complete_http_managed_boundary_inventory_is_closed_and_decodable -- --exact

tvm-aot-http-channel-plan-check: tvm-aot-http-managed-boundary-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::router::router_test::aot_router_plan_materializes_canonical_channel_targets -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::http_values_test::request_option_case_lowers_without_scalar_constructor_patterns -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::vm_stream_websocket_upgrade_activates_materialized_router_middleware -- --exact

tvm-aot-http-persistent-shard-check: tvm-aot-http-channel-plan-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::one_owner_loop_services_multiple_parked_actors_without_migration -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::new_actors_balance_across_shards_and_resume_sticky -- --exact
	@! rg -q 'spawn_shard\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'shard\.shutdown\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'Mutex<PureNativeExecutionShard' crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache/
	@rg -q 'sync_channel\(SHARD_INBOX_CAPACITY\)' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs
	@rg -q 'serve\.aot\.capability_dispatch_missing' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs

tvm-http-axum-performance-record:
	$(CARGO) build --locked --release -p terlan --bin terlan-axum-baseline --bin terlan-http-framework-benchmark --features axum-baseline
	TERLAN_BENCH_HTTP_AXUM_BIN=$(CURDIR)/target/release/terlan-axum-baseline \
	TERLAN_BENCH_HTTP_AXUM_OUTPUT=$(CURDIR)/target/quality/http-axum-performance.json \
	$(CURDIR)/target/release/terlan-http-framework-benchmark
	test -s target/quality/http-axum-performance.json
	@rg -q '"schema": "terlan-http-framework-performance-v1"' target/quality/http-axum-performance.json

tvm-aot-http-native-invocation-check: tvm-aot-http-persistent-shard-check

tvm-aot-http-websocket-invocation-check: tvm-aot-http-native-invocation-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::router::router_test::aot_router_plan_materializes_websocket_callbacks -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler::websocket_invocation::websocket_invocation_test::websocket_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact

tvm-aot-http-sse-invocation-check: tvm-aot-http-websocket-invocation-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::router::router_test::aot_router_plan_materializes_sse_callbacks -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler::sse_invocation::sse_invocation_test::sse_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact

tvm-aot-http-generation-lifetime-check: tvm-aot-http-sse-invocation-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::handler_cache_generation_test::hot_reload_pins_in_flight_generation_until_its_last_lease_drops -- --exact

tvm-aot-http-channel-transport-check: tvm-aot-http-generation-lifetime-check
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::channel_transport::channel_transport_test::production_channel_pumps_preserve_vm_lifecycle_and_pressure_contracts -- --exact

tvm-aot-http-cleanup-check: tvm-aot-http-channel-transport-check
	$(RUST_TEST) --locked -p terlan --bin terlc callbacks_share_native_invocation_entry_resume_and_cancellation
	$(RUST_TEST) --locked -p terlan --bin terlc request_resources_track_peaks_and_release_every_transient_class
	$(RUST_TEST) --locked -p terlan --bin terlc request_resources_reject_duplicate_stale_and_unknown_completion
	$(RUST_TEST) --locked -p terlan --bin terlc vm_accounted_websocket_queue_cancellation_releases_pending_frames
	$(RUST_TEST) --locked -p terlan --bin terlc vm_accounted_sse_stream_cancellation_releases_all_pending_buffers
	$(RUST_TEST) --locked -p terlan --bin terlc http_session_expiration_cleans_actor_table_and_reports_stale
	$(RUST_TEST) --locked -p terlan --bin terlc timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order
	$(RUST_TEST) --locked -p terlan --bin terlc resource_table_cleans_up_owner_resources_on_process_exit
	$(RUST_TEST) --locked -p terlan --bin terlc vm_http_tcp_server_shutdown_closes_listener_and_active_handlers
	$(RUST_TEST) --locked -p terlan --bin terlc native_boundary_deadline_timeout_wakes_actor_and_rejects_late_completion

tvm-aot-http-lifecycle-inventory-check: tvm-aot-http-cleanup-check runtime-aot-only-check rust-quality-check
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-self-test
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
		$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-compare || \
		(echo 'error[aot.http.lifecycle_inventory]: performance evidence must be wholly absent or complete, comparable, and within policy' && exit 1); \
	fi

tvm-aot-http-checked-coreir-reference-record:
	@test -n "$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN)" || (echo 'error[aot.http.performance]: set TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN to the preserved checked-CoreIR terlc binary' && exit 1)
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-self-test
	TERLAN_BENCH_HTTP_AOT_LANE=checked-coreir \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN) \
	TERLAN_BENCH_HTTP_AOT_OUTPUT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance
	test -s ../benchmarks/results/http-checked-coreir-performance.json

tvm-aot-http-performance-check: tvm-aot-http-cleanup-check
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-self-test
	test -s ../benchmarks/results/http-checked-coreir-performance.json
	$(CARGO) build --locked --release -p terlan --bin terlc
	TERLAN_BENCH_HTTP_AOT_LANE=native-aot \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(CURDIR)/target/release/terlc \
	TERLAN_BENCH_HTTP_AOT_OUTPUT=$(CURDIR)/target/quality/http-native-aot-performance.json \
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance
	TERLAN_BENCH_HTTP_CHECKED_COREIR_REPORT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
	TERLAN_BENCH_HTTP_NATIVE_AOT_REPORT=$(CURDIR)/target/quality/http-native-aot-performance.json \
	TERLAN_BENCH_HTTP_AOT_COMPARISON_OUTPUT=$(CURDIR)/target/quality/http-aot-performance-comparison.json \
	TERLAN_BENCH_HTTP_AOT_POLICY=$(CURDIR)/benchmarks/baselines/http-aot-performance-limits.json \
	$(CARGO) run --locked --release -p terlan --bin terlan-benchmark --quiet -- http-aot-performance-compare
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
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::native_cache_test::native_cache_rejects_poisoned_keys_target_drift_and_incomplete_publications -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::native_reuse::native_reuse_test
	$(RUST_TEST) --locked -p terlan --test direct_aot_package package_build_emits_one_tvm_image_with_qualified_module_exports -- --exact
	$(RUST_TEST) --locked -p terlan --test direct_aot_cache native_aot_cache_verifies_and_recovers_every_required_file -- --exact

tvm-aot-compilation-benchmark-check:
	$(CARGO) build --locked --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark
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
	@for measurement in small_cold_development multi_cold_development one_package_edit no_op_development cold_release package_relink repl_startup first_repl changed_repl unchanged_repl; do \
		rg -q "\"name\": \"$$measurement\"" target/quality/aot-compilation-benchmark.json || exit 1; \
	done

tvm-aot-compilation-time-check: tvm-single-image-artifact-check tvm-aot-compilation-benchmark-check
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::specialization_budget_test
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::native_ir::codegen_policy_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::build_test::tests::args_test::parse_build_args_selects_explicit_native_release_policy -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::build_test::tests::args_test::build_command_rejects_release_policy_for_non_vm_target -- --exact
	$(RUST_TEST) --locked -p terlan --test direct_aot_cache native_codegen_policies_publish_and_reuse_distinct_cache_entries -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::parallel_compile_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::source_roots_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::checked_cache_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::build_test::tests::parallel_compilation_test::parallel_frontend_compilation_preserves_one_application_link -- --exact
	$(RUST_TEST) --locked --release -p terlan --test direct_aot_cache vm_aot_timings_report_compile_and_native_artifact_phases -- --exact
	$(RUST_TEST) --locked --release -p terlan --test direct_aot_cache vm_aot_warm_noop_p95_stays_under_one_second -- --exact
	$(RUST_TEST) --locked --release -p terlan --test direct_aot_cache unchanged_repl_generation_reuses_native_image_without_relinking -- --exact
	$(RUST_TEST) --locked --release -p terlan --bin terlc commands::repl::repl_aot_test::native_repl_unchanged_generation_p95_stays_under_one_second -- --exact --ignored
	$(RUST_TEST) --locked --release -p terlan --bin terlc commands::repl::repl_aot_test::native_repl_changed_generation_p95_stays_under_one_second -- --exact --ignored

no-tvm-json-runtime-check:
	$(RUST_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_tvm_json_artifact_rejections

no-vmir-interpreter-check:
	$(RUST_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_vmir_interpreter_rejections

runtime-aot-only-check:
	$(PYTHON) tools/release_transition_scan.py crates tools tests std docs
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::build_test::tests::args_test::parse_build_args_rejects_runtime_fallback_selection -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::build::vm_artifact::output_cleanup_test::native_output_cleanup_removes_json_and_reuse_sidecars -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::run::run_test::validate_run_args_rejects_runtime_fallback_selection -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::test::test_command_test::parse_test_args_rejects_runtime_fallback_selection -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::serve_test::parse_serve_args_rejects_explicit_beam_handler_runtime -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::handler_cache_generation_test::handler_cache_compilation_removes_legacy_runtime_sidecars -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::debug::debug_test::native_debug_session_rejects_source_and_renamed_json_targets -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::debug::debug_test::debug_args_reject_runtime_fallback_selection -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_ignores_renamed_json_and_cleans_legacy_sidecars -- --exact
	$(RUST_TEST) --locked -p terlan --test tvm_transition_rejection
	@test ! -e crates/terlan/src/runtime/vm_part_001.rs
	@if rg -n 'struct TerlanVm|impl TerlanVm|runtime::vm::TerlanVm|fn evaluate_repl_function|fn apply_closure' crates/terlan/src/runtime crates/terlan/src/commands --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.legacy_runtime_present]: runtime CoreIR execution code is forbidden after the AOT cutover'; \
		exit 1; \
	fi
	@if rg -n 'execute_request_invocation_with|beam_eval|Command::new|TerlanVm|evaluate_[A-Za-z0-9_]*|apply_closure' \
		crates/terlan/src/commands/serve/handler.rs \
		crates/terlan/src/commands/serve/handler_cache.rs \
		crates/terlan/src/commands/serve/handler_cache/invocation.rs; then \
		echo 'error[aot.http_fallback_present]: HTTP serving must not retain an evaluator, host command, or synchronous wake-injection path'; \
		exit 1; \
	fi
	@if rg -n '^\| .* \| temporary-migration-support \|' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md; then \
		echo 'error[aot.temporary_migration_support]: active AOT inventory cannot retain temporary migration rows'; \
		exit 1; \
	fi
	@if rg -n '^\| .* \| deletion-debt \|' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md; then \
		echo 'error[aot.deletion_debt]: active AOT inventory cannot retain deletion-debt rows'; \
		exit 1; \
	fi
	@if rg -n 'run-fallback|worker directly|eventual adapter|replacing compiled payload publication' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md; then \
		echo 'error[aot.inventory_stale]: named-consumer inventory still describes a retired fallback or migration owner'; \
		exit 1; \
	fi
	@for path in \
		crates/terlan/src/commands/build/args.rs \
		crates/terlan/src/commands/run/mod.rs \
		crates/terlan/src/commands/test/vm_runner.rs \
		crates/terlan/src/commands/repl/mod.rs \
		crates/terlan/src/commands/serve/handler_cache.rs \
		crates/terlan/src/commands/debug/session.rs \
		crates/terlan/src/commands/vm/native_reload.rs; do \
		rg -Fq "$$path" docs/runtime/TVM_AOT_PIVOT_INVENTORY.md || { \
			echo "error[aot.inventory_missing]: named native consumer $$path is absent from the AOT inventory"; \
			exit 1; \
		}; \
	done
	@rg -q 'commands/run/mod.rs.*native-image-consumer.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'commands/test/vm_runner.rs.*native-image-consumer.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'commands/repl/mod.rs.*native-image-consumer.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'commands/serve/handler_cache.rs.*http-handler-path.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'commands/debug/session.rs.*debugger-path.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'commands/vm/native_reload.rs.*hot-reload-path.*reusable-runtime-semantics' docs/runtime/TVM_AOT_PIVOT_INVENTORY.md
	@rg -q 'image: PureNativeExecutionImage' crates/terlan/src/commands/serve/handler_cache.rs
	@rg -q 'begin_request_invocation' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@rg -q 'execute_immediate_native' crates/terlan/src/commands/serve/handler_cache.rs
	@rg -q 'error\[vm\.aot_required\]' crates/terlan/src/vm/main_part_004.rs

.PHONY: tvm-aot-shard-ownership-check
tvm-aot-shard-ownership-check: tvm-direct-aot-backend-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::execution_shard_protocol::execution_shard_protocol_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_shard_service_isolation_test::actor_runtime_services_and_image_generations_are_shard_local -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::code_server::code_parallel_load_beam_suite_parity_test::code_parallel_load_suite_shard_local_workers_switch_without_global_lock -- --exact
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/vm/main/native_image_runner.rs
	@rg -q 'code_server: VmCodeServer' crates/terlan/src/runtime/vm/actor_impl.rs
	@rg -q 'release_process_bindings' crates/terlan/src/runtime/vm/actor_exit.rs crates/terlan/src/runtime/vm/code_server.rs
	@if rg -n 'Mutex|RwLock|OnceLock|LazyLock|Arc<Mutex' \
		crates/terlan/src/runtime/vm/actor_impl.rs \
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
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_admission_and_shutdown_follow_supervisor_lifecycle -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_replacement_drains_and_publishes_the_next_epoch -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_crash_recovery_rejects_early_restart_and_stale_execution -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::repl::repl_aot_test::float_repl_generation_executes_without_resident_core_ir -- --exact
	@rg -q 'VmRestartBackoffSchedule' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@rg -Fq 'matches!(self.phase, VmShardPhase::Ready)' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@rg -q 'supervisor: VmExecutionShardSupervisor' crates/terlan/src/runtime/vm/pure_native/execution_shard.rs
	@rg -q 'active\.shard\.replace_image\(&native_image\)' crates/terlan/src/commands/repl/mod_part_002.rs
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
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::execution_shard_epoch::execution_shard_epoch_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::execution_shard_supervisor::execution_shard_supervisor_test::supervisor_rejects_stale_operations_and_suppresses_uncertain_recovery -- --exact
	@rg -q 'VmShardOperationKind::ALL' crates/terlan/src/runtime/vm/execution_shard_epoch_test.rs
	@rg -q 'pub\(crate\) replay_policy: VmShardReplayPolicy' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@rg -q 'IndeterminateSuppressed' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@rg -q 'DuplicateSuppressed' crates/terlan/src/runtime/vm/execution_shard_epoch.rs
	@if rg -n 'epoch:[[:space:]]*u64' crates/terlan/src/runtime/vm/execution_shard_epoch.rs; then \
		echo 'error[aot.stale_epoch]: shard operations must use the canonical typed epoch'; \
		exit 1; \
	fi

tvm-aot-crash-injection-check: tvm-aot-stale-epoch-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::execution_shard_supervisor::execution_shard_fault_injection_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::actor_exit_releases_native_continuation_ownership -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_send_transition_delivers_before_exact_owner_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::capability_worker::capability_worker_test::capability_worker_reply_completes_live_vm_deadline -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::capability_worker::capability_worker_test::capability_worker_cancellation_wins_over_late_reply -- --exact
	@rg -q 'const ALL: \[Self; 16\]' crates/terlan/src/runtime/vm/execution_shard_fault_injection_test.rs
	@rg -q 'pub\(crate\) shard_id: VmExecutionShardId' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs
	@if rg -n 'CrashBoundary' crates/terlan/src/runtime/vm/execution_shard_supervisor.rs crates/terlan/src/runtime/vm/execution_shard_epoch.rs crates/terlan/src/runtime/vm/execution_shard_protocol.rs; then \
		echo 'error[aot.crash_injection]: crash injection must remain test-only'; \
		exit 1; \
	fi

tvm-aot-runtime-transition-check: tvm-aot-runtime-transition-focused-check

tvm-aot-runtime-transition-focused-check: tvm-native-image-format-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::local_actor_entry_and_resume_never_use_worker_transport -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::rejected_entry_releases_its_local_actor -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::foreign_resume_owner_cannot_consume_or_fail_another_actor -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::resume_failure_propagates_and_releases_all_direct_path_ownership -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::direct_backend::direct_backend_test::execution_runtime_interleaves_owner_scoped_continuations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_continuation_resume_requires_exact_process_request_and_continuation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_continuation_parking_rejects_duplicate_and_zero_identities -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::actor_exit_releases_native_continuation_ownership -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_send_transition_delivers_before_exact_owner_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_send_transition_rejects_invalid_ownership_without_mailbox_mutation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_receive_transition_consumes_typed_mailbox_value_before_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_receive_transition_retains_lease_and_nonmatching_mailbox_values -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_spawn_transition_creates_scheduled_child_before_parent_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_spawn_transition_rejects_invalid_ownership_without_child_creation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_timer_transition_fires_before_exact_owner_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_timer_transition_rejects_invalid_ownership_without_wakeup -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_link_transition_creates_failure_relationship_before_owner_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_link_transition_rejects_invalid_ownership_without_relationship_mutation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_monitor_transition_allocates_reference_before_down_delivery -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_monitor_transition_rejects_missing_targets_before_reference_allocation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_resource_transition_registers_owned_handle_and_cleans_up_on_exit -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_resource_transition_rejects_invalid_authority_before_allocation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_cancellation_records_target_before_resuming_exact_owner -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_cancellation_rejects_invalid_authority_before_target_mutation -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_test::native_self_cancellation_wins_before_resume_boundary -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_native_failure_test::native_failure_uses_vm_exit_propagation_monitoring_and_cleanup -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_native_failure_test::native_failure_rejects_invalid_authority_and_code_before_exit -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_native_scheduling_test::native_scheduling_reclassifies_owner_before_exact_resume -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor::actor_native_scheduling_test::native_scheduling_rejects_foreign_owner_without_reclassification -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_send_transition_dispatches_through_vm_mailbox_ownership -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_receive_transition_dispatches_typed_mailbox_result -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_spawn_transition_dispatches_vm_owned_child_identity -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_timer_transition_dispatches_vm_owned_deadline -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_link_transition_dispatches_vm_owned_failure_relationship -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_monitor_transition_dispatches_vm_owned_reference -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_resource_transition_dispatches_vm_owned_identity -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_cancellation_transition_dispatches_scheduler_owned_request -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_failure_transition_dispatches_vm_owned_abnormal_exit -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_scheduling_transition_dispatches_vm_owned_reclassification -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_accepts_active_typed_operations -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_send_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_receive_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_spawn_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_timer_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_link_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_monitor_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_resource_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_cancellation_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_failure_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm main_test::native_transition_test::native_transition_argument_contract_rejects_malformed_scheduling_before_parking -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::typeck::core_intrinsic_test::syntax_output_lowering_canonicalizes_process_send_int_transition -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_process_receive_int_transition -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-vm compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_erases_typed_lifecycle_descriptors -- --exact
	$(RUST_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact

vm-native-worker-runtime-check: terlan-vm-artifact-format-check stdlib-native-artifacts-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-native-worker-runtime

vm-io-reactor-runtime-check: vm-native-worker-runtime-check no-default-tokio-runtime-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-io-reactor-runtime

vm-supervision-restart-check: vm-supervision-primitives-check vm-timer-deadline-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_relationship_test::actor_unlinked_child_termination_preserves_parent_mailbox_progress -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_waits_for_clean_exit_and_cancels_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_deadline_forces_typed_exit_and_restarts_child -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_normal_exit_honors_transient_restart_class -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_supervision_restart_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-supervision-restart

vm-http-handler-dispatch-check: vm-io-reactor-runtime-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-handler-dispatch

vm-http-handler-scheduler-fairness-check: vm-http-handler-dispatch-check
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 25 --concurrency 4 --queue-capacity 8
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 1 --handler-delay-ms 2
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 4 --queue-capacity 4 --requests-per-connection 4
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 24 --concurrency 4 --queue-capacity 4 --request-mix crud --payload-bytes 512
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 16 --concurrency 4 --queue-capacity 4 --request-mix large-static --payload-bytes 4096
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 32 --concurrency 8 --queue-capacity 8 --request-mix slow-client
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 24 --concurrency 6 --queue-capacity 4 --request-mix streaming
	TERLAN_VM_HTTP_SOCKET_ALLOW_SKIP=1 $(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-http-socket --iterations 25 --concurrency 5 --queue-capacity 5 --request-mix synthetic-handlers
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-handler-scheduler-fairness

vm-http-stateful-actor-session-check: vm-http-handler-scheduler-fairness-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-stateful-actor-session

vm-live-template-stream-check: \
	vm-http-stateful-actor-session-check \
	vm-http-sse-check \
	vm-http-websocket-source-check \
	vm-http-websocket-queue-check \
	vm-http-websocket-termination-check \
	vm-http-live-channel-source-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-live-template-stream

vm-live-template-client-protocol-check: \
	vm-live-template-stream-check \
	angular-ts-terlan-integration-check \
	angular-ts-namespace-generation-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-live-template-client-protocol

typed-template-render-mode-check: vm-live-template-client-protocol-check typed-template-interpolation-check
	node editors/vscode/test/template_links_test.js
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- typed-template-render-mode

web-asset-pipeline-check: \
	typed-template-render-mode-check \
	browser-package-preflight \
	web-profile-preflight
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- web-asset-pipeline

vm-web-security-policy-check: \
	web-asset-pipeline-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-security-policy

vm-web-config-secret-boundary-check: \
	vm-web-security-policy-check \
	http-tls-check \
	web-compose-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-config-secret-boundary

vm-web-observability-check: \
	vm-web-config-secret-boundary-check \
	http-observability-check \
	vm-diagnostics-quality-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-observability

vm-web-lifecycle-health-check: \
	vm-web-observability-check \
	web-compose-check \
	http-tls-check \
	vm-source-hot-reload-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-lifecycle-health

vm-web-deployment-profile-check: \
	vm-web-lifecycle-health-check \
	http-router-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-deployment-profile

vm-web-route-schema-client-check: \
	vm-web-deployment-profile-check \
	api-schema-check \
	web-profile-preflight
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-web-route-schema-client

vm-model-sync-store-check: \
	vm-web-route-schema-client-check \
	native-boundary-postgres-check \
	db-command-check
	$(TERLC) test std/vm/ModelSyncTest.terl
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-model-sync-store

vm-persistent-actor-store-check: \
	vm-model-sync-store-check \
	vm-process-model-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check \
	vm-distributed-transport-check
	$(TERLC) check std/vm/PersistentActorTest.terl
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-store

vm-persistent-actor-schema-check: \
	vm-persistent-actor-store-check \
	vm-distributed-state-check \
	vm-distributed-transport-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-schema

vm-persistent-actor-compaction-check: \
	vm-persistent-actor-schema-check \
	vm-distributed-state-check \
	vm-resource-ownership-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-compaction

vm-persistent-actor-restore-check: \
	vm-persistent-actor-compaction-check \
	vm-distributed-state-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-restore

vm-persistent-actor-adapter-conformance-check: \
	vm-persistent-actor-restore-check \
	vm-distributed-state-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-adapter

vm-persistent-actor-performance-budget-check: vm-persistent-actor-adapter-conformance-check
	TERLAN_BENCH_PERSISTENT_ACTOR_OUTPUT=target/quality/vm-persistent-actor-benchmark.json $(CARGO) run --locked -p terlan --bin terlan-benchmark -- vm-persistent-actor-runtime-baseline
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-performance

vm-persistent-actor-telemetry-check: vm-persistent-actor-performance-budget-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-telemetry

vm-persistent-actor-policy-check: vm-persistent-actor-telemetry-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-policy

vm-http-acme-tls-production-check: vm-timer-deadline-check http-tls-check

vm-http-acme-worker-migration-check: vm-http-acme-tls-production-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-worker

vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-cache-custody

vm-http-acme-renewal-rotation-check: vm-http-acme-cache-custody-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-acme-renewal

terlan-vm-run-command-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::validate_run_args_defaults_to_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::validate_run_args_accepts_vm_and_rejects_erlang_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::validate_run_args_rejects_unsupported_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::build_command_for_run_appends_default_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::build_command_for_run_preserves_explicit_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::find_single_vm_artifact_accepts_one_artifact -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::find_single_vm_artifact_rejects_multiple_artifacts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::run::run_test::run_built_vm_artifact_executes_vm_runner -- --exact

terlan-vm-test-command-check:
	$(PYTHON) tools/check_vm_cli_exact_selector_surface.py
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::parse_test_args_accepts_default_terlan_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::parse_test_args_defaults_to_tests_directory -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::parse_test_args_rejects_explicit_erlang_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::parse_test_args_accepts_explicit_terlan_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_executes_bool_test -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::run_test_defaults_to_terlan_vm_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::run_project_directory_tests_default_to_vm_and_prepare_source_roots -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_fails_false_bool_test -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::test::test_command_test::run_terlan_vm_tests_writes_runtime_manifests -- --exact

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
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_actor_poll_parks_then_wakes_through_tcp_scheduler_adapter -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_actor_poll_rejects_missing_and_exited_handler_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_accepts_tcp_stream_into_handler_process_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_finishes_tcp_handler_by_closing_stream_and_exiting_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_finishes_cancelled_tcp_handler_with_error_reason -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_polls_runnable_handlers_and_skips_parked_handlers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_reuses_handler_for_pipelined_requests -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_parks_idle_handler_and_wakes_on_later_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_accept_limit_bounds_accept_work_per_poll -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_limit_bounds_handler_work_per_poll -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_handler_budget_uses_round_robin_cursor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_honors_connection_close_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_reports_half_closed_truncated_body -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_reports_half_closed_partial_headers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_cancels_parked_handler_and_closes_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_rejects_invalid_transitions_without_closing_listener -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_completes_woken_handler_before_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_forces_parked_handler_at_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_does_not_count_cancellation_as_completion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_test::vm_http_tcp_server_tls_drain_removes_plan_only_after_terminal_tick -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_hooks_test::vm_http_lifecycle_hook_observes_ordered_worker_request_channel_and_shutdown_events -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_hooks_test::vm_http_lifecycle_hook_rejects_request_before_handler_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_hooks_test::vm_http_lifecycle_hook_can_reject_drain_without_closing_listener -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::lifecycle_hooks_test::vm_http_lifecycle_hook_channel_rejection_rolls_back_process_and_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::source_lifecycle_test::vm_http_router_source_lifecycle_rejects_request_before_handler_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::source_lifecycle_test::vm_http_router_source_lifecycle_cannot_veto_channel_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::source_lifecycle_test::vm_http_router_source_lifecycle_rejects_invalid_descriptor_ownership -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_queue_overload_policies_preserve_full_queue_and_work_ownership -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_queue_overload_policies_enqueue_when_capacity_is_available -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_server_queue_policy_backpressures_at_the_listener_bound -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_server_reject_policy_closes_saturated_work_without_leaking_a_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_server_spill_policy_reports_fallback_admission -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::overload_test::vm_http_server_saturation_stress_preserves_policy_accounting_and_cleanup -- --exact
	@$(TERLC) test std/http/RouterTest.terl
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_shutdown_closes_listener_and_active_handlers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_inspects_listener_pressure_and_handler_counters -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_propagates_handler_errors_without_finishing_handler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_reports_missing_retained_handler_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_cancel_adjusts_round_robin_cursor_edges -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_shutdown_with_tls_removes_listener_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_reports_plaintext_transport_mode -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_reports_tls_transport_mode -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_reports_missing_transport_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_poll_with_tls_allows_plaintext_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_poll_with_tls_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_allows_plaintext_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_honors_connection_close_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_accept_limit_preserves_accept_budget -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_accept_limit_handles_encrypted_transport -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_limits_preserves_scheduler_budgets -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_tcp_server_keep_alive_with_tls_limits_handles_encrypted_transport -- --exact

terlan-vm-http-lane-check: \
	vm-tcp-framing-check \
	vm-http-stream-serve-check \
	vm-http-router-middleware-check \
	http-session-actor-check \
	vm-http-sse-check \
	vm-http-live-channel-source-check

vm-http-stream-serve-check:
	$(RUST_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::build_test::tests::js_target_diagnostics_test::build_command_rejects_function_head_pattern_for_js_target -- --exact

vm-http-in-memory-transport-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test

vm-http-router-middleware-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_router::route_concurrency_test::vm_http_router_middleware_bounded_concurrency_smoke -- --exact
	$(RUST_TEST) -p terlan --bin terlc commands::build::js_browser::js_browser_test::discover_web_handlers_rejects_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::js_browser::js_browser_test::write_browser_package_serializes_constant_handlers_as_static_responses -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler::handler_test::validate_static_response_requires_complete_router_owner_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::js_browser::js_browser_test::discover_web_handlers_from_modules_extracts_grouped_router_builder_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_prefers_dynamic_handler_over_file_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_prefers_dynamic_handler_over_static_response_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_http_router_error_boundary_recovers_handler_failure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_http_router_error_boundary_reports_recovery_failure -- --exact
	@$(TERLC) test std/http/RouterTest.terl

vm-http-sse-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::sse::sse_test

vm-http-websocket-source-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_adapter_frame_constructors_build_typed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_adapter_endpoint_validates_channel_limits -- --exact
	@$(TERLC) test std/http/WebSocketTest.terl

vm-http-websocket-upgrade-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::js_browser::js_browser_test::discover_web_route_manifest_extracts_websocket_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler::handler_test::validate_websocket_requires_source_for_router_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_request_returns_websocket_upgrade_handshake_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_websocket_upgrade_activates_materialized_router_middleware -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_accept_upgrade_binds_stream_and_endpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_accept_upgrade_rejects_blank_key_without_session -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_accept_upgrade_rejects_inactive_stream_without_session -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_accept_upgrade_rejects_duplicate_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_upgrade_response_serializes_http1_switching_protocols -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_upgrade_response_serialization_rejects_invalid_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_send_upgrade_response_writes_to_peer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_send_upgrade_response_rejects_closed_stream -- --exact

vm-http-websocket-queue-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_endpoint_opens_bounded_inbound_queue -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_inbound_queue_preserves_order_and_pressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_inbound_queue_rejects_full_and_oversized_frames -- --exact

vm-http-websocket-policy-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_endpoint_declares_binary_payload_rejection_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_decode_client_frame_rejects_binary_frame -- --exact

vm-http-websocket-tls-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_send_tls_upgrade_response_writes_to_peer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_send_tls_upgrade_response_rejects_stream_mismatch -- --exact

vm-http-websocket-termination-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_timeout_termination_sends_close_and_reason -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::websocket::websocket_test::vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame -- --exact

vm-http-live-channel-source-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::js_browser::js_browser_test::discover_web_route_manifest_extracts_grouped_sse_routes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::validate_web_package_rejects_sse_route_conflicting_with_http_handler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	@$(TERLC) test std/http/RouterTest.terl
	@$(TERLC) test std/http/SseTest.terl
	@$(TERLC) test std/http/WebSocketTest.terl
	@$(TERLC) test std/http/LiveChannelTest.terl

vm-in-memory-stream-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reads_writes_and_closes_raw_streams -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_preserves_partial_exact_frame_across_polls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_raw_read_drains_staged_bytes_first -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reads_delimited_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reads_fragmented_length_prefixed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_writes_length_prefixed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_eof_for_half_closed_partial_frame -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_timeout_for_pending_exact_read -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_cancelled_streams -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_rejects_bounded_buffer_overflow -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_backpressure_from_peer_inbox -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::framing::framing_test::vm_framing_fixture_reports_closed_reader_stream -- --exact
	$(CARGO) run -p terlan --bin terlan-vm --quiet -- benchmark-in-memory-framing --iterations 100 --payload-bytes 128 >/tmp/terlan-vm-in-memory-framing-benchmark.json

vm-tcp-framing-check: vm-in-memory-stream-check

vm-http-static-streaming-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_table_infers_content_type_and_cache_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_table_marks_fingerprinted_assets_immutable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_table_preserves_manifest_overrides -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_table_rejects_invalid_manifest_entries -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_table_rolls_back_failed_manifest_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_modes_are_explicit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_stream_plan_requires_bounded_nonzero_limits -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_converts_text_and_binary_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_converts_static_asset_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_asset_emits_typed_byte_range_responses -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_static_asset_clamps_and_rejects_adversarial_ranges -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_converts_sse_events_to_http_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_rejects_invalid_sse_event_stream -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http_static_test::vm_http_response_body_rejects_stream_conversion_until_emitter_exists -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_splits_and_partially_flushes_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_applies_atomic_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_finishes_and_aborts_with_stable_states -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_flushes_to_tcp_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_parks_and_retries_tcp_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::stream_test::vm_http_response_stream_aborts_on_terminal_tcp_failures -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_writes_head_chunks_and_end_in_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_preserves_chunk_during_tcp_backpressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_static::http1_stream_test::vm_http1_response_stream_rejects_invalid_metadata_and_terminal_races -- --exact

vm-http-queue-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test

vm-tcp-stream-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::core_intrinsic_test::core_primitive_intrinsic_resolves_vm_library_primitives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::core_intrinsic_test::vm_library_primitive_registry_keys_are_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::core_intrinsic_test::vm_library_primitive_return_types_are_registered -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::core_intrinsic_test::core_primitive_intrinsic_rejects_wrong_vm_primitive_arities -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_accepts_streams_and_moves_bytes_between_peers -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_preserves_accept_order_and_splits_large_receives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_applies_listener_backlog_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_inspects_listener_backlog_waiters_and_closed_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_write_half_close_blocks_sender_but_allows_peer_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_applies_stream_inbox_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_send_and_reports_wakeup_when_peer_drains_capacity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_rejects_closed_cancelled_and_invalid_resources -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_rejects_zero_receive_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_accept_and_reports_wakeup_when_connection_arrives -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp::tcp_test::tcp_runtime_parks_receive_and_reports_wakeup_when_bytes_arrive -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_accept_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_read_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_wakes_blocked_write_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::tcp_scheduler::tcp_scheduler_test::tcp_scheduler_adapter_reports_missing_and_exited_wake_targets -- --exact


http-session-actor-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_lookup_creates_actor_and_sticky_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_reuses_actor_and_table_state_for_cookie_lookup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_rotate_changes_cookie_without_losing_actor_state -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_expiration_cleans_actor_table_and_reports_stale -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_recovery_policy_can_fail_closed_for_stale_cookie -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test::http_session_rejects_invalid_runtime_configuration -- --exact
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::http_session::http_session_test

vm-process-model-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::process
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_send_test::actor_custom_name_registry_preserves_via_registration_routing_and_cleanup_semantics -- --exact

vm-scheduler-contract-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler

vm-actor-mutator-ownership-check:
	cargo check --locked -p terlan --bin terlan-vm
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor_directory::actor_directory_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_runs_runnable_process_and_requeues_yielded_slice -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact

vm-multicore-mailbox-publication-check: vm-process-model-check vm-failure-primitives-check
	cargo check --locked -p terlan --bin terlan-vm
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor_directory::mailbox::mailbox_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor_directory::actor_directory_test::publication_during_execution_prevents_lost_wakeup_park -- --exact
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@! rg -q 'ConcurrentQueue::unbounded\(\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_signal_beam_suite_parity_test::signal_suite_contended_enqueue_inspection_and_single_wakeup_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_signal_beam_suite_parity_test::signal_suite_message_before_down_order_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::failure::failure_erl_link_parity_test::erl_link_suite_portable_link_monitor_race_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::process::process_registry_test::process_registry_exit_removes_every_name_before_reuse -- --exact

vm-multicore-fixed-placement-check: vm-multicore-mailbox-publication-check vm-scheduler-fairness-check rust-quality-check
	cargo check --locked -p terlan --bin terlan-vm
	cargo check --locked -p terlan --bin terlc
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_telemetry::fixed_scheduler_telemetry_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::shard_owner::shard_owner_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test

tvm-aot-multicore-migration-check: tvm-aot-runtime-transition-check tvm-managed-memory-check vm-actor-mutator-ownership-check rust-quality-check
	cargo check --locked -p terlan --bin terlan-vm
	cargo check --locked -p terlan --bin terlc
	$(RUST_TEST) --locked -p terlan --bin terlan-vm transfer_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact

vm-multicore-work-stealing-policy-check:
	cargo check --locked -p terlan --bin terlan-vm
	cargo check --locked -p terlan --bin terlc
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::work_stealing::work_stealing_test

vm-multicore-work-stealing-check: vm-multicore-work-stealing-policy-check tvm-aot-multicore-migration-check vm-scheduler-fairness-check rust-quality-check
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::shard_owner::shard_owner_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_yields_requeue_before_each_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::resumed_generated_aot_actor_yields_before_replying -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test::queued_actor_migration_publishes_destination_before_reacquisition -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_runnable_actor_is_stolen_between_scheduler_owners -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::stolen_generated_actor_retains_destination_route_when_it_parks -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::rejected_generated_runnable_steal_rolls_back_without_actor_loss -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_runnable_classes_receive_weighted_local_service -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_multicore_fanout_completes_under_adversarial_class_skew -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_runnable_shutdown_reclaims_queued_class_work -- --exact

vm-multicore-runtime-cleanup-check:
	cargo check --locked -p terlan --bin terlan-vm
	cargo check --locked -p terlan --bin terlc
	@test ! -e crates/terlan/src/runtime/vm/work_stealing/runtime.rs
	@test ! -e crates/terlan/src/runtime/vm/scheduler/steal.rs
	@test ! -e crates/terlan/src/runtime/vm/actor_directory/steal.rs
	@! rg -q 'VmWorkStealingRuntime|VmSchedulerStealClaim|VmActorStealClaim' crates/terlan/src
	@! rg -q '^(vm-multicore-work-stealing-owner-check|tvm-aot-multicore-yield-queue-check|tvm-aot-multicore-runnable-steal-check|tvm-aot-multicore-policy-coordination-check):' Makefile
	@! rg -q 'hidden MC-5|next MC-5|staged MC-5|Activated by the MC-6|Used when MC-6' crates/terlan/src/runtime/vm crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache

tvm-aot-multicore-io-epoch-check: vm-multicore-work-stealing-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::stale_io_completion_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact

vm-multicore-timer-epoch-check: tvm-aot-multicore-io-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::timer_ingress_test::current_timer_tick_delivers_once_through_shard_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::timer_ingress_test::foreign_shard_timer_tick_fails_before_timer_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::timer_ingress_test::stale_timer_tick_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_runtime_transfer_test::actor_runtime_transfer_moves_delayed_message_with_exact_timer_deadline -- --exact

vm-multicore-timer-scheduler-check: vm-multicore-timer-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_parks_until_scheduler_owned_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_does_not_block_peer_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_is_cancelled_by_scheduler_shutdown -- --exact

vm-multicore-protocol-reactor-check: vm-multicore-timer-scheduler-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::protocol_task_executor::protocol_task_executor_test::protocol_completion_origin_rejects_foreign_and_ambient_threads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::handler_cache_generation_test::immediate_callback_executes_on_its_protocol_owner_without_rpc -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_completion_resumes_only_through_actor_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_rejects_same_scheduler_foreign_connection -- --exact

vm-multicore-capability-worker-check: vm-multicore-protocol-reactor-check tvm-aot-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_enforces_bounded_non_reentrant_admission -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_duplicate_completion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_cancellation_releases_exact_request_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_replaces_crashed_slot_without_capacity_bypass -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_rejects_identity_and_capability_bypass -- --exact

vm-multicore-capability-completion-check: vm-multicore-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_completes_an_already_parked_generated_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_late_already_parked_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_capability_completion_is_published_before_owner_dispatch -- --exact
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@if rg -n 'capability_dispatch_missing' crates/terlan/src/commands/serve/handler_cache/shard_owner; then \
		echo 'error[multicore.capability]: generated capability suspensions must not be rejected'; \
		exit 1; \
	fi

vm-multicore-capability-event-pump-check: vm-multicore-capability-completion-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_event_pump_correlates_completion_with_fixed_owner_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_event_pump_returns_payload_on_backpressure_and_cancellation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_event_pump_drains_generation_payloads_on_worker_loss -- --exact
	@rg -q 'VmCapabilityWorkerEventPump' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs
	@if rg -n 'HashMap|mpsc::channel\(\)' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs; then \
		echo 'error[multicore.capability_event_pump]: correlation must remain deterministic and bounded by worker credits'; \
		exit 1; \
	fi

vm-multicore-capability-scheduler-check: vm-multicore-capability-event-pump-check
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::capability_worker::pool::pool_test::capability_event_pump_shutdown_returns_all_pending_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-native-worker protocol::protocol_test::worker_admits_declared_filesystem_operation -- --exact
	$(CARGO) build --locked -p terlan --bin terlan-native-worker
	TERLAN_NATIVE_WORKER=$(CURDIR)/target/debug/terlan-native-worker TERLAN_TEST_AOT_CAPABILITY_PUMP=1 TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX=1 $(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_capability_event_pump_executes_real_worker_full_cycle -- --ignored --exact
	@rg -q 'GeneratedCapabilityDispatcher' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs

vm-epmd-discovery-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::epmd_protocol_round_trips_alive2_and_rejects_malformed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::epmd_registry_owns_registration_until_exact_connection_closes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::logical_node_registers_only_after_pool_listener_and_router_are_ready -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::one_logical_registration_survives_scheduler_owner_migration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::node_shutdown_closes_admission_before_unregistering -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::fixed_scheduler_connection_handler_owns_alive_registration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::fixed_scheduler_connection_handler_rejects_bad_alive_name_without_registration -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::logical_node_router_publishes_to_current_actor_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::logical_node_transport_frame_is_bounded_and_actor_addressed -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::epmd::epmd_test::logical_node_bootstrap_runs_discovery_transport_and_shutdown_full_cycle -- --ignored --exact
	@rg -q 'serve_protocol_tasks' crates/terlan/src/runtime/vm/epmd/transport.rs
	@rg -q 'start_protocol_tasks_with_topology' crates/terlan/src/runtime/vm/epmd/bootstrap.rs
	@rg -q 'resolve_route' crates/terlan/src/runtime/vm/epmd/node_transport.rs crates/terlan/src/runtime/vm/fixed_scheduler_control.rs

vm-multicore-runtime-integration-check: vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-timer-deadline-check native-boundary-runtime-adversarial-check vm-http-concurrency-investigation-check rust-quality-check

vm-multicore-replay-observability-check: rust-quality-check
	cargo check --locked -p terlan --bin terlan-vm
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::multicore_replay::multicore_replay_test
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_telemetry::fixed_scheduler_telemetry_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::shard_owner::shard_owner_test::panic_detail_is_bounded_and_stable_for_all_payload_classes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::support_bundle::support_bundle_test::native_support_bundle_serializes_validated_multicore_evidence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_yields_requeue_before_each_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_runnable_actor_is_stolen_between_scheduler_owners -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_parks_until_scheduler_owned_deadline -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::vm::debugger_control::debugger_control_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::debugger_pause_and_step_follow_owner_migration_without_duplicate_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::detached_actor_generation_blocks_source_reload_and_rejects_replaced_destination -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_crash_recovery_rejects_early_restart_and_stale_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::pure_native::execution_shard::execution_shard_test::orderly_shard_shutdown_records_one_generation_qualified_lifecycle -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::generated_runnable_shutdown_reclaims_queued_class_work -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::serve::handler_cache::invocation::invocation_test::scheduler_panic_fails_the_whole_handler_generation_closed -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
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
	$(RUST_TEST) --locked -p terlan --bin terlan-vm runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) --locked -p terlan --bin terlc commands::serve::handler_cache::multicore_performance_test -- --nocapture
	test -s benchmarks/baselines/vm-multicore-performance-limits.json
	TERLAN_VM_MULTICORE_PERFORMANCE_OUTPUT=$(CURDIR)/target/quality/vm-multicore-performance.json $(RUST_TEST) --locked --release -p terlan --bin terlc commands::serve::handler_cache::multicore_performance_test::multicore_runtime_width_matrix_records_workloads_and_owner_overlap -- --ignored --exact --nocapture
	test -s target/quality/vm-multicore-performance.json
	@rg -q '"schema": "terlan.vm-multicore-performance.v1"' target/quality/vm-multicore-performance.json
	@rg -q '"effective_parallelism":' target/quality/vm-multicore-performance.json
	@rg -q '"maximum_simultaneously_active_schedulers":' target/quality/vm-multicore-performance.json
	@rg -q '"runtime_workload_contract_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"mixed_tail_contract_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"performance_policy_sha256":' target/quality/vm-multicore-performance.json
	@rg -q '"source_revision":' target/quality/vm-multicore-performance.json
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
	@for width in 1 2 4; do \
		rg -q "\"requested_schedulers\": $$width" target/quality/vm-multicore-performance.json || exit 1; \
	done
	@for workload in actor_spawn_exit mailbox_round_trip timer_delivery http_handler_response supervision_restart epmd_registration_lifecycle; do \
		rg -q "\"workload\": \"$$workload\"" target/quality/vm-multicore-performance.json || exit 1; \
	done
	@for metric in scheduler_wait mailbox_delivery timer_delay http_latency failed_steal_backoff allocation_pause collection_pause; do \
		rg -q "\"metric\": \"$$metric\"" target/quality/vm-multicore-performance.json || exit 1; \
	done

vm-multicore-memory-model-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlan-vm runtime::vm::pure_native::multicore_model_test
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlan-vm runtime::vm::actor_directory::mailbox::mailbox_test::seeded_mailbox_flood_preserves_every_sender_under_forced_interleaving -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlan-vm runtime::vm::work_stealing::work_stealing_test::seeded_skew_burst_and_fanout_decisions_remain_bounded_and_work_conserving -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::deadlock_watchdog_terminates_stuck_child -- --exact
	TERLAN_VM_MULTICORE_STRESS_OUTPUT=$(CURDIR)/target/quality/vm-multicore-memory-model.json $(EXACT_CARGO_TEST) --locked -p terlan --bin terlan-vm runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::bounded_seeded_multicore_memory_model_has_deadlock_watchdog -- --exact --nocapture
	test -s target/quality/vm-multicore-memory-model.json
	@rg -q '"schema": "terlan.vm-multicore-memory-model.v1"' target/quality/vm-multicore-memory-model.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-memory-model.json
	@rg -q '"watchdog_timeout_millis": 15000' target/quality/vm-multicore-memory-model.json
	@test "$$(rg -c '0x[0-9a-f]{16}' target/quality/vm-multicore-memory-model.json)" -eq 8

vm-multicore-thread-sanitizer-contract-check:
	$(PYTHON) tools/check_vm_multicore_thread_sanitizer.py self-test

vm-multicore-thread-sanitizer-check: vm-multicore-memory-model-check vm-multicore-thread-sanitizer-contract-check
	@if rustup target list --installed --toolchain 1.96.0 2>/dev/null | rg -qx 'x86_64-unknown-linux-gnutsan'; then \
		$(PYTHON) tools/check_vm_multicore_thread_sanitizer.py run; \
	elif test "$${GITHUB_ACTIONS:-}" = true; then \
		echo 'error[vm.multicore.tsan]: pinned Rust 1.96.0 ThreadSanitizer target is mandatory in CI'; \
		exit 1; \
	else \
		echo 'VM multicore ThreadSanitizer target unavailable locally; portable memory-model gate passed'; \
	fi

vm-multicore-mc9-evidence-contract-check:
	$(PYTHON) tools/check_vm_multicore_mc9_evidence.py self-test

vm-multicore-mc9-evidence-check: vm-multicore-mc9-evidence-contract-check
	test -s target/quality/vm-multicore-performance.json
	test -s target/quality/vm-multicore-thread-sanitizer-report.json
	$(PYTHON) tools/check_vm_multicore_mc9_evidence.py seal
	test -s target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"schema": "terlan.vm-multicore-mc9-evidence.v1"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"dedicated_runner_label": "terlan-linux-x86_64-multicore-v1"' target/quality/vm-multicore-mc9-evidence.json
	@rg -q '"sanitizer_toolchain": "1.96.0"' target/quality/vm-multicore-mc9-evidence.json

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
	rust-quality-check \
	roadmap-gate-integrity-check \
	check

vm-multicore-release-contract-check:
	$(PYTHON) tools/check_vm_multicore_release.py self-test

vm-multicore-release-check: vm-multicore-release-contract-check
	$(MAKE) vm-multicore-mc9-evidence-check
	$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)
	$(PYTHON) tools/check_vm_multicore_release.py record
	test -s target/quality/vm-multicore-release-closeout.json
	@rg -q '"schema": "terlan.vm-multicore-release-closeout.v1"' target/quality/vm-multicore-release-closeout.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json

vm-final-health-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact

vm-memory-heap-pressure-check: vm-process-model-check vm-resource-ownership-check
	test -f target/quality/vm-memory-pressure-report.json
	test -f target/quality/vm-memory-soak-report.json

vm-scheduler-fairness-check: vm-memory-heap-pressure-check vm-scheduler-contract-check
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::http::response_memory_test
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::native_boundary::deadline::deadline_test::native_boundary_deadline_charges_only_successful_parks_to_scheduler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_accounting_test::timer_table_charges_only_successful_mailbox_deliveries -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_terminal_accounting_test::scheduler_charges_terminal_reductions_only_after_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_reclassification_accounting_test::scheduler_charges_only_successful_explicit_reclassification -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_cancellation_accounting_test::scheduler_charges_only_successful_cancellation_requests -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_exit_accounting_test::actor_runtime_charges_only_newly_initiated_exit_to_exiting_actor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_checkpoint_accounting_test::actor_runtime_separates_checkpoint_operation_and_memory_reductions -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_timer_accounting_test::actor_runtime_charges_only_successful_timer_scheduling_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_timer_cancellation_accounting_test::actor_runtime_charges_only_successful_timer_cancellation_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_spawn_test::actor_spawn_charges_only_successful_child_creation_to_parent -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_relationship_accounting_test::actor_runtime_charges_only_successful_relationship_operations_to_initiator -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_registry_accounting_test::actor_runtime_charges_only_successful_registry_mutations_to_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_suspension_accounting_test::actor_runtime_charges_only_successful_suspension_operations_to_actor -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_send_accounting_test::actor_runtime_charges_only_successful_send_operations_to_sender -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_receive_accounting_test::actor_runtime_charges_receive_operations_without_charging_invalid_attempts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_fairness_telemetry_is_deterministic_under_cpu_bound_load -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_writes_fairness_report_with_starvation_evidence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_weighted_classes_preserve_order_and_bound_background_wait -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_rejects_silent_reclassification_of_queued_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test
	test -f target/quality/vm-scheduler-fairness-report.json

vm-actor-primitives-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::actor


vm-failure-primitives-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::failure
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::reference

vm-supervision-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_starts_child_and_exposes_inspection_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_only_failed_child_for_one_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_all_children_for_one_for_all_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_one_for_all_enforces_restart_limit_before_group_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_failed_and_later_children_for_rest_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_rest_for_one_enforces_restart_limit_before_group_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_temporary_child_never_restarts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_transient_child_restarts_only_after_abnormal_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_skips_non_restartable_children_without_blocking_restartable_siblings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_applies_exponential_restart_backoff_for_one_for_one_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_reports_per_child_backoff_delays -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_shutdown_timeout_for_live_child_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_group_restart_reports_per_child_shutdown_timeouts -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_enforces_restart_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_supervisor_failure_when_restart_limit_escalates -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_propagates_child_supervisor_failure_to_parent_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_restart_history_for_restart_and_limit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_records_restart_history_for_non_restartable_child -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_child_diagnostic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_diagnostic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_rejects_duplicate_child_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restart_exits_live_child_before_restarting -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_process_instead_of_panicking_on_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_for_restart_and_snapshot -- --exact

vm-timer-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_starts_one_shot_timer_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_interval_timer_fires_and_reschedules -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_coalesces_late_interval_timer_and_reschedules_after_skipped_deadlines -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_overflow_when_interval_reschedule_exceeds_tick_range -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_overflow_when_late_interval_coalescing_exceeds_tick_range -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_deadline_missed_for_late_interval_before_next_interval_boundary -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_zero_interval_timer_without_installing_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_cancels_timer_and_reports_missing_timer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_owner_exited_for_owner_timer_cleanup_in_stable_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_distinguishes_manual_cancel_from_owner_exit_cleanup -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_fires_due_timers_only_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_deadline_missed_for_late_one_shot_timer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_reports_owner_exited_if_due_timer_owner_exited_before_fire -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_fires_equal_deadlines_in_timer_id_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_receive_timeout_wakes_blocked_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_deadline_missed_receive_timeout_still_wakes_blocked_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_exited_process_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_receive_timeout_deadline_overflow_without_blocking_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_missing_process_owner -- --exact
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test

vm-timer-deadline-check:
	test -f target/quality/vm-timer-deadline-report.json

vm-resource-ownership-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_registers_resource_and_exposes_inspection_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_transfers_transferable_resource_between_live_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_releases_transferred_resource_from_new_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_owner_only_transfer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_cleanup_owner_handles_removes_live_process_handle_rows -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_owner_test::resource_table_owner_snapshots_are_ordered_isolated_and_live_only -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_wrong_owner_access_transfer_and_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_for_transfer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_for_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_missing_process_roles -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_exited_process_roles -- --exact

vm-table-primitives-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_creates_owner_table_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_inserts_replaces_looks_up_and_deletes_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_reports_missing_or_exited_processes_and_stale_handles -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_delete_returns_none_for_missing_key -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_public_read_allows_reads_but_rejects_non_owner_writes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_owner_only_rejects_non_owner_reads_and_writes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_public_read_write_allows_non_owner_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_public_read_write_allows_non_owner_delete -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_cleans_up_owner_tables_on_process_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_traversal_preserves_stable_entry_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_traversal_handles_empty_replacement_deletion_and_missing_keys -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_traversal_enforces_read_access -- --exact

vm-code-server-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_publishes_initial_generation_and_exposes_snapshot -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_inspection_test::code_server_module_scoped_inspection_excludes_unrelated_lifecycle_traffic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_inspection_test::code_server_tracks_active_coreir_function_exports_across_reload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_inspection_test::code_server_replaces_module_info_lifecycle_fixture -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_inspection_test::code_server_rejects_unloading_process_bound_module_without_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_false_dependency_test::returned_functions_do_not_leave_false_module_generation_dependencies -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_false_dependency_test::nested_calls_release_once_and_failed_entry_is_side_effect_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_failed_lifecycle_operations_are_mutation_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_hot_reload_binds_new_processes_to_new_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_release_retires_drained_old_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_hot_reload_retires_unused_previous_generation_immediately -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reports_missing_active_generation_and_missing_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reports_stale_release_binding_and_active_release_noop -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_rejects_duplicate_release_and_keeps_unique_process_bindings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_purges_retired_generations_in_generation_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reload_after_purge_keeps_generation_identity_monotonic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_purge_preserves_process_bound_retiring_generation_until_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_process_bound_reload_records_ordered_retire_and_purge_events -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_renders_generation_purge_event_without_internal_debug_shape -- --exact



vm-source-hot-reload-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_publishes_compiled_generations_and_preserves_bindings -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_publish_source_compiles_and_publishes_new_generation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_detects_changed_helper_function_body -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_rollback_validates_artifact_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_rollback_keeps_live_replaced_generation_retiring -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_reports_missing_generation_and_active_promote_noop -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::source_hot_reload_records_reload_and_rollback_events_for_inspection -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_publishes_changed_terlan_file_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_publishes_only_sources_from_mixed_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_reports_mixed_batch_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_invalid_mixed_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_report_rejects_invalid_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_unreadable_mixed_batch_without_partial_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_collapses_duplicate_source_paths_in_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_ignores_non_terlan_paths -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_reports_unreadable_terlan_source -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::source_reload::source_reload_test::source_reload_adapter_rejects_invalid_source_without_publication -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_publishes_changed_sources_through_code_server -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_ignores_assets_in_mixed_source_batch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_parses_diagnostics_flag_as_command_option -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_reports_mixed_batch_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::vm::vm_test::vm_reload_rejects_non_source_inputs -- --exact

vm-distribution-envelope-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_runtime_test::tetf_runtime_term_roundtrips_nested_portable_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_runtime_test::tetf_runtime_term_decoder_rejects_wrong_profile_atoms_and_trailing_data -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encodes_vm_refs_with_kind_node_local_id_and_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encodes_distribution_envelope_with_refs_and_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_roundtrips_canonical_runtime_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_decoder_rejects_corrupt_headers_truncation_and_trailing_bytes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_decoder_rejects_undeclared_atoms_noncanonical_bits_and_excessive_depth -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encoder_rejects_duplicate_map_keys_and_record_fields -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_payload_atoms_missing_from_manifest -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_rejects_vm_refs_with_empty_node_ids -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_rejects_vm_refs_with_zero_local_ids -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_rejects_vm_refs_with_zero_epochs_as_stale -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_empty_route_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_empty_trace_and_destination_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_zero_epoch_as_stale -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_invalid_nested_refs_before_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_runtime_only_payload_values -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_builds_tetf_distribution_envelope_with_refs -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_distributed_transport_decodes_declared_atom_payload_before_acceptance -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_distributed_transport_rejects_corrupt_or_mismatched_tetf_without_advancing -- --exact

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
	$(RUST_TEST) -p terlan --bin terlc key_compatibility_ -- --nocapture


vm-latin1-source-policy-check:
	$(RUST_TEST) -p terlan --bin terlc latin1_source_ -- --nocapture





vm-compiler-transform-retirement-check:
	$(RUST_TEST) -p terlan --bin terlc compiler_transform_retirement_ -- --nocapture

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
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_message_wakeup_is_deduplicated_and_missing_target_is_side_effect_free -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_wakeup_keeps_exactly_one_scheduler_entry -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_wakeup_preserves_suspension_until_explicit_resume -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_explicit_reclassification_moves_one_queued_entry -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_reclassifies_blocked_process_without_waking_it -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_reclassification_rejects_missing_and_exited_processes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::checksum::checksum_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::bitstring::bitstring_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::packet::packet_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native::base64::base64_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::process::process_location_test::nested_call_frames_restore_continuations_in_lifo_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_selective_receive_retries_after_matching_message_wakes_actor -- --exact

vm-diagnostics-quality-check:
	$(RUST_TEST) -p terlan --bin terlan-quality vm_diagnostics_quality_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-diagnostics-quality
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native_image::native_image_test::native_inspection_rejects_json_and_non_executables -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native_image::native_image_test::descriptor_rejects_tampering_and_noncanonical_records -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_reports_missing_and_exited_context_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id -- --exact


vm-coordination-docker-check:
	$(PYTHON) tools/check_vm_coordination_docker.py

all-terlan-tests-vm-inventory-check:
	$(PYTHON) tools/check_all_terlan_tests_vm_inventory.py

all-terlan-tests-vm-check: all-terlan-tests-vm-inventory-check terlan-vm-run-command-check terlan-vm-test-command-check flexible-shape-guards-check language-feature-coverage-100-check operator-coverage-100-check pattern-matching-support-check stdlib-release-tests-vm-default-check stdlib-release-tests

std-vm-parity-matrix-check: all-terlan-tests-vm-check

terlc-doctor-vm-pivot-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::parse_doctor_args_defaults_to_current_directory -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::parse_doctor_args_rejects_unknown_option -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_accepts_clean_vm_project -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_vm_pivot_hazards -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_vm_execution_gap_for_checked_coreir -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_summary_compiler_contract_mismatch -- --exact

mobile-target-diagnostic-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::build_test::tests::mobile_build_test::build_command_mobile_android_emits_planning_manifest -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::build_test::tests::mobile_build_test::build_command_mobile_ios_emits_planning_manifest -- --exact

mobile-shell-profile-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::init::init_test::parse_init_args_accepts_mobile_profile -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::init::init_test::write_project_mobile_profile_creates_mobile_shell_files -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::init::init_test::next_steps_for_mobile_profile_build_current_targets -- --exact

mobile-bridge-typecheck:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::mobile_bridge_validation::mobile_bridge_validation_test::mobile_bridge_typecheck_accepts_valid_declarations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::mobile_bridge_validation::mobile_bridge_validation_test::mobile_bridge_typecheck_generates_validated_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc compiler::typeck::mobile_bridge_validation::mobile_bridge_validation_test::mobile_bridge_typecheck_rejects_missing_capability -- --exact

mobile-bridge-runtime-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_angular_bridge::mobile_angular_bridge_test::mobile_angular_bridge_encodes_typed_command -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_angular_bridge::mobile_angular_bridge_test::mobile_angular_bridge_decodes_typed_event -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_angular_bridge::mobile_angular_bridge_test::mobile_angular_bridge_generates_runtime_source -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_angular_bridge::mobile_angular_bridge_test::mobile_angular_bridge_encodes_component_lifecycle -- --exact

mobile-reactive-process-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::reactive_ui_process::reactive_ui_process_test::reactive_ui_process_generates_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::reactive_ui_process::reactive_ui_process_test::reactive_ui_process_runs_full_cycle_with_native_reply_fixture -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::reactive_ui_process::reactive_ui_process_test::reactive_ui_process_replays_native_reply_fixtures_deterministically -- --exact

mobile-android-shell-smoke:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_android_shell::mobile_android_shell_test::android_shell_generates_minimal_module_layout -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_android_shell::mobile_android_shell_test::android_shell_generates_path_config_file -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_android_shell::mobile_android_shell_test::android_bridge_protocol_rejects_invalid_json -- --exact

mobile-ios-shell-smoke:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_ios_shell::mobile_ios_shell_test::ios_shell_generates_minimal_module_layout -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_ios_shell::mobile_ios_shell_test::ios_shell_generates_native_screen_route_plan -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc mobile::mobile_ios_shell::mobile_ios_shell_test::ios_shell_platform_behaviors_declare_required_capabilities -- --exact

no-default-erlang-emission-check: erlang-backend-classification-check

no-default-beam-runtime-check: no-implicit-otp-runtime-check terlan-vm-no-otp-runtime-fallback-check

no-default-tokio-runtime-check:
	$(RUST_TEST) -p terlan --bin terlan-quality no_default_tokio_runtime_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- no-default-tokio-runtime

no-terlan-vm-erts-rust-dependency-check:
	$(RUST_TEST) -p terlan --bin terlan-quality no_terlan_vm_erts_rust_dependency_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- no-terlan-vm-erts-rust-dependency

no-implicit-otp-runtime-check:
	$(RUST_TEST) -p terlan --bin terlan-quality no_implicit_otp_runtime_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- no-implicit-otp-runtime

otp-runtime-exit-check:
	$(RUST_TEST) -p terlan --bin terlan-quality otp_runtime_exit_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- otp-runtime-exit

otp-test-pipeline-inventory-check:
	$(RUST_TEST) -p terlan --bin terlan-quality otp_test_pipeline_inventory_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- otp-test-pipeline-inventory

terlan-vm-erl-suite-audit-check:
	$(RUST_TEST) -p terlan --bin terlc module_layout_ -- --nocapture
	$(PYTHON) tools/check_terlan_vm_erl_suite_file_status.py
	$(PYTHON) tools/check_terlan_vm_erl_suite_audit_test.py
	$(PYTHON) tools/check_terlan_vm_erl_suite_audit.py

roadmap-legacy-runtime-cleanup-check:
	$(PYTHON) tools/check_roadmap_legacy_runtime_cleanup_test.py
	$(PYTHON) tools/check_roadmap_legacy_runtime_cleanup.py

roadmap-gate-integrity-check:
	$(RUST_TEST) -p terlan --bin terlan-quality roadmap_gate_integrity_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- roadmap-gate-integrity

callable-syntax-cleanup-check:
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_function_value_dot_call_syntax -- --exact
	$(TERLC_EXACT_TEST) --bin terlc compiler::syntax::parser::parser_expr_test::tests::rejects_parenthesized_function_value_dot_call_syntax -- --exact
	$(PYTHON) tools/check_callable_syntax_cleanup.py

terlan-lint-style-profile-check:
	$(RUST_TEST) -p terlan --bin terlan-quality terlan_lint_style_profile_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- terlan-lint-style-profile

terlan-lint-pipe-canonicalization-check: \
	terlan-lint-style-profile-check \
	terlan-lint-style-check \
	formatter-pipe-canonicalization-check
	$(TERLC_EXACT_TEST) --bin terlc commands::lint::lint_test::pipe_test::lint_rejects_default_argument_ambiguous_module_pipe_candidate -- --exact
	$(TERLC_EXACT_TEST) --bin terlc commands::lint::lint_test::pipe_test::lint_rejects_default_argument_ambiguous_receiver_pipe_candidate -- --exact

std-test-honesty-check:
	$(RUST_TEST) -p terlan --bin terlan-quality std_test_honesty_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- std-test-honesty

std-test-table-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::intrinsics::intrinsics_test::core_intrinsics_report_stable_direct_diagnostics -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm formal_pipeline::formal_pipeline_test::embedded_std_interfaces_include_float_math_contract -- --exact
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::native::base64::base64_test
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::native::md5::md5_test
	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/test/Test.terl
	target/debug/terlc check std/test/AssertionsTest.terl
	target/debug/terlc check std/test/TableTest.terl
	target/debug/terlc check std/test/LifecycleTest.terl
	target/debug/terlc test std/test/AssertionsTest.terl
	target/debug/terlc test std/test/TableTest.terl
	target/debug/terlc test std/test/LifecycleTest.terl
	target/debug/terlc test std/core/IntTest.terl
	target/debug/terlc test std/core/FloatTest.terl
	target/debug/terlc test std/core/StringTest.terl
	target/debug/terlc test std/encoding/Base64Test.terl
	target/debug/terlc test std/encoding/Md5Test.terl

std-test-property-check:
	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/collections/ListPropertyTest.terl
	target/debug/terlc check std/collections/MapPropertyTest.terl
	target/debug/terlc check std/collections/SetPropertyTest.terl
	target/debug/terlc check std/binary/BinaryPropertyTest.terl
	target/debug/terlc check std/core/AtomPropertyTest.terl
	target/debug/terlc check std/core/ErrorPropertyTest.terl
	target/debug/terlc check std/core/BoolPropertyTest.terl
	target/debug/terlc check std/core/FloatPropertyTest.terl
	target/debug/terlc check std/core/IntPropertyTest.terl
	target/debug/terlc check std/core/ObjectPropertyTest.terl
	target/debug/terlc check std/core/OptionPropertyTest.terl
	target/debug/terlc check std/core/OrderingPropertyTest.terl
	target/debug/terlc check std/core/ResultPropertyTest.terl
	target/debug/terlc check std/core/StringPropertyTest.terl
	target/debug/terlc check std/core/UnitPropertyTest.terl
	target/debug/terlc check std/data/JsonPropertyTest.terl
	target/debug/terlc check std/encoding/Base64PropertyTest.terl
	target/debug/terlc check std/io/PathPropertyTest.terl
	target/debug/terlc check std/net/UriPropertyTest.terl
	target/debug/terlc check std/range/RangePropertyTest.terl
	target/debug/terlc check std/random/RandomPropertyTest.terl
	target/debug/terlc check std/regex/RegexPropertyTest.terl
	target/debug/terlc check std/test/Gen.terl
	target/debug/terlc check std/test/GenTest.terl
	target/debug/terlc check std/test/PropertyDistributionTest.terl
	target/debug/terlc check std/test/PropertyTest.terl
	target/debug/terlc check std/test/Shrink.terl
	target/debug/terlc check std/test/ShrinkTest.terl
	target/debug/terlc check std/test/StatefulPropertyTest.terl
	target/debug/terlc test std/collections/ListPropertyTest.terl
	target/debug/terlc test std/collections/MapPropertyTest.terl
	target/debug/terlc test std/collections/SetPropertyTest.terl
	target/debug/terlc test std/binary/BinaryPropertyTest.terl
	target/debug/terlc test std/core/AtomPropertyTest.terl
	target/debug/terlc test std/core/ErrorPropertyTest.terl
	target/debug/terlc test std/core/BoolPropertyTest.terl
	target/debug/terlc test std/core/FloatPropertyTest.terl
	target/debug/terlc test std/core/IntPropertyTest.terl
	target/debug/terlc test std/core/ObjectPropertyTest.terl
	target/debug/terlc test std/core/OptionPropertyTest.terl
	target/debug/terlc test std/core/OrderingPropertyTest.terl
	target/debug/terlc test std/core/ResultPropertyTest.terl
	target/debug/terlc test std/core/StringPropertyTest.terl
	target/debug/terlc test std/core/UnitPropertyTest.terl
	target/debug/terlc test std/data/JsonPropertyTest.terl
	target/debug/terlc test std/encoding/Base64PropertyTest.terl
	target/debug/terlc test std/io/PathPropertyTest.terl
	target/debug/terlc test std/net/UriPropertyTest.terl
	target/debug/terlc test std/range/RangePropertyTest.terl
	target/debug/terlc test std/random/RandomPropertyTest.terl
	target/debug/terlc test std/regex/RegexPropertyTest.terl
	target/debug/terlc test std/test/GenTest.terl
	target/debug/terlc test std/test/PropertyDistributionTest.terl
	target/debug/terlc test std/test/PropertyTest.terl
	target/debug/terlc test std/test/ShrinkTest.terl
	target/debug/terlc test std/test/StatefulPropertyTest.terl

std-range-check:
	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/range/Range.terl
	target/debug/terlc check std/range/RangeTest.terl
	target/debug/terlc check std/range/RangePropertyTest.terl
	target/debug/terlc test std/range

std-random-check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::native::random::random_test
	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/random/Random.terl
	target/debug/terlc check std/random/RandomTest.terl
	target/debug/terlc check std/random/RandomPropertyTest.terl
	target/debug/terlc test std/random

std-regex-check:
	$(CARGO) build --locked --bin terlc
	target/debug/terlc check std/regex/Regex.terl
	target/debug/terlc check std/regex/RegexTest.terl
	target/debug/terlc check std/regex/RegexPropertyTest.terl
	target/debug/terlc test std/regex

std-package-coverage-100-check: shape-implications-check
	$(RUST_TEST) -p terlan --bin terlan-quality std_package_coverage_100_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- std-package-coverage-100

js-type-emission-contract-check:
	$(RUST_TEST) -p terlan --bin terlan-quality js_type_emission_contract_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- js-type-emission-contract


vm-otp-abstractions-terlan-stdlib-check:
	$(RUST_TEST) -p terlan --bin terlan-quality vm_otp_abstractions_terlan_stdlib_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-otp-abstractions-terlan-stdlib

vm-ownership-classification-check:
	$(RUST_TEST) -p terlan --bin terlan-quality vm_ownership_classification_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-ownership-classification

vm-runtime-concept-inventory-check:
	$(RUST_TEST) -p terlan --bin terlan-quality vm_runtime_concept_inventory_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-runtime-concept-inventory

terlan-runtime-conformance-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native::vector::vector_test::vector_mutations_update_storage -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native::json::json_test::parse_and_stringify_round_trip_json_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact

terlan-release-train-check:
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::build_test::tests::artifact_test::build_command_emits_js_module_and_manifest_for_single_file -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc formal_pipeline::formal_pipeline_test::compile_syntax_module_with_core_v0_profile_accepts_covered_subset -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native::postgres::postgres_test::config_builders_set_pool_limits_and_timeouts -- --exact

otp-reference-inventory-check:
	$(RUST_TEST) -p terlan --bin terlan-quality otp_reference_inventory_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- otp-reference-inventory

vm-multicore-invariant-inventory-check:
	$(RUST_TEST) --locked -p terlan --bin terlan-quality multicore_invariant_inventory_test
	$(CARGO) run --locked -p terlan --bin terlan-quality --quiet -- vm-multicore-invariant-inventory

vm-otp-corpus-inventory-check: otp-reference-inventory-check

terlan-vm-no-otp-runtime-fallback-check: otp-reference-inventory-check otp-test-pipeline-inventory-check erlang-backend-classification-check terlan-vm-external-repo-boundary-check

hex-target-metadata-check:
	$(RUST_TEST) -p terlan --bin terlan-quality hex_target_metadata_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- hex-target-metadata

native-no-std-target-feasibility-check: hex-target-metadata-check
	$(CARGO) run --locked -p terlan --bin terlan-native-target-feasibility --quiet

device-target-planner-check: native-no-std-target-feasibility-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- device-target-planner

terlan-package-git-source-check:
	$(RUST_TEST) -p terlan --bin terlan-quality package_git_source_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-git-source
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc commands::build::project_manifest::project_manifest_test::project_manifest_rejects_git_dependency_without_rev -- --exact

terlan-package-lockfile-check:
	$(RUST_TEST) -p terlan --bin terlan-quality package_lockfile_contract_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-lockfile-contract

package-resolver-reproducibility-check: device-target-planner-check terlan-package-lockfile-check terlan-package-git-source-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-resolver-reproducibility

package-registry-publish-check: package-resolver-reproducibility-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-registry-publish

package-capability-contract-check: package-registry-publish-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-capability-contract

package-release-test-matrix-check: package-capability-contract-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-release-test-matrix

package-api-compatibility-check: package-release-test-matrix-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-api-compatibility

package-cli-workflow-check: package-api-compatibility-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-cli-workflow

package-editor-integration-check: package-cli-workflow-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-editor-integration

package-cache-integrity-check: package-editor-integration-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-cache-integrity

package-workspace-graph-check: package-cache-integrity-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-workspace-graph

package-build-artifact-isolation-check: package-workspace-graph-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-build-artifact-isolation

source-map-debug-info-check: package-build-artifact-isolation-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- source-map-debug-info

compiler-incremental-cache-check: source-map-debug-info-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- compiler-incremental-cache

watch-mode-hot-reload-check: compiler-incremental-cache-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- watch-mode-hot-reload

release-flake-detection-check: watch-mode-hot-reload-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-flake-detection

release-gate-shard-resume-check: release-flake-detection-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-gate-shard-resume

release-gate-duration-budget-check: release-gate-shard-resume-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-gate-duration-budget

release-gate-report-schema-check: release-gate-duration-budget-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-gate-report-schema

release-failure-reproduction-check: release-gate-report-schema-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- release-failure-reproduction

release-generated-artifacts-freshness-pass:
	$(MAKE) --no-print-directory stdlib-summary-drift-check
	$(MAKE) --no-print-directory stdlib-js-bindings-drift-check
	$(MAKE) --no-print-directory stdlib-native-artifacts-check
	$(MAKE) --no-print-directory stdlib-release-manifest-check
	$(MAKE) --no-print-directory tree-sitter-cli-check

release-generated-artifacts-check:
	$(PYTHON) tools/check_release_generated_artifacts.py --record-snapshot target/quality/release-generated-artifacts-before.json
	$(MAKE) --no-print-directory release-generated-artifacts-freshness-pass
	$(PYTHON) tools/check_release_generated_artifacts.py --compare-snapshot target/quality/release-generated-artifacts-before.json
	$(MAKE) --no-print-directory release-generated-artifacts-freshness-pass
	$(PYTHON) tools/check_release_generated_artifacts.py --compare-snapshot target/quality/release-generated-artifacts-before.json
	$(PYTHON) tools/check_release_generated_artifacts.py --self-test
	$(PYTHON) tools/check_release_generated_artifacts.py --regeneration-run-count 2
	rm -f target/quality/release-generated-artifacts-before.json

package-test-exec-check:

terlan-vm-internal-crate-check:
	$(RUST_TEST) -p terlan --bin terlan-quality terlan_vm_internal_crate_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- terlan-vm-internal-crate

terlan-vm-external-repo-boundary-check:
	$(RUST_TEST) -p terlan --bin terlan-quality terlan_vm_external_repo_boundary_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- terlan-vm-external-repo-boundary

native-boundary-terminology-check:
	$(RUST_TEST) -p terlan --bin terlan-quality native_boundary_terminology_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- native-boundary-terminology

native-boundary-security-check:
	$(RUST_TEST) -p terlan --bin terlan-quality native_boundary_security_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- native-boundary-security

native-binding-generator-contract-check:
	$(RUST_TEST) -p terlan --bin terlan-quality native_binding_generator_contract_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- native-binding-generator-contract

cpp-binding-generator-check cpp-package-consumer-check: export RUSTFLAGS := -D warnings

cpp-binding-generator-check: native-binding-generator-contract-check cpp-binding-metadata-extractor-check
	$(RUST_TEST) -p terlan --bin terlc cpp_binding_generator
	$(MAKE) --no-print-directory cpp-package-consumer-check

cpp-binding-metadata-extractor-check:
	$(RUST_TEST) -p terlan --bin terlc committed_clang_metadata_is_consumed_offline
	$(PYTHON) tools/check_cpp_metadata_extractor.py

cpp-binding-metadata-extractor-live-check: cpp-binding-metadata-extractor-check
	$(PYTHON) tools/check_cpp_metadata_extractor.py --live

cpp-binding-build-plan-check: cpp-binding-generator-check

cpp-binding-value-record-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check
	$(RUST_TEST) -p terlan --bin terlc native_protocol_decodes_copied_integer_records

cpp-binding-copied-containers-check: cpp-binding-generator-check
	$(RUST_TEST) -p terlan --bin terlc native_protocol_decodes_copied_string_bytes_and_integer_lists

cpp-binding-enum-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check
	$(RUST_TEST) -p terlan --bin terlc native_protocol_decodes_symbolic_enum_atoms_without_discriminants

cpp-binding-exception-check: cpp-binding-generator-check
	$(RUST_TEST) -p terlan --bin terlc native_protocol_round_trips_handles_and_result_errors

cpp-package-consumer-check:
	$(CARGO) build -p terlan --bin terlc --bin terlan-vm
	$(RUST_TEST) -p terlan --bin terlc generated_cpp_git_package_executes_and_rejects_stale_handles -- --ignored

c-abi-binding-generator-check: native-binding-generator-contract-check
	$(RUST_TEST) -p terlan --bin terlc c_abi_binding_generator

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

.PHONY: libpq-c-abi-check
libpq-c-abi-check: c-abi-binding-generator-check
	$(CARGO) test -p terlan-libpq --all-targets --offline
	$(CARGO) test -p terlan --bin terlc runtime::native::postgres::libpq::libpq_test --no-default-features
	$(CARGO) check -p terlan --bin terlan-vm --no-default-features

terlan-polars-package-check: terlan-polars-package-focused-check

terlan-polars-package-focused-check: c-abi-binding-generator-check
	$(RUST_TEST) -p terlan --bin terlc package_git
	$(RUST_TEST) -p terlan --bin terlan-quality terlan_polars_
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- terlan-polars-package

terlan-pytorch-package-check: c-abi-binding-generator-check
	$(CARGO) build -p terlan --bin terlc --no-default-features
	@set -euo pipefail; \
		package_dir="$(TERLAN_PYTORCH_DIR)"; \
		workspace=""; \
		trap '[[ -z "$$workspace" ]] || rm -rf "$$workspace"' EXIT; \
		if [[ -z "$$package_dir" ]]; then \
			workspace=$$(mktemp -d "$${TMPDIR:-/tmp}/terlan-pytorch-package.XXXXXX"); \
			source_url="$(TERLAN_PYTORCH_REPOSITORY)"; \
			sibling="$(abspath ../terlan-pytorch)"; \
			if git -C "$$sibling" cat-file -e "$(TERLAN_PYTORCH_REV)^{commit}" 2>/dev/null; then \
				source_url="$$sibling"; \
			fi; \
			printf '%s\n' \
				'[package]' \
				'name = "terlan-pytorch-gate"' \
				'version = "0.0.7"' \
				'' \
				'[dependencies]' \
				'pytorch = { git = "'"$$source_url"'", rev = "$(TERLAN_PYTORCH_REV)" }' \
				> "$$workspace/terlan.toml"; \
			"$(CURDIR)/target/debug/terlc" package fetch "$$workspace"; \
			package_dir="$$workspace/.terlan/packages/git/$(TERLAN_PYTORCH_REV)"; \
		fi; \
		test -f "$$package_dir/Makefile" || { \
			echo "error[terlan_pytorch_package_missing]: package metadata did not resolve a terlan-pytorch checkout" >&2; \
			exit 1; \
		}; \
		if [[ -n "$(TERLAN_PYTORCH_LIBTORCH)" ]]; then \
			$(MAKE) -C "$$package_dir" package-check \
				TERLC="$(CURDIR)/target/debug/terlc" \
				LIBTORCH="$(TERLAN_PYTORCH_LIBTORCH)"; \
		else \
			$(MAKE) -C "$$package_dir" release-check \
				TERLC="$(CURDIR)/target/debug/terlc"; \
		fi; \
		$(PYTHON) "$$package_dir/scripts/check-package-report.py" \
			"$$package_dir/generated/package-execution-report.json"

cuda-package-availability-check:
	$(RUST_TEST) -p terlan --bin terlan-quality cuda_package_availability::tests
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- cuda-package-availability

cuda-package-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- cuda-package-check

native-boundary-runtime-adversarial-check:
	$(RUST_TEST) --locked -p terlan --bin terlc runtime::native_boundary::capability_wire_test
	$(RUST_TEST) --locked -p terlan --bin terlan-native-worker protocol::protocol_test::framing_rejects_oversized_input_and_output -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_disposed_handles_through_terms -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_duplicate_dispose_as_stale_handle -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_disposal -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_enforces_postgres_capability_before_adapter_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_cancel_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_completion_wins_cancellation_race_without_request_id_reuse -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_lifecycle_events_write_native_boundary_report -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_lifecycle_event_history_is_bounded -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_timeout_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlc runtime::native_boundary::worker::worker_test::worker_duplicate_dispose_returns_stale_handle_and_releases_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact

.PHONY: vm-native-boundary-contract-check
vm-native-boundary-contract-check: lean-proof-track-check
	test -f target/quality/vm-native-boundary-report.json

.PHONY: vm-sql-macro-validation-check
vm-sql-macro-validation-check: vm-db-migration-command-check
	$(PYTHON) tools/check_sql_form_boundary.py
	env RUSTFLAGS='-D warnings' $(RUST_TEST) --locked -p terlan --bin terlc --bin terlan-quality sql
	$(CARGO) run --locked -p terlan --bin terlan-quality --quiet -- vm-sql-macro-validation
	test -s target/quality/vm-sql-macro-validation-report.json

.PHONY: vm-db-migration-command-check
vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check
	env RUSTFLAGS='-D warnings' $(RUST_TEST) --locked -p terlan --bin terlan-quality vm_db_migration_command
	$(CARGO) run --locked -p terlan --bin terlan-quality --quiet -- vm-db-migration-command
	test -s target/quality/vm-db-migration-report.json

.PHONY: vm-dev-dependency-orchestration-check
vm-dev-dependency-orchestration-check:
	env RUSTFLAGS='-D warnings' $(RUST_TEST) --locked -p terlan --bin terlc commands::dev_dependencies
	env RUSTFLAGS='-D warnings' $(RUST_TEST) --locked -p terlan --bin terlan-quality vm_dev_dependency_orchestration
	$(CARGO) run --locked -p terlan --bin terlan-quality --quiet -- vm-dev-dependency-orchestration
	test -s target/quality/vm-dev-dependency-report.json

.PHONY: vm-postgres-runtime-check
vm-postgres-runtime-check: vm-sql-macro-validation-check vm-native-boundary-contract-check no-default-tokio-runtime-check libpq-c-abi-check
	test -s target/quality/vm-postgres-runtime-report.json

native-boundary-postgres-baseline-benchmark:
	$(CARGO) run -p terlan --bin terlan-benchmark --quiet -- native-boundary-postgres-baseline

native-boundary-http-baseline-benchmark:
	$(CARGO) run -p terlan --bin terlan-benchmark --quiet -- native-boundary-http-baseline

vm-performance-baseline-check: achamp-adversarial-coverage-check
	$(RUST_TEST) -p terlan --bin terlan-benchmark tests::synthetic_helper_source_contains_requested_workload -- --exact
	$(RUST_TEST) -p terlan --bin terlan-benchmark tests::vm_performance_skipped_tracks_match_required_policy -- --exact
	$(RUST_TEST) -p terlan --bin terlan-benchmark tests::map_benchmark_tracks_cover_otp_threshold_sizes -- --exact
	$(RUST_TEST) -p terlan --bin terlan-benchmark tests::otp_map_benchmark_eval_uses_native_map_assertions -- --exact
	$(CARGO) run -p terlan --bin terlan-benchmark --quiet -- vm-performance-baseline

achamp-adversarial-coverage-check: vm-memory-heap-pressure-check
	$(RUST_TEST) -p terlan --bin terlan-quality achamp_adversarial_coverage_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- achamp-adversarial-coverage

executable-docs-vm-check:
	$(RUST_TEST) -p terlan --bin terlan-quality executable_docs_vm_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- executable-docs-vm
	$(EXACT_CARGO_TEST) -p terlan --bin terlc tests::doc_test::readme_hello_world_terlan_block_compiles -- --exact
	$(CARGO) build --locked --bin terlc
	bash scripts/check_readme_hello_world_run.sh

docs-codeblock-executable-check: executable-docs-vm-check


terlan-vm-compiler-bridge-check:
	$(MAKE) --no-print-directory cli-terlan-vm-compiler-bridge-check

terlc-build-executable-check: cli-terlc-build-executable-check

http-runtime-stack-check:
	$(PYTHON) tools/check_http_runtime_stack.py
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_get_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_file_with_query_string -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_omits_static_head_response_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_static_parent_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_unmatched_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_streams_reload_sse_events -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_heads_reload_sse_without_opening_stream -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_reload_sse_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::run_serve_check_rejects_dynamic_handlers_missing_source_metadata -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::run_serve_check_rejects_dynamic_handlers_with_removed_beam_runtime -- --exact

vm-http-vs-axum-check:
	$(PYTHON) tools/check_vm_benchmark_family_plan.py vm-http-vs-axum-check
	$(PYTHON) scripts/benchmarks/protocol/protocol_benchmark.py --validate-only --anchor binary_protocol_concurrency_benchmark

vm-http-concurrency-investigation-check: vm-scheduler-fairness-check vm-http-vs-axum-check
	$(PYTHON) tools/check_vm_http_concurrency_investigation.py --self-test
	$(PYTHON) tools/check_vm_http_concurrency_investigation.py

vm-http-benchmark-comparability-check: $(VM_HTTP_BENCHMARK_COMPARABILITY_DEPS)
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-benchmark-comparability

vm-http-runtime-attribution-check: vm-http-benchmark-comparability-check
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-http-runtime-attribution

vm-http-soak-stability-check: vm-http-runtime-attribution-check vm-timer-deadline-check
	test -s $(HTTP_SOAK_REPORT)

vm-semantics-vs-otp-check: binary-bitstring-processing-check
	$(PYTHON) tools/check_vm_benchmark_family_plan.py vm-semantics-vs-otp-check
	$(PYTHON) scripts/benchmarks/protocol/protocol_benchmark.py --validate-only --anchor binary_protocol_benchmark

runtime-release-dependency-self-test:
	$(PYTHON) tools/check_runtime_release_dependencies.py --self-test

battleship-external-vm-contract-check:
	$(PYTHON) tools/check_battleship_external_vm_contract.py

angular-ts-terlan-integration-check: angular-ts-namespace-generation-check angular-ts-terlan-app-ownership-check
	$(PYTHON) tools/check_angular_ts_terlan_integration.py

angular-ts-namespace-generation-check:
	$(PYTHON) tools/check_angular_ts_terlan_integration.py --namespace-generation-check

angular-ts-terlan-app-ownership-check:
	$(PYTHON) tools/check_angular_ts_terlan_integration.py --app-ownership-check



changelog-public-scope-check:
	$(PYTHON) tools/check_changelog_public_scope.py

internal-docs-check:
	$(RUST_TEST) -p terlan --bin terlan-quality internal_docs_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- internal-docs

module-readme-check:
	$(RUST_TEST) -p terlan --bin terlan-quality module_readmes_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- module-readmes

rustdoc-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-docs

release-artifact-current:
	$(MAKE) release-boundary-check
	$(MAKE) release-version-metadata-check
	$(MAKE) source-extension-check
	$(MAKE) vm-release-artifact-matrix-check
	$(PYTHON) tools/release_promotion_pipeline.py seal

release-artifact-linux:
	TERLAN_RELEASE_OS=Linux TERLAN_RELEASE_ARCH=x86_64 $(MAKE) release-artifact-current

release-artifact-smoke:
	$(PYTHON) tools/package_release_artifact.py smoke

release-artifact-installer-smoke:
	$(PYTHON) tools/package_release_artifact.py installer-smoke

publish-preflight: release-candidate-check
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
		echo "next step: review and commit the release contents, then rerun make publish VERSION=$(VERSION)"; \
		exit 1; \
	fi
	@branch=$$(git branch --show-current); \
	if [ "$$branch" != "main" ]; then \
		echo "publication must run from main; current branch is $$branch"; \
		exit 1; \
	fi
	bash scripts/check_release_version_metadata.sh "$(VERSION)"
	@if git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		tag_sha=$$(git rev-parse "refs/tags/v$(VERSION)"); \
		head_sha=$$(git rev-parse HEAD); \
		if [ "$$tag_sha" != "$$head_sha" ]; then \
			echo "local tag v$(VERSION) already exists at $$tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "local tag v$(VERSION) already exists at HEAD; continuing"; \
	fi
	@if remote_tag_line=$$(git ls-remote --tags origin "refs/tags/v$(VERSION)" 2>/dev/null) && [ -n "$$remote_tag_line" ]; then \
		remote_tag_sha=$$(printf '%s\n' "$$remote_tag_line" | awk '{ print $$1 }' | head -1); \
		head_sha=$$(git rev-parse HEAD); \
		if [ "$$remote_tag_sha" != "$$head_sha" ]; then \
			echo "remote tag v$(VERSION) already exists at $$remote_tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "remote tag v$(VERSION) already exists at HEAD; release upload can be retried"; \
	fi
	$(MAKE) release-artifact-current

publish: publish-preflight
	@command -v gh >/dev/null 2>&1 || { \
		echo "publish requires GitHub CLI: install gh and run gh auth login"; \
		exit 127; \
	}
	@gh auth status >/dev/null 2>&1 || { \
		echo "publish requires authenticated GitHub CLI: run gh auth login"; \
		exit 1; \
	}
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		git tag "v$(VERSION)"; \
	fi
	git push origin main
	git push origin "v$(VERSION)"
	$(MAKE) publish-release-from-dist VERSION=$(VERSION)

publish-release-from-dist:
	bash scripts/publish_release_from_dist.sh "$(VERSION)"

release-promotion-pipeline-check:
	$(PYTHON) tools/release_promotion_pipeline.py self-test --report
	$(PYTHON) tools/release_promotion_pipeline.py contract

release-promotion-dry-run:
	$(PYTHON) tools/release_promotion_pipeline.py dry-run $(if $(VERSION),--version "$(VERSION)",)

clean:
	$(MAKE) cli-clean
