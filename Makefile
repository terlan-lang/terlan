CARGO := cargo --locked
RUST_TEST := $(CARGO) test --profile dev
.DEFAULT_GOAL := terlan-serve-runtime-bootstrap
RELEASE_VERSION ?= $(shell sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
EXACT_CARGO_TEST := bash scripts/run_exact_cargo_test.sh
# Requests with explicit execution policy must run, even with hosted coverage.
POLICY_CARGO_TEST = bash scripts/run_exact_cargo_test.sh
TERLAN_BOOTSTRAP_COMPILER := target/debug/terlc
TERLAN_BOOTSTRAP_COMPILER_BUILD := $(TERLAN_BOOTSTRAP_COMPILER) build --incremental
TERLAN_BOOTSTRAP_VM := target/debug/terlan-vm
TERLAN_RUST_SUITE_TERLC ?= $(CURDIR)/target/debug/terlc
TERLAN_RUST_SUITE_VM ?= $(CURDIR)/target/debug/terlan-vm
TERLAN_RUST_SUITE_NATIVE_WORKER ?= $(CURDIR)/target/debug/terlan-native-worker
TERLAN_VALIDATION_VM := target/release/terlan-vm
TERLAN_SELF_HOST_IMAGE ?= target/quality/self-host-bootstrap-current-40/bin/terlan-self-host
TERLAN_SELF_HOST_PHASE2_EVIDENCE ?= target/quality/self-host-phase2-current
TERLAN_BUILD_CACHE := target/debug/terlan-build-cache
TERLAN_SERVE_RUNTIME_PROFILE := serve-runtime
TERLAN_SERVE_RUNTIME_BIN := $(CURDIR)/target/$(TERLAN_SERVE_RUNTIME_PROFILE)/terlan-serve-runtime
TERLAN_SERVE_RUNTIME_REL_BIN := $(patsubst $(CURDIR)/%,%,$(TERLAN_SERVE_RUNTIME_BIN))
TERLAN_SERVE_RUNTIME_BUILD := $(CARGO) build --profile $(TERLAN_SERVE_RUNTIME_PROFILE) -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin
# Validation always needs the quality-tool feature superset later in the same
# graph. Build the compiler with that superset once so Cargo does not compile
# the large `terlan` crate again under a second feature identity.
TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS = -p terlan --bin terlc --bin terlan-vm --bin terlan-native-worker --features terlan/validation-union
TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS ?= 3600
TERLAN_CHECK_GRAPH_TIMEOUT_SECONDS ?= 3600
TERLAN_BOOTSTRAP_LOCK_WAIT_SECONDS ?= 120
# Only acquisition uses the lock deadline; each producer owns its execution bound.
TERLAN_BOOTSTRAP_LOCK = flock --exclusive --wait $(TERLAN_BOOTSTRAP_LOCK_WAIT_SECONDS) target/quality/bootstrap-owner.lock
TERLAN_BUILD_MINIMUM_FREE_BYTES ?= 8589934592
TERLAN_TYPED_VALIDATOR_BUILD := bash scripts/build_typed_validator.sh
TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT := target/self-validation/typed-validator-common.inputs.sha256
TERLAN_TYPED_VALIDATOR_COMMON_INPUTS := $(TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT)
TERLAN_SELF_VALIDATION_DIR := target/self-validation/build-artifact-budget
TERLAN_SELF_VALIDATION_IMAGE := $(TERLAN_SELF_VALIDATION_DIR)/vm/scripts_self_validation_BuildArtifactBudgetTest.tvm
TERLAN_MAKE_RECIPE_DIR := target/self-validation/make-recipe-thinness
TERLAN_MAKE_RECIPE_IMAGE := $(TERLAN_MAKE_RECIPE_DIR)/vm/scripts_self_validation_MakeRecipeThinness.tvm
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
TERLAN_RUST_QUALITY := $(CURDIR)/$(TERLAN_VALIDATION_VM) run $(CURDIR)/$(TERLAN_RUST_QUALITY_IMAGE) --script-eval --
TERLAN_RELEASE_PROMOTION_DIR := target/self-validation/release-promotion
TERLAN_RELEASE_PROMOTION_IMAGE := $(TERLAN_RELEASE_PROMOTION_DIR)/vm/scripts_ReleasePromotion.tvm
TERLAN_RELEASE_PROMOTION := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_RELEASE_PROMOTION_IMAGE) --script-eval --

# The outer invocation owns fd 9. Reuse that file description, not a new open
# of the same flock (which would conflict with our own invocation). A distinct
# owner lock serializes sibling graph producers within an inherited scope.
TERLAN_PREPARATION_OWNER = /bin/sh -ec '\
	test -d target && test ! -L target; \
	test -d target/quality && test ! -L target/quality; \
	for lock in target/quality/preparation.lock target/quality/preparation-owner.lock; do \
		test ! -L "$$lock"; \
		if test -e "$$lock"; then test -f "$$lock"; fi; \
	done; \
	case "$${TERLAN_PREPARATION_LOCK_HELD:-}" in \
		1) test /proc/self/fd/9 -ef target/quality/preparation.lock ;; \
		"") exec 9>target/quality/preparation.lock ;; \
		*) echo "invalid preparation lease scope" >&2; exit 1 ;; \
	esac; \
	flock --exclusive --wait 120 9; \
	test /proc/self/fd/9 -ef target/quality/preparation.lock; \
	export TERLAN_PREPARATION_LOCK_HELD=1; \
	exec flock --exclusive --wait 120 target/quality/preparation-owner.lock "$$@"\
' preparation-owner

# Publication preparation owns contract execution; ordinary development targets
# continue to run the requested contract directly. Reuse is hash-verified by the
# owner, never authorized merely by this routing selection.
ifneq ($(filter publish-prepare publish-prepare-locked publish-evidence-refresh,$(MAKECMDGOALS)),)
ifeq ($(origin TERLAN_PREPARATION_CONTRACT_OWNERS),command line)
ifneq ($(TERLAN_PREPARATION_CONTRACT_OWNERS),1)
$(error publication preparation cannot disable contract ownership)
endif
endif
override export TERLAN_PREPARATION_CONTRACT_OWNERS := 1
endif
ifeq ($(TERLAN_PREPARATION_CONTRACT_OWNERS),1)
TERLAN_TVM_CONTRACT_CHECK := $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-platform-contract "$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"
TERLAN_TVM_CONTRACT_BOOTSTRAP := terlan-tvm-platform-matrix-bootstrap terlan-release-promotion-bootstrap publish-preparation-lock-directory
else
TERLAN_TVM_CONTRACT_CHECK := $(TERLAN_TVM_PLATFORM_MATRIX)
TERLAN_TVM_CONTRACT_BOOTSTRAP := terlan-tvm-platform-matrix-bootstrap
endif
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
# Account for executable mappings and image admission; managed heap stays at 64 MiB.
TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB := 196608
TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX := 1
TERLAN_TVM_PLATFORM_REPORT_ROOT ?= target/quality/tvm-aot-platform-input
TERLAN_SHARED_HELPER_TIMEOUT_SECONDS := 300
TERLAN_SHARED_HELPER_VIRTUAL_MEMORY_KIB := 196608
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
	vm-float-native-arithmetic-check \
	std-test-table-check \
	std-test-property-check \
	std-range-check \
	std-random-check \
	std-regex-check \
	executable-docs-vm-check \
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
SHELL := bash
.SHELLFLAGS := -eo pipefail -c

TERLAN_SETUP_APT_PACKAGES := build-essential pkg-config libpq-dev

# Installs native tooling the default build and pre-commit hook need.
# CUDA/Docker stay out of scope -- opt-in targets the hook never touches.
.PHONY: setup
# Installs native build tooling (C toolchain, pkg-config, libpq) and checks for node/npm.
setup:
ifeq ($(shell uname -s),Linux)
	command -v apt-get >/dev/null 2>&1 || { \
		echo "setup: apt-get not found; install manually: $(TERLAN_SETUP_APT_PACKAGES)" >&2; \
		exit 1; \
	}
	SUDO=""; \
	[ "$$(id -u)" = "0" ] || SUDO="sudo"; \
	$$SUDO apt-get update; \
	$$SUDO apt-get install -y --no-install-recommends $(TERLAN_SETUP_APT_PACKAGES)
	command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 || { \
		echo "setup: node/npm not found; the pre-commit hook needs them when staged changes touch editor/tree-sitter source, stdlib summary generators, or package-lock.json. apt's nodejs is often stale -- install via nvm instead: https://github.com/nvm-sh/nvm#installing-and-updating" >&2; \
		exit 1; \
	}
else
	echo "setup: automated installation only covers Linux; install a C toolchain, pkg-config, and libpq development headers for your platform (e.g. 'brew install pkg-config libpq' on macOS) and ensure pkg-config can resolve libpq" >&2
	exit 1
endif

.PHONY: tvm-native-image-format-check tvm-direct-aot-backend-check tvm-aot-application-closure-check tvm-aot-case-lowering-check tvm-aot-higher-order-specialization-check tvm-aot-lowering-coverage-check tvm-aot-managed-continuation-check tvm-aot-owned-closure-representation-check tvm-aot-static-callable-check tvm-aot-thread-neutral-continuation-check tvm-aot-typed-lifecycle-check tvm-aot-typed-mailbox-check tvm-managed-memory-check tvm-native-image-loader-check tvm-aot-consumer-check tvm-aot-test-consumer-check tvm-aot-repl-consumer-check tvm-aot-debugger-consumer-check tvm-aot-hot-reload-consumer-check tvm-aot-package-install-consumer-check tvm-aot-support-crash-metadata-check tvm-aot-platform-target-check tvm-aot-platform-matrix-check tvm-aot-http-managed-cycle-check tvm-aot-http-request-accessor-check tvm-aot-http-response-mutation-check tvm-aot-http-typed-metadata-check tvm-aot-http-router-callable-check tvm-aot-http-managed-error-check tvm-aot-http-template-check tvm-aot-http-template-render-plan-check tvm-aot-http-template-expression-check tvm-aot-http-body-json-check tvm-aot-http-session-check tvm-aot-http-managed-boundary-check tvm-aot-http-channel-plan-check tvm-aot-http-persistent-shard-check tvm-aot-http-native-invocation-check tvm-aot-http-websocket-invocation-check tvm-aot-http-sse-invocation-check tvm-aot-http-generation-lifetime-check tvm-aot-http-channel-transport-check tvm-aot-http-cleanup-check tvm-aot-http-lifecycle-inventory-check tvm-aot-http-checked-coreir-reference-record tvm-aot-http-performance-check tvm-single-image-artifact-check tvm-aot-runtime-transition-check tvm-aot-runtime-transition-focused-check tvm-aot-compilation-time-check tvm-aot-capability-worker-check

.PHONY: docs-light-check rust-security-audit-check node-security-audit-check terlan-serve-runtime-bootstrap terlan-http-benchmark-release-bootstrap

ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifneq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
# Adds the build-owner lock as a prerequisite when not already bootstrapped or prebuilt.
terlan-serve-runtime-bootstrap: | terlan-build-owner-bootstrap
endif
endif
# Builds the terlan-serve-runtime binary, reusing the cached cargo build-owner lock.
terlan-serve-runtime-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	test -x $(TERLAN_SERVE_RUNTIME_BIN)
else
	@if test "$(shell uname -s)" = Linux; then \
		set -eu; \
		if (set -o pipefail) 2>/dev/null; then set -o pipefail; fi; \
		mkdir -p target/quality; \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		build_command="$$cargo_program $$cargo_args build --profile $(TERLAN_SERVE_RUNTIME_PROFILE) -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin"; \
		if git diff --quiet \
			&& git diff --cached --quiet \
			&& test -z "$$(git ls-files --others --exclude-standard)"; then \
			revision="$$(git rev-parse --verify HEAD)"; \
			input="$$( { printf '%s\n' "$$revision"; sha256sum Cargo.toml Cargo.lock rust-toolchain.toml crates/terlan/Cargo.toml; rustc --version; cargo --version; printf '%s\n' serve-runtime-release; sha256sum "$(TERLAN_BUILD_CACHE)"; } | sha256sum | awk '{print $$1}')"; \
			$(TERLAN_BOOTSTRAP_LOCK) \
			"$(TERLAN_BUILD_CACHE)" owner \
				--cargo "$(firstword $(CARGO))" \
				--source-revision "$$revision" \
				--receipt "target/quality/preparation/bootstrap/$$revision/serve-runtime.json" \
				--input-sha256 "$$input" \
				--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
				--output "$(TERLAN_SERVE_RUNTIME_REL_BIN)" \
				-- /bin/sh -c "$$build_command"; \
		else \
			$(TERLAN_BOOTSTRAP_LOCK) \
			$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$$cargo_program" $$cargo_args build --profile $(TERLAN_SERVE_RUNTIME_PROFILE) -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin; \
		fi; \
	else \
		$(TERLAN_SERVE_RUNTIME_BUILD); \
	fi
endif

ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifneq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
# Adds the build-owner lock as a prerequisite when not already bootstrapped or prebuilt.
terlan-http-benchmark-release-bootstrap: | terlan-build-owner-bootstrap
endif
endif
# Builds the HTTP benchmark release binaries (axum/hyper baselines and framework/paired benchmarks).
terlan-http-benchmark-release-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	@for binary in terlan-axum-baseline terlan-hyper-baseline terlan-http-framework-benchmark terlan-http-paired-benchmark; do \
		test -x "target/release/$$binary"; \
	done
else
	@if test "$(shell uname -s)" = Linux; then \
		set -eu; \
		if (set -o pipefail) 2>/dev/null; then set -o pipefail; fi; \
		mkdir -p target/quality; \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		build_command="$$cargo_program $$cargo_args build --release -p terlan --bin terlan-axum-baseline --bin terlan-hyper-baseline --bin terlan-http-framework-benchmark --bin terlan-http-paired-benchmark --features axum-baseline,benchmark-tools"; \
		if git diff --quiet \
			&& git diff --cached --quiet \
			&& test -z "$$(git ls-files --others --exclude-standard)"; then \
			revision="$$(git rev-parse --verify HEAD)"; \
			input="$$( { printf '%s\n' "$$revision"; sha256sum Cargo.toml Cargo.lock rust-toolchain.toml crates/terlan/Cargo.toml; rustc --version; cargo --version; printf '%s\n' http-benchmark-release; sha256sum "$(TERLAN_BUILD_CACHE)"; } | sha256sum | awk '{print $$1}')"; \
			$(TERLAN_BOOTSTRAP_LOCK) \
			"$(TERLAN_BUILD_CACHE)" owner \
				--cargo "$(firstword $(CARGO))" \
				--source-revision "$$revision" \
				--receipt "target/quality/preparation/bootstrap/$$revision/http-benchmark-release.json" \
				--input-sha256 "$$input" \
				--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
				--output target/release/terlan-axum-baseline \
				--output target/release/terlan-hyper-baseline \
				--output target/release/terlan-http-framework-benchmark \
				--output target/release/terlan-http-paired-benchmark \
				-- /bin/sh -c "$$build_command"; \
		else \
			$(TERLAN_BOOTSTRAP_LOCK) \
			$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$$cargo_program" $$cargo_args build --release -p terlan --bin terlan-axum-baseline --bin terlan-hyper-baseline --bin terlan-http-framework-benchmark --bin terlan-http-paired-benchmark --features axum-baseline,benchmark-tools; \
		fi; \
	else \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		"$$cargo_program" $$cargo_args build --release -p terlan --bin terlan-axum-baseline --bin terlan-hyper-baseline --bin terlan-http-framework-benchmark --bin terlan-http-paired-benchmark --features axum-baseline,benchmark-tools; \
	fi
endif
# Runs the docs-light self-test and the docs-light consistency check.
docs-light-check: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) docs-light-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) docs-light-check

# Runs npm audit (lockfile-only, moderate level) across each Node package directory.
node-security-audit-check:
	@for package_dir in editors/vscode tests/js sites/terlan.io tools/css tools/web-toolchain tree-sitter-terlan; do \
		printf 'Auditing locked Node dependencies in %s\n' "$$package_dir"; \
		npm audit --prefix "$$package_dir" --package-lock-only --audit-level=moderate; \
	done
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
.PHONY: vm-scheduler-fairness-check vm-actor-mutator-ownership-check vm-parallel-messages-suite-parity-check vm-efile-suite-parity-check vm-float-native-arithmetic-check vm-fun-suite-parity-check vm-gc-suite-parity-check vm-guard-suite-parity-check vm-guard-no-opt-suite-parity-check vm-hash-suite-parity-check vm-hello-suite-parity-check vm-hibernate-suite-parity-check vm-small-suite-parity-check vm-smoke-suite-parity-check vm-multicore-mailbox-publication-check vm-multicore-fixed-placement-check tvm-aot-multicore-migration-check vm-multicore-work-stealing-policy-check vm-multicore-work-stealing-check vm-multicore-runtime-cleanup-check tvm-aot-multicore-io-epoch-check vm-multicore-timer-epoch-check vm-multicore-timer-scheduler-check vm-multicore-protocol-reactor-check vm-multicore-capability-worker-check vm-multicore-capability-completion-check vm-multicore-capability-event-pump-check vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-multicore-runtime-integration-check vm-multicore-replay-observability-check vm-multicore-performance-record vm-multicore-performance-check vm-multicore-memory-model-check vm-multicore-thread-sanitizer-contract-check vm-multicore-thread-sanitizer-check vm-multicore-release-contract-check vm-multicore-release-record vm-multicore-release-check vm-multicore-publish-evidence-refresh vm-multicore-publish-check
.PHONY: tvm-http-axum-performance-record tvm-http-paired-performance-check
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
.PHONY: comprehension-guards-check comprehension-guards-source-check comprehension-guards-rust-check
.PHONY: lalrpop-parser-parity-check lean-proof-parser-shape-check lean-proof-native-boundary-check lean-proof-track-check lean-proof-smoke-check lean-proof-feature-binding-check lean-proof-change-impact-report lean-proof-feature-binding-review lean-proof-snapshot-consistency-check lean-proof-counterexample-check lean-proof-track-gap-hygiene-check lean-proof-feature-cull-check proof-repro-check proof_repro_check lean-proof-track-pr-gate lean-proof-track-regression-check lean-proof-track-runtime-check lean-proof-track-release-closeout-check lean-proof-templates-routes-check lean-proof-concurrency-check lean-proof-collections-check lean-proof-wasm-bridge-check lean-proof-db-sql-check lean-proof-std-package-check lean-proof-semantic-kernels-check release-artifacts-closeout-check proof-coverage-release-artifacts-smoke proof-readiness-release-mode-check release-evidence-compose release-evidence-refresh release-preflight release-check publish-evidence-refresh publish-evidence-check publish-evidence-plan-check publish-evidence-refresh-plan-check aot-developer-hot-reload-check
.PHONY: function-head-pattern-parameters-check function-head-migration-diagnostic-policy-check function-head-migration-lint-check function-head-pattern-migration-assist-check function-head-pattern-migration-benchmark-check function-head-pattern-migration-docs-check function-head-pattern-0-0-7-handoff-check function-head-pattern-parameters-hardening-check
.PHONY: syntax-contract-check syntax-contract-artifacts syntax-contract-artifacts-check shape-implications-check
.PHONY: shape-synonyms-check
.PHONY: wasm-coreir-lowering-check wasm-runtime-exec-check wasm-contract-discovery-check
.PHONY: flexible-shape-guards-check
.PHONY: callable-syntax-cleanup-check
.PHONY: release-version-channel-check release-version-bump
.PHONY: editor-definition-navigation-check editor-code-action-auto-import-check
.PHONY: terlan-lint-style-profile-check
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
.PHONY: vm-http-benchmark-comparability-check vm-http-soak-stability-check
.PHONY: vm-http-acme-tls-base-check vm-http-acme-tls-production-check vm-http-protocol-readiness-check
.PHONY: vm-otp-abstractions-terlan-stdlib-check

include mk/rust-coverage.mk
include crates/terlan/cli.mk
include std/stdlib.mk
include editors/editor.mk

# Legacy success/skip switches are deliberately unsupported. A caller that
# still supplies one must fail before Make expands any producer, and use the
# live current-input coverage owner instead.
ifneq ($(filter 1,$(TERLAN_RUST_SUITE_ALREADY_RUN) $(TERLAN_CHECK_ALREADY_RUN)),)
$(error legacy Rust/Make skip flags are unsupported; use the live coverage owner)
endif
include mk/code-quality.mk

COVERAGE_MIN ?= 84.20
COVERAGE_VM_RUNNER ?= $(CURDIR)/target/debug/terlan-vm
COVERAGE_IGNORE_FILENAME_REGEX ?= crates/terlan/src/(lsp|vm)/\.\./

TEST_RELEASE_STDLIB_TARGET := stdlib-release-check
LEAN_PROOF_CLOSEOUT_DEPS := lean-proof-track-check
VM_HTTP_BENCHMARK_COMPARABILITY_DEPS := vm-http-concurrency-investigation-check

HTTP_SOAK_PROFILE ?= short
HTTP_SOAK_REPORT = target/quality/vm-http-soak-$(if $(filter release,$(HTTP_SOAK_PROFILE)),release-,)stability-report.json
TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS ?= 3600

ifneq ($(filter release release-plan release-predict release-wait release-tag release-status publish publish-local publish-upload publish-tag publish-preflight publish-prepare,$(MAKECMDGOALS)),)
VERSION ?= $(RELEASE_VERSION)
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

CHECK_GATES := \
	repository-build-release-contract-check \
	source-extension-check \
	release-control-check \
	terlan-self-validation-inventory-check \
	terlan-self-validation-capabilities-check \
	terlan-make-recipe-thinness-check \
	terlan-benchmark-framework-check \
	rust-structure-census-check \
	rust-code-quality-preflight-check \
	build-artifact-budget-check \
	aot-developer-hot-reload-check \
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
	vm-multicore-release-contract-check \
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
.PHONY: terlan-compiler-bootstrap terlan-quality-tools-bootstrap terlan-quality-bootstrap terlan-benchmark-release-bootstrap terlan-native-worker-bootstrap terlan-typed-validator-fingerprint terlan-artifact-measurement-bootstrap terlan-make-recipe-bootstrap terlan-self-validation-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap
# Build the small process owner before the compiler. Linux admits disk/cache
# resources through that same support build; no second Cargo bootstrap is added.
# Prebuilt consumers and read-only publication never request this prerequisite.
ifeq ($(filter 1,$(TERLAN_VALIDATION_BOOTSTRAPPED) $(TERLAN_BUILD_ARTIFACTS_PREBUILT)),)
# Adds the build-owner lock as a prerequisite unless already bootstrapped or prebuilt.
terlan-compiler-bootstrap: | terlan-build-owner-bootstrap
ifeq ($(shell uname -s),Linux)
# Adds disk/cache resource admission as a prerequisite on Linux.
terlan-compiler-bootstrap: | rust-build-resource-admission
endif
endif
# Builds terlc, terlan-vm, and terlan-native-worker, caching the build via the build-owner lock.
terlan-compiler-bootstrap:
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
	test -x $(TERLAN_BOOTSTRAP_COMPILER)
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -x target/debug/terlan-native-worker
	test -x $(TERLAN_RUST_ORCHESTRATOR)
else
	@if test "$(shell uname -s)" = Linux; then \
		set -eu; \
		if (set -o pipefail) 2>/dev/null; then set -o pipefail; fi; \
		mkdir -p target/quality; \
		build_command='$(CARGO) build $(TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS)'; \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		if git diff --quiet \
			&& git diff --cached --quiet \
			&& test -z "$$(git ls-files --others --exclude-standard)"; then \
			revision="$$(git rev-parse --verify HEAD)"; \
			input="$$( { printf '%s\n' "$$revision"; sha256sum Cargo.toml Cargo.lock rust-toolchain.toml crates/terlan/Cargo.toml; rustc --version; cargo --version; printf '%s\n' cargo --locked build $(TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS); sha256sum "$(TERLAN_BUILD_CACHE)"; } | sha256sum | awk '{print $$1}')"; \
			$(TERLAN_BOOTSTRAP_LOCK) \
			"$(TERLAN_BUILD_CACHE)" owner \
				--cargo "$(firstword $(CARGO))" \
				--source-revision "$$revision" \
				--receipt "target/quality/preparation/bootstrap/$$revision/compiler.json" \
				--input-sha256 "$$input" \
				--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
				--output "$(TERLAN_BOOTSTRAP_COMPILER)" \
				--output "$(TERLAN_BOOTSTRAP_VM)" \
				--output "target/debug/terlan-native-worker" \
				--output "$(patsubst $(CURDIR)/%,%,$(TERLAN_RUST_ORCHESTRATOR))" \
				-- /bin/sh -c "$$build_command"; \
		else \
			$(TERLAN_BOOTSTRAP_LOCK) \
			$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$$cargo_program" $$cargo_args build $(TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS); \
		fi; \
	else \
		$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$(firstword $(CARGO))" $(wordlist 2,99,$(CARGO)) build $(TERLAN_COMPILER_BOOTSTRAP_BUILD_ARGS); \
	fi
endif
	@stamp=target/debug/.terlan-prebuilt-terlc.profile; \
	temporary="$$stamp.tmp"; \
	printf '%s\n' validation-union > "$$temporary"; \
	mv "$$temporary" "$$stamp"
endif

# Seal the quality and structural-analysis CLIs in one Cargo invocation. The
# release aggregate orders this after the canonical suite, while a focused
# quality target can request the tools without running the entire test suite.
terlan-quality-tools-bootstrap:
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifneq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
# Adds the build-owner lock as a prerequisite when not already bootstrapped or prebuilt.
terlan-quality-tools-bootstrap: | terlan-build-owner-bootstrap
endif
ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
	@for binary in \
		terlan-quality \
		terlan-native-target-feasibility \
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
	@if test "$(shell uname -s)" = Linux; then \
		set -eu; \
		if (set -o pipefail) 2>/dev/null; then set -o pipefail; fi; \
		mkdir -p target/quality; \
		build_command='$(CARGO) build -p terlan -p terlan-rust-boundary-audit --bin terlan-quality --bin terlan-native-target-feasibility --bin terlan-accelerator-value-contract --bin terlan-accelerator-target-admission --bin terlan-accelerator-ir --bin terlan-accelerator-aot-backend --bin terlan-accelerator-placement --bin terlan-accelerator-vm-integration --bin terlan-accelerator-specialized-artifact --bin terlan-rust-boundary-audit --features terlan/validation-union'; \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		if git diff --quiet \
			&& git diff --cached --quiet \
			&& test -z "$$(git ls-files --others --exclude-standard)"; then \
			revision="$$(git rev-parse --verify HEAD)"; \
			input="$$( { printf '%s\n' "$$revision"; sha256sum Cargo.toml Cargo.lock rust-toolchain.toml crates/terlan/Cargo.toml tools/rust_boundary_audit/Cargo.toml; rustc --version; cargo --version; printf '%s\n' cargo --locked build quality-tools; sha256sum "$(TERLAN_BUILD_CACHE)"; } | sha256sum | awk '{print $$1}')"; \
			$(TERLAN_BOOTSTRAP_LOCK) \
			"$(TERLAN_BUILD_CACHE)" owner \
				--cargo "$(firstword $(CARGO))" \
				--source-revision "$$revision" \
				--receipt "target/quality/preparation/bootstrap/$$revision/quality-tools.json" \
				--input-sha256 "$$input" \
				--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
				--output target/debug/terlan-quality \
				--output target/debug/terlan-native-target-feasibility \
				--output target/debug/terlan-accelerator-value-contract \
				--output target/debug/terlan-accelerator-target-admission \
				--output target/debug/terlan-accelerator-ir \
				--output target/debug/terlan-accelerator-aot-backend \
				--output target/debug/terlan-accelerator-placement \
				--output target/debug/terlan-accelerator-vm-integration \
				--output target/debug/terlan-accelerator-specialized-artifact \
				--output target/debug/terlan-rust-boundary-audit \
				-- /bin/sh -c "$$build_command"; \
		else \
			$(TERLAN_BOOTSTRAP_LOCK) \
				$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$$cargo_program" $$cargo_args build -p terlan -p terlan-rust-boundary-audit --bin terlan-quality --bin terlan-native-target-feasibility --bin terlan-accelerator-value-contract --bin terlan-accelerator-target-admission --bin terlan-accelerator-ir --bin terlan-accelerator-aot-backend --bin terlan-accelerator-placement --bin terlan-accelerator-vm-integration --bin terlan-accelerator-specialized-artifact --bin terlan-rust-boundary-audit --features terlan/validation-union; \
		fi; \
	else \
		$(CARGO) build \
			-p terlan \
			-p terlan-rust-boundary-audit \
			--bin terlan-quality \
			--bin terlan-native-target-feasibility \
			--bin terlan-accelerator-value-contract \
			--bin terlan-accelerator-target-admission \
			--bin terlan-accelerator-ir \
			--bin terlan-accelerator-aot-backend \
			--bin terlan-accelerator-placement \
			--bin terlan-accelerator-vm-integration \
			--bin terlan-accelerator-specialized-artifact \
			--bin terlan-rust-boundary-audit \
			--features terlan/validation-union; \
	fi
endif
endif
	@stamp=target/debug/.terlan-prebuilt-terlan-quality.profile; \
	temporary="$$stamp.tmp"; \
	printf '%s\n' validation-union > "$$temporary"; \
	mv "$$temporary" "$$stamp"

ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
ifneq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
# Adds the build-owner lock as a prerequisite when not already bootstrapped or prebuilt.
terlan-benchmark-release-bootstrap: | terlan-build-owner-bootstrap
endif
endif
# Builds the release terlc, terlan-vm, and terlan-benchmark binaries.
terlan-benchmark-release-bootstrap:
ifeq ($(TERLAN_RELEASE_BINARIES_PREBUILT),1)
	test -x target/release/terlc
	test -x target/release/terlan-vm
	test -x target/release/terlan-benchmark
else
	@if test "$(shell uname -s)" = Linux; then \
		set -eu; \
		if (set -o pipefail) 2>/dev/null; then set -o pipefail; fi; \
		mkdir -p target/quality; \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		build_command="$$cargo_program $$cargo_args build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark --features benchmark-tools"; \
		if git diff --quiet \
			&& git diff --cached --quiet \
			&& test -z "$$(git ls-files --others --exclude-standard)"; then \
			revision="$$(git rev-parse --verify HEAD)"; \
			input="$$( { printf '%s\n' "$$revision"; sha256sum Cargo.toml Cargo.lock rust-toolchain.toml crates/terlan/Cargo.toml; rustc --version; cargo --version; printf '%s\n' benchmark-release-tools; sha256sum "$(TERLAN_BUILD_CACHE)"; } | sha256sum | awk '{print $$1}')"; \
			$(TERLAN_BOOTSTRAP_LOCK) \
			"$(TERLAN_BUILD_CACHE)" owner \
				--cargo "$(firstword $(CARGO))" \
				--source-revision "$$revision" \
				--receipt "target/quality/preparation/bootstrap/$$revision/benchmark-release.json" \
				--input-sha256 "$$input" \
				--timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" \
				--output target/release/terlc \
				--output target/release/terlan-vm \
				--output target/release/terlan-benchmark \
				-- /bin/sh -c "$$build_command"; \
		else \
			$(TERLAN_BOOTSTRAP_LOCK) \
			$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- "$$cargo_program" $$cargo_args build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark --features benchmark-tools; \
		fi; \
	else \
		cargo_program='$(firstword $(CARGO))'; \
		cargo_args='$(wordlist 2,99,$(CARGO))'; \
		"$$cargo_program" $$cargo_args build --release -p terlan --bin terlc --bin terlan-vm --bin terlan-benchmark --features benchmark-tools; \
	fi
endif

ifeq ($(TERLAN_BUILD_ARTIFACTS_PREBUILT),1)
# Verifies the prebuilt terlan-native-worker binary exists.
terlan-native-worker-bootstrap:
	test -x target/debug/terlan-native-worker
else
# Builds the compiler first, then verifies terlan-native-worker exists.
terlan-native-worker-bootstrap: terlan-compiler-bootstrap
	test -x target/debug/terlan-native-worker
endif

# Records the compiler toolchain identity fingerprint used by typed validator builds.
terlan-typed-validator-fingerprint: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	@identity=$$($(TERLAN_BOOTSTRAP_COMPILER) build --print-toolchain-identity) || exit $$?; \
	TERLAN_TYPED_VALIDATOR_TOOLCHAIN_IDENTITY="$$identity" \
	$(TERLAN_TYPED_VALIDATOR_BUILD) fingerprint \
		$(TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT) \
		$(TERLAN_BOOTSTRAP_COMPILER) std
endif

# Shares the toolchain fingerprint prerequisite across all typed-validator bootstrap targets.
terlan-make-recipe-bootstrap terlan-ebnf-validator-bootstrap terlan-shared-helper-bootstrap terlan-external-package-matrix-bootstrap terlan-tvm-package-consumer-bootstrap terlan-tvm-platform-matrix-bootstrap terlan-rust-quality-bootstrap terlan-docs-static-release-parity-bootstrap terlan-release-promotion-bootstrap terlan-release-closeout-bootstrap terlan-web-manifest-preflight-bootstrap terlan-self-validation-checkout-bootstrap terlan-stdlib-validation-bootstrap terlan-repository-validation-bootstrap: terlan-typed-validator-fingerprint

# Builds the typed BuildArtifactBudgetTest validator image used for artifact-size checks.
terlan-artifact-measurement-bootstrap: terlan-compiler-bootstrap
# Measurement removes target with cargo clean, including this validator. Admit
# it again before the gate graph; only an already-bootstrapped consumer skips it.
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SELF_VALIDATION_IMAGE) \
		$(TERLAN_BOOTSTRAP_COMPILER) std scripts/self_validation/BuildArtifactBudgetTest.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/BuildArtifactBudgetTest.terl --out-dir $(TERLAN_SELF_VALIDATION_DIR)
endif
	test -s $(TERLAN_SELF_VALIDATION_IMAGE)

# Builds the typed MakeRecipeThinness validator image for the terlan-vm target.
terlan-make-recipe-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_MAKE_RECIPE_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/MakeRecipeThinness.terls \
		scripts/self_validation/MakeRecipeThinnessMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/MakeRecipeThinness.terls \
		--target terlan-vm --out-dir $(TERLAN_MAKE_RECIPE_DIR)
	test -s $(TERLAN_MAKE_RECIPE_IMAGE)
endif

# Builds the typed EbnfValidator validator image for the terlan-vm target.
terlan-ebnf-validator-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_EBNF_VALIDATOR_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/EbnfValidator.terls \
		scripts/self_validation/EbnfValidatorMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/EbnfValidator.terls \
		--target terlan-vm --out-dir $(TERLAN_EBNF_VALIDATOR_DIR)
	test -s $(TERLAN_EBNF_VALIDATOR_IMAGE)
endif

# Builds the typed SharedHelperCheck validator image for the terlan-vm target.
terlan-shared-helper-bootstrap: terlan-compiler-bootstrap terlan-validation-vm-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SHARED_HELPER_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/SharedHelperCheck.terls \
		scripts/self_validation/SharedHelperCheckMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/SharedHelperCheck.terls \
		--target terlan-vm --out-dir $(TERLAN_SHARED_HELPER_DIR)
	test -s $(TERLAN_SHARED_HELPER_IMAGE)
endif

# Builds the typed ExternalPackageExecutionMatrix validator image for the terlan-vm target.
terlan-external-package-matrix-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/ExternalPackageExecutionMatrix.terls \
		scripts/self_validation/ExternalPackageExecutionMatrixMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/ExternalPackageExecutionMatrix.terls \
		--target terlan-vm --out-dir $(TERLAN_EXTERNAL_PACKAGE_MATRIX_DIR)
	test -s $(TERLAN_EXTERNAL_PACKAGE_MATRIX_IMAGE)
endif

# Builds the typed TvmPackageInstallConsumer validator image for the terlan-vm target.
terlan-tvm-package-consumer-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_TVM_PACKAGE_CONSUMER_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/TvmPackageInstallConsumer.terls \
		scripts/self_validation/TvmPackageInstallConsumerMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/TvmPackageInstallConsumer.terls \
		--target terlan-vm --out-dir $(TERLAN_TVM_PACKAGE_CONSUMER_DIR)
	test -s $(TERLAN_TVM_PACKAGE_CONSUMER_IMAGE)
endif

# Builds the typed TvmAotPlatformMatrix validator image for the terlan-vm target.
terlan-tvm-platform-matrix-bootstrap: terlan-compiler-bootstrap terlan-tvm-package-consumer-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/tvm_aot_platform_matrix -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/tvm_aot_platform_matrix/scripts/TvmAotPlatformMatrix.terls \
		--target terlan-vm --out-dir $(TERLAN_TVM_PLATFORM_MATRIX_DIR)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
endif

.PHONY: terlan-validation-vm-bootstrap
# Builds the release terlan-vm binary (unless prebuilt/bootstrapped) and verifies it exists.
terlan-validation-vm-bootstrap: terlan-compiler-bootstrap
ifeq ($(filter 1,$(TERLAN_VALIDATION_BOOTSTRAPPED) $(TERLAN_VALIDATION_VM_PREBUILT) $(TERLAN_RELEASE_BINARIES_PREBUILT)),)
	$(TERLAN_BOOTSTRAP_LOCK) \
		$(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds "$(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS)" -- \
		$(CARGO) build --release -p terlan --bin terlan-vm
endif
	test -x $(TERLAN_VALIDATION_VM)

# Builds the typed RustQuality validator image for the terlan-vm target.
terlan-rust-quality-bootstrap: terlan-compiler-bootstrap terlan-validation-vm-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RUST_QUALITY_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/rust_quality -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/rust_quality/scripts/RustQuality.terls \
		--target terlan-vm --out-dir $(TERLAN_RUST_QUALITY_DIR)
	test -s $(TERLAN_RUST_QUALITY_IMAGE)
endif

# Builds the typed ReleasePromotion validator image for the terlan-vm target.
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
# Verifies the already-sealed release closeout image exists.
terlan-release-closeout-bootstrap:
	test -s $(TERLAN_RELEASE_CLOSEOUT_IMAGE)
else
# Builds the release closeout image and checks the release artifact matrix.
terlan-release-closeout-bootstrap: terlan-release-closeout-image-bootstrap vm-release-artifact-matrix-check
endif

.PHONY: terlan-release-closeout-image-bootstrap
# Builds the typed ReleaseCloseout validator image for the terlan-vm target.
terlan-release-closeout-image-bootstrap: terlan-compiler-bootstrap terlan-typed-validator-fingerprint
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RELEASE_CLOSEOUT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/release_closeout \
		scripts/self_validation/rust_quality -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/release_closeout/scripts/ReleaseCloseout.terls \
		--target terlan-vm --out-dir $(TERLAN_RELEASE_CLOSEOUT_DIR)
	test -s $(TERLAN_RELEASE_CLOSEOUT_IMAGE)
endif

# Builds the typed WebManifestPreflight validator image for the terlan-vm target.
terlan-web-manifest-preflight-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/WebManifestPreflight.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/WebManifestPreflight.terls \
		--target terlan-vm --out-dir $(TERLAN_WEB_MANIFEST_PREFLIGHT_DIR)
	test -s $(TERLAN_WEB_MANIFEST_PREFLIGHT_IMAGE)
endif

# Builds the typed SelfValidationCheckout validator image for the terlan-vm target.
terlan-self-validation-checkout-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/SelfValidationCheckout.terls -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/SelfValidationCheckout.terls \
		--target terlan-vm --out-dir $(TERLAN_SELF_VALIDATION_CHECKOUT_DIR)
	test -s $(TERLAN_SELF_VALIDATION_CHECKOUT_IMAGE)
endif

# Builds the typed StdlibValidation validator image for the terlan-vm target.
terlan-stdlib-validation-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_STDLIB_VALIDATION_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/StdlibValidation.terls \
		scripts/self_validation/StdlibValidationMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/StdlibValidation.terls \
		--target terlan-vm --out-dir $(TERLAN_STDLIB_VALIDATION_DIR)
	test -s $(TERLAN_STDLIB_VALIDATION_IMAGE)
endif

# Builds the typed docs_static_release_parity validator image for the terlan-vm target.
terlan-docs-static-release-parity-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(DOCS_STATIC_RELEASE_PARITY_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/docs_static_release_parity -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/docs_static_release_parity \
		--target terlan-vm --out-dir $(DOCS_STATIC_RELEASE_PARITY_DIR)
	test -s $(DOCS_STATIC_RELEASE_PARITY_IMAGE)
endif

# Builds the typed RepositoryValidation validator image for the terlan-vm target.
terlan-repository-validation-bootstrap: terlan-compiler-bootstrap
ifneq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_REPOSITORY_VALIDATION_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/self_validation/RepositoryValidation.terls \
		scripts/self_validation/RepositoryValidationMain.terl -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) \
		scripts/self_validation/RepositoryValidation.terls \
		--target terlan-vm --out-dir $(TERLAN_REPOSITORY_VALIDATION_DIR)
	test -s $(TERLAN_REPOSITORY_VALIDATION_IMAGE)
endif
# Adds the compiler-bootstrap prerequisite to the whole compiler-consumer gate list.
$(TERLAN_COMPILER_CONSUMER_GATES): | terlan-compiler-bootstrap
# Adds the quality-tools bootstrap prerequisite to these lean-proof and proof-repro checks.
lean-proof-parser-shape-check lean-proof-feature-cull-check lean-proof-native-boundary-check lean-proof-smoke-check proof-repro-check lean-proof-track-runtime-check lean-proof-track-pr-gate lean-proof-track-regression-check: | terlan-quality-tools-bootstrap
# Adds the compiler-bootstrap prerequisite to the lalrpop/lean-proof-lanes contract checks.
lalrpop-grammar-contract-check lean-proof-lanes-check lean-proof-feature-binding-contract-check: | terlan-compiler-bootstrap
# Also adds the quality-tools bootstrap prerequisite to the same lean-proof lanes.
lean-proof-templates-routes-check lean-proof-concurrency-check lean-proof-collections-check lean-proof-wasm-bridge-check lean-proof-db-sql-check lean-proof-std-package-check lean-proof-semantic-kernels-check: | terlan-quality-tools-bootstrap

# Each invocation has one correctness owner; ambiguous aggregate requests must
# not run one suite with whichever target-specific environment happens to win.
ifneq ($(word 2,$(filter check release-check release-evidence-refresh,$(MAKECMDGOALS))),)
$(error choose one correctness entry point: check, release-check, or release-evidence-refresh)
endif

ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)
# Requests whole-suite coverage from the orchestrator when a coverage context is active.
rust-test-suite:
	$(TERLAN_RUST_ORCHESTRATOR) --coverage-request --whole-suite
else
# Validation never deletes artifacts needed by later consumers. Retention runs
# only after the complete check graph has finished.
rust-test-suite: rust-cargo-metadata-report
	@set -eu; \
	for binary in terlc terlan-vm terlan-native-worker; do \
		test -x "target/debug/$$binary"; \
	done; \
	test -x $(TERLAN_RUST_ORCHESTRATOR); \
	TERLAN_RUST_SUITE_REPORT=$(CURDIR)/target/quality/rust-test-suite-report.json \
	TERLAN_RUST_PHASE_RECEIPT_DIR=$${TERLAN_RUST_PHASE_RECEIPT_DIR:-$${CARGO_TARGET_DIR:-$(CURDIR)/target}/validation-results/rust-phases} \
		$(TERLAN_RUST_ORCHESTRATOR)
	test -s target/quality/rust-test-suite-report.json
endif

ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)
# Composes release evidence from all gates when a live coverage owner is bound
release-evidence-compose: $(RELEASE_EVIDENCE_GATES)
	@echo "[release-evidence-compose] version $(RELEASE_VERSION) candidate-bound evidence composed"
else
# Fails fast when no live coverage owner is available to compose evidence
release-evidence-compose:
	@echo 'release evidence requires a live coverage owner; run make release-evidence-refresh' >&2
	@exit 1
endif

# Benchmarks the native AOT HTTP server against the checked-CoreIR baseline
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

ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)
# Runs publish gates directly when hosted Rust coverage context is active
vm-multicore-publish-prerequisites: vm-multicore-release-contract-check $(VM_MULTICORE_PUBLISH_LOCAL_GATES)

# Refreshes the multicore release record as part of publish prerequisites
vm-multicore-publish-evidence-refresh: vm-multicore-release-contract-check vm-multicore-publish-prerequisites | terlan-release-promotion-bootstrap
	$(TERLAN_PREPARATION_OWNER) \
		$(TERLAN_RELEASE_PROMOTION) prepare-multicore-release-record \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"
else
# Re-invokes make under the hosted-coverage orchestrator outside a coverage context
vm-multicore-publish-prerequisites vm-multicore-publish-evidence-refresh: | terlan-release-promotion-bootstrap
	$(TERLAN_RUST_ORCHESTRATOR) --with-hosted-cargo-coverage -- $(MAKE) --no-print-directory $@
endif

# Verifies release failure reproduction tooling
release-failure-reproduction-check: release-gate-report-schema-check
	$(TERLAN_QUALITY) release-failure-reproduction

GENERATED_ARTIFACT_FRESHNESS_GATES := \
	stdlib-summary-drift-check \
	stdlib-js-bindings-drift-check \
	stdlib-native-artifacts-check \
	stdlib-release-manifest-check \
	tree-sitter-cli-check

TERLAN_GENERATED_SUMMARIES ?= 1
TERLAN_GENERATED_JS_BINDINGS ?= 1
TERLAN_GENERATED_NATIVE_ARTIFACTS ?= 1
TERLAN_GENERATED_RELEASE_MANIFEST ?= 1
TERLAN_GENERATED_TREE_SITTER ?= 1
ifneq ($(strip $(filter-out 0 1,$(TERLAN_GENERATED_SUMMARIES) $(TERLAN_GENERATED_JS_BINDINGS) $(TERLAN_GENERATED_NATIVE_ARTIFACTS) $(TERLAN_GENERATED_RELEASE_MANIFEST) $(TERLAN_GENERATED_TREE_SITTER))),)
$(error generated artifact selectors must be 0 or 1)
endif
SELECTED_GENERATED_ARTIFACT_FRESHNESS_GATES := \
	$(if $(filter 1,$(TERLAN_GENERATED_SUMMARIES)),stdlib-summary-drift-check) \
	$(if $(filter 1,$(TERLAN_GENERATED_JS_BINDINGS)),stdlib-js-bindings-drift-check) \
	$(if $(filter 1,$(TERLAN_GENERATED_NATIVE_ARTIFACTS)),stdlib-native-artifacts-check) \
	$(if $(filter 1,$(TERLAN_GENERATED_RELEASE_MANIFEST)),stdlib-release-manifest-check) \
	$(if $(filter 1,$(TERLAN_GENERATED_TREE_SITTER)),tree-sitter-cli-check)

RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES := $(GENERATED_ARTIFACT_FRESHNESS_GATES)

.PHONY: source-generated-artifacts-check
# Runs the selected set of generated-artifact freshness gates
source-generated-artifacts-check:
	@test -n "$(strip $(SELECTED_GENERATED_ARTIFACT_FRESHNESS_GATES))" || { echo 'no generated artifact owner selected' >&2; exit 2; }
	$(MAKE) --no-print-directory -j5 $(SELECTED_GENERATED_ARTIFACT_FRESHNESS_GATES)
	@echo "[source-preflight] every generated artifact is current"

.PHONY: release-generated-artifacts-before release-generated-artifacts-freshness-pass release-generated-artifacts-check

# Capture before any shared freshness producer, including producers reached first
# through check-gates or publication source prerequisites. Non-release focused
# checks do not pay for an unrelated full-inventory snapshot.
ifneq ($(strip $(filter 1,$(TERLAN_CHECK_RELEASE_EVIDENCE))$(filter hosted-source,$(TERLAN_RUST_COVERAGE_SCOPE))$(filter release-evidence-compose release-generated-artifacts-before release-generated-artifacts-check release-generated-artifacts-freshness-pass,$(MAKECMDGOALS))),)
RELEASE_GENERATED_ARTIFACT_SNAPSHOT_REQUIRED := 1
# Makes each freshness gate depend on the shared before-snapshot when required
$(RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES): | release-generated-artifacts-before
else
RELEASE_GENERATED_ARTIFACT_SNAPSHOT_REQUIRED := 0
endif

publish-preparation-cold: publish-preparation-checks publish-preparation-verified publish-cache-prune
# Warm-path preparation: checks, reuse staged distribution, then cache prune
publish-preparation-warm: publish-preparation-checks publish-staged-distribution-prepare publish-cache-prune

# Cache retirement cannot race refresh/restoration; warm installation must not
# proceed after failed source checks. Keep these edges local to these entrypoints
# so ordinary development checks retain their focused dependency graphs.
ifneq ($(filter publish-preparation-cold,$(MAKECMDGOALS)),)
ifneq ($(filter publish-preparation-warm,$(MAKECMDGOALS)),)
$(error preparation selects either the cold or warm graph, never both)
endif
# Adds the full checks and verified-evidence prerequisites when targeting the cold path
publish-cache-prune: publish-preparation-checks publish-preparation-verified
endif
ifneq ($(filter publish-preparation-warm,$(MAKECMDGOALS)),)
# Adds the preparation checks prerequisite when targeting the warm path
publish-cache-prune: publish-preparation-checks
# Refreshes staged-distribution verification after checks and cache pruning complete
release-staged-distribution-verification-refresh: publish-preparation-checks publish-cache-prune
endif
.PHONY: terlan-build-owner-bootstrap rust-build-cache-bootstrap rust-build-resource-admission rust-incremental-cache-audit rust-incremental-cache-prune
# Builds the Rust test orchestrator and installs its build-owner snapshot
terlan-build-owner-bootstrap:
ifeq ($(shell uname -s),Linux)
	$(MAKE) --no-print-directory -f mk/hermetic-support.mk hermetic-support-root
else
	CARGO_BUILD_JOBS=1 $(CARGO) build -p terlan-test-orchestrator
endif
	target/debug/terlan-test-orchestrator --install-snapshot $(TERLAN_RUST_ORCHESTRATOR)

# Bootstraps validation tooling, then runs the covered evidence gates (dry-run aware)
publish-evidence-refresh: release-version-metadata-check terlan-self-validation-bootstrap terlan-release-closeout-image-bootstrap terlan-quality-tools-bootstrap terlan-native-worker-bootstrap terlan-benchmark-release-bootstrap publish-preparation-admission
	@case "$${MAKEFLAGS%% *}" in \
		*n*) TERLAN_RUST_COVERAGE_SCOPE=hosted-source \
			TERLAN_RUST_COVERAGE_CONTEXT=dry-run:no-live-owner \
			$(MAKE) --no-print-directory publish-evidence-covered-gates ;; \
		*) TERLAN_VALIDATION_BOOTSTRAPPED=1 \
			TERLAN_RELEASE_BINARIES_PREBUILT=1 \
			TERLAN_TEST_PHASE_TIMEOUT_SECONDS=$${TERLAN_TEST_PHASE_TIMEOUT_SECONDS:-14400} \
			timeout 14400s $(TERLAN_RUST_ORCHESTRATOR) --with-hosted-cargo-coverage -- $(MAKE) --no-print-directory publish-evidence-covered-gates ;; \
	esac

# One live owner admits hosted source coverage before any source-test gate runs.
# Ordinary prerequisites share overlapping multicore/AOT nodes within this Make.
# Local report production and staged-distribution validation still execute locally.
.PHONY: publish-evidence-source-prerequisites publish-evidence-covered-gates publish-evidence-staged-inputs
ifeq ($(TERLAN_RUST_COVERAGE_SCOPE),hosted-source)
ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)
POLICY_CARGO_TEST = $(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds $(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS) -- bash scripts/run_exact_cargo_test.sh
# Runs multicore, local AOT, and closeout-contract gates before source coverage
publish-evidence-source-prerequisites: vm-multicore-publish-prerequisites $(AOT_RELEASE_LOCAL_GATES) tvm-aot-release-closeout-contract-check
	$(AOT_RELEASE_CARGO_CHECK)

# Composes release evidence after the proof lanes check passes
publish-evidence-covered-gates: release-evidence-compose lean-proof-lanes-check
# Release-profile measurements execute under their own producer, never debug coverage.
tvm-managed-list-profile-benchmark-check: TERLAN_MANAGED_LIST_PROFILE_RUN = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-managed-list-profile
# Requires the promotion bootstrap and preparation lock before benchmarking
tvm-managed-list-profile-benchmark-check: | terlan-release-promotion-bootstrap publish-preparation-lock-directory
# The runtime prerequisite owns all three source-only policy reports under one
# shared preflight. PR/regression recipes retain their exact Rust coverage
# requests, but their report producers have already succeeded in that graph.
# The native consumer may only verify completed replicas, and must not race
# the proof owner for the preparation lease under parallel Make.
# Requires the promotion bootstrap and preparation lock before this check runs
# Requires the promotion bootstrap and preparation lock before this check runs
# The binding owner executes and seals the entire ordered snapshot chain once.
# Requires the promotion bootstrap and preparation lock before the lanes run
# Depends on source prerequisites before the proof-track runtime check runs
# Requires the promotion bootstrap and preparation lock before the smoke check
# Depends on the proof repro check completing before the PR gate runs
# Depends on the PR gate completing before the regression check runs
# Requires the promotion bootstrap and preparation lock for all three track checks
# Depends on source prerequisites before the proof repro check runs
# Requires the promotion bootstrap, semantic-kernel bootstrap, and preparation lock
# This graph's semantic owner consumes the completed track with the same fresh
# tool admission; it cannot execute a missing proof or publish a partial report.
# Depends on the proof repro check before verifying semantic kernels
release-staged-distribution-verification-check: TERLAN_STAGED_DISTRIBUTION_RUN = $(TERLAN_OWNED_STAGED_DISTRIBUTION)
release-readiness-attestation-check: TERLAN_READINESS_RUN = $(TERLAN_OWNED_READINESS)
# Depends on staged evidence inputs and the promotion bootstrap before closeout
release-artifacts-closeout-check: publish-evidence-staged-inputs | terlan-release-promotion-bootstrap
# Depends on the artifacts closeout check before attesting readiness
release-readiness-attestation-check: release-artifacts-closeout-check

# Report and distribution consumers wait for this producer, while independent
# source/proof gates may share prerequisites with source admission in parallel.
# These are data dependencies, so -k cannot seal evidence after staging fails.
release-evidence-compose: publish-evidence-staged-inputs
# Adds staged evidence inputs as an order-only prerequisite for both targets
terlan-release-closeout-bootstrap release-generated-artifacts-check: | publish-evidence-staged-inputs

# Finish the proof consumers before another producer takes the same lease.
publish-evidence-staged-inputs: publish-evidence-source-prerequisites lean-proof-lanes-check
	$(TERLAN_PREPARATION_OWNER) \
		timeout 900s $(TERLAN_RELEASE_PROMOTION) prepare-release-reports \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"
	timeout 900s bash scripts/download_validated_release_artifacts.sh "$$(git rev-parse HEAD)" --restore
	timeout 900s $(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-matrix
else
# Fails fast: hosted-source scope selected without an active coverage-owner context
publish-evidence-source-prerequisites publish-evidence-covered-gates publish-evidence-staged-inputs:
	@echo 'publication source gates require a live hosted coverage owner' >&2
	@exit 1
endif
else
# Fails fast outside hosted-source scope; callers must go through publish-evidence-refresh
publish-evidence-source-prerequisites publish-evidence-covered-gates publish-evidence-staged-inputs:
	@echo 'run make publish-evidence-refresh to admit hosted source coverage' >&2
	@exit 1
endif

.PHONY: release release-plan release-predict release-wait release-tag publish-tag release-status release-control-check terlan-release-control-fingerprint terlan-release-control-bootstrap
# Release orchestration is a consumer of the latest working Terlan tools. It
# receives prebuilt candidate tools during publication; hosted compiler
# evidence validates those inputs independently.
# The shared fingerprint is a real data dependency: the parallel validator
# bootstrap must never test for it before its canonical producer completes.
terlan-release-control-fingerprint: terlan-typed-validator-fingerprint
	test -x $(TERLAN_BOOTSTRAP_COMPILER)
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_TYPED_VALIDATOR_COMMON_FINGERPRINT)

# Builds the release-control VM image, reusing it when already validated
terlan-release-control-bootstrap: terlan-release-control-fingerprint
ifeq ($(TERLAN_VALIDATION_BOOTSTRAPPED),1)
	test -s $(TERLAN_RELEASE_CONTROL_IMAGE)
else
	$(TERLAN_TYPED_VALIDATOR_BUILD) $(TERLAN_RELEASE_CONTROL_IMAGE) \
		$(TERLAN_TYPED_VALIDATOR_COMMON_INPUTS) scripts/release_control -- \
		$(TERLAN_BOOTSTRAP_COMPILER_BUILD) scripts/release_control/scripts/Release.terls --target terlan-vm --out-dir target/release-control
endif


# ============================================================
# Targets below are grouped by subsystem (grouping pass; a small
# number of conditionally-guarded targets above were left in place
# to avoid splitting an ifeq/ifneq block across sections).
# ============================================================

# ---- Primary entry points and checks not owned by a specific subsystem below ----

# Runs the Rust test suite, builds the quality/self-validation tooling, then runs all check gates.
check: rust-test-suite
	$(MAKE) --no-print-directory \
		terlan-quality-tools-bootstrap
	TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		$(MAKE) --no-print-directory --jobs=$(TERLAN_VALIDATOR_BUILD_JOBS) \
		terlan-self-validation-bootstrap \
		$(if $(filter 1,$(TERLAN_CHECK_RELEASE_EVIDENCE)),terlan-release-closeout-image-bootstrap)
	TERLAN_RUST_SUITE_REPORT=$(CURDIR)/target/quality/rust-test-suite-report.json \
		$(TERLAN_RUST_ORCHESTRATOR) --with-cargo-coverage \
		$(CURDIR)/target/quality/rust-test-suite-report.json \
		--graph-timeout-seconds "$(TERLAN_CHECK_GRAPH_TIMEOUT_SECONDS)" -- \
		$(MAKE) --no-print-directory \
		check-gates $(if $(filter 1,$(TERLAN_CHECK_RELEASE_EVIDENCE)),release-evidence-compose)
	$(MAKE) --no-print-directory rust-artifact-retention-check


# Aggregates every target listed in CHECK_GATES.
check-gates: $(CHECK_GATES)


# Runs the editor-release-parity VM entries (extract, surface, smokes, freshness, lsp, adversarial, closeout, size).
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


# Runs the docs/static release-parity validator's main entry.
docs-static-release-parity-check: | terlan-docs-static-release-parity-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(DOCS_STATIC_RELEASE_PARITY_IMAGE) \
			--entry docs_static_release_parity.Main.run_entry

.PHONY: terlan-make-recipe-thinness-check
# Runs the make-recipe-thinness validator image against the repository root.
terlan-make-recipe-thinness-check: | terlan-make-recipe-bootstrap
	TERLAN_MAKE_RECIPE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_MAKE_RECIPE_IMAGE) --script-eval

.PHONY: terlan-benchmark-framework-check
# Runs the benchmark framework test, a sample benchmark pass, and validates emitted JSON evidence.
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
# Runs the full check suite plus the package/cuda/external-package experimental checks.
check-experimental: check
	$(MAKE) --no-print-directory package-test-exec-check
	$(MAKE) --no-print-directory cuda-package-check
	TERLAN_EXTERNAL_PACKAGE_PROFILE=experimental \
		$(MAKE) --no-print-directory external-package-execution-matrix-check

.PHONY: value-lifecycle-contract-check
# Runs value-lifecycle/const-eval/macro/REPL/completion Rust tests plus VM and JS target compiler tests.
value-lifecycle-contract-check: validate-ebnf tree-sitter-cli-check editor-check
	$(RUST_TEST) -p terlan --lib value_lifecycle_test -- --nocapture
	$(RUST_TEST) -p terlan --lib const_eval_test -- --nocapture
	$(RUST_TEST) -p terlan --lib expression_macro -- --nocapture
	$(RUST_TEST) -p terlan --lib value_lifecycle_ -- --nocapture
	$(RUST_TEST) -p terlan --lib repl_rejects_local_constants_but_accepts_constant_imports -- --nocapture
	$(RUST_TEST) -p terlan --lib --features validation-union completion_items_include_local_and_imported_shapes_and_functions -- --nocapture
	$(RUST_TEST) -p terlan --lib --features validation-union value_lifecycle_ -- --nocapture
	$(TERLC) test tests/language/ValueLifecycleTest.terl --target terlan-vm
	$(TERLC) --target-profile js.shared test tests/language/ValueLifecycleTest.terl --target js


# Alias for the cli-test target.
test: cli-test


# Runs the release CLI test, release-train check, and release stdlib check.
test-release: cli-test-release terlan-release-train-check $(TEST_RELEASE_STDLIB_TARGET)

.PHONY: dev-check dev-vm-check dev-web-check build
# Fast developer loop: warnings, test honesty, lint style, and exact-selector checks.
dev-check: rust-warnings-check std-test-honesty-check terlan-lint-style-profile-check cli-exact-selector-check


# Fast developer loop for VM-focused changes: run-command, diagnostics quality, and runtime inventory.
dev-vm-check: terlan-vm-run-command-check vm-diagnostics-quality-check vm-runtime-concept-inventory-check


# Fast developer loop for web/editor-focused changes: tree-sitter CLI, debugger surface, and TS namespace generation.
dev-web-check: tree-sitter-cli-check editor-debugger-surface-check angular-ts-namespace-generation-check


# Alias for the cli-build target.
build: cli-build

.PHONY: formal-cloud-deploy-plan-check formal-cloud-release-bundle-check formal-cloud-dashboard-toolchain-check
# Runs deploy-manifest Rust tests and the cloud deploy plan v2 shell check.
formal-cloud-deploy-plan-check: cli-build
	$(RUST_TEST) -p terlan --lib commands::deploy::deploy_test -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_parses_semantic_deployment_intent_without_values -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_rejects_secret_values_and_undeclared_secret_names -- --nocapture
	$(RUST_TEST) -p terlan --lib project_manifest_rejects_machine_local_deployment_paths_and_urls -- --nocapture
	bash scripts/check_cloud_deploy_plan_v2.sh


# Runs the release-bundle completeness Rust test and the cloud release bundle v1 shell check.
formal-cloud-release-bundle-check: cli-build
	$(RUST_TEST) -p terlan --lib release_bundle_is_complete_portable_and_deterministic -- --nocapture
	bash scripts/check_cloud_release_bundle_v1.sh


# Validates the generated web toolchain bindings and managed toolchain script.
formal-cloud-dashboard-toolchain-check: cli-build
	$(RUST_TEST) -p terlan --lib commands::bind::typescript::tests -- --nocapture
	$(RUST_TEST) -p terlan --lib commands::build::web_toolchain::tests -- --nocapture
	TERLC="$(if $(CARGO_TARGET_DIR),$(abspath $(CARGO_TARGET_DIR)),$(abspath target))/debug/terlc" bash scripts/check_managed_web_toolchain.sh

LOCAL_TERLC ?= $(shell command -v terlc 2>/dev/null || printf '%s/.local/bin/terlc' "$$HOME")
LOCAL_TERLAN_VM ?= $(dir $(LOCAL_TERLC))terlan-vm
LOCAL_TERLAN_NATIVE_WORKER ?= $(dir $(LOCAL_TERLC))terlan-native-worker


# Builds and installs local terlc, terlan-vm, and terlan-native-worker binaries.
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


# Upgrades the local compiler toolchain and validates editor extension updates.
update-terlc: upgrade-local vscode-extension-check editor-extension-install-update-check
	@printf '%s\n' "updated local terlc at $(LOCAL_TERLC)"
	@printf '%s\n' "updated local terlan-vm at $(LOCAL_TERLAN_VM)"
	@printf '%s\n' "updated local terlan-native-worker at $(LOCAL_TERLAN_NATIVE_WORKER)"
	@printf '%s\n' "validated VS Code and Tree-sitter editor package update artifacts"
	@printf '%s\n' "reload VS Code so the Terlan extension uses the updated compiler"


# Runs the EBNF grammar validator's self-test and strict checks.
validate-ebnf: | terlan-ebnf-validator-bootstrap
	TERLAN_EBNF_ROOT="$(CURDIR)" \
	TERLAN_EBNF_COMPILER="$(CURDIR)/$(TERLAN_BOOTSTRAP_COMPILER)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_EBNF_VALIDATOR_IMAGE) --script-eval -- --self-test
	TERLAN_EBNF_ROOT="$(CURDIR)" \
	TERLAN_EBNF_COMPILER="$(CURDIR)/$(TERLAN_BOOTSTRAP_COMPILER)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_EBNF_VALIDATOR_IMAGE) --script-eval -- --strict


# Regenerates the syntax contract artifacts from the compiler.
syntax-contract-artifacts: | terlan-compiler-bootstrap
	TERLAN_SYNTAX_ARTIFACT_ROOT="$(CURDIR)" \
	TERLAN_SYNTAX_ARTIFACT_COMPILER="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls -- --write


# Checks the syntax contract artifacts are up to date.
syntax-contract-artifacts-check: | terlan-compiler-bootstrap
	TERLAN_SYNTAX_ARTIFACT_ROOT="$(CURDIR)" \
	TERLAN_SYNTAX_ARTIFACT_COMPILER="$(CURDIR)/target/debug/terlc" \
		target/debug/terlc run scripts/ci/scripts/SyncSyntaxContractArtifacts.terls -- --check


# Validates workspace version consistency across the repository.
workspace-version-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) workspace-version


# Validates source file extensions with the bounded standalone Terlan contract.
source-extension-check: | terlan-compiler-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_COMPILER) run scripts/self_validation/RepositorySourceExtensions.terls \
			--out-dir target/source-extension-check/build \
			--cache-dir target/source-extension-check/cache

.PHONY: repository-validation-self-test repository-build-release-contract-check release-artifact-set-check
# Runs the repository validation tool's own self-test.
repository-validation-self-test: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) self-test


# Validates the build/release contract and its generated plan report.
repository-build-release-contract-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) build-release-contract
	test -s target/quality/validation-build-plan-report.json


# Validates the single-root project contract.
single-root-contract-check:
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/SingleRootContractTest.terl


# Checks for whitespace errors in the diff.
diff-whitespace-check:
	git diff --check


# Detects dormant/unused runtime code.
dormant-runtime-code-check:
	$(RUST_TEST) -p terlan --lib --features validation-union dormant_runtime_code_test
	$(TERLAN_QUALITY) dormant-runtime-code


# Validates the safe-Rust runtime boundary.
safe-rust-runtime-check:
	target/debug/terlc test scripts/self_validation/SafeRustRuntimeTest.terl


# Validates the test hierarchy/organization conventions.
test-hierarchy-check:
	$(RUST_TEST) -p terlan --lib --features validation-union test_hierarchy_test
	$(TERLAN_QUALITY) test-hierarchy


# Validates the fast-feedback profile used for local dev builds.
dev-fast-feedback-profile-check:
	$(RUST_TEST) -p terlan --lib --features validation-union dev_fast_feedback_profile_test
	$(TERLAN_QUALITY) dev-fast-feedback-profile


# Validates exact test-selector support in the CLI.
cli-exact-selector-check:
	$(RUST_TEST) -p terlan --lib --features validation-union cli_exact_selectors_test
	$(TERLAN_QUALITY) cli-exact-selectors


# Validates the core type system against its specification.
core-typing-spec-check:
	$(RUST_TEST) -p terlan --lib --features validation-union core_typing_spec_test
	$(TERLAN_QUALITY) core-typing-spec


# Validates target-profile inference rules for builds, runs, and REPL.
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
# Validates that build/run/test commands default to the Terlan VM target.
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


# Runs the shared native-helper self-test under memory/time limits.
shared-helper-check: rust-boundary-audit-report terlan-shared-helper-bootstrap
	TERLAN_SHARED_HELPER_ROOT="$(CURDIR)" \
		$(TERLAN_VALIDATION_VM) run $(TERLAN_SHARED_HELPER_IMAGE) --script-eval -- --self-test
	@ulimit -v $(TERLAN_SHARED_HELPER_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_SHARED_HELPER_TIMEOUT_SECONDS)s env \
			TERLAN_SHARED_HELPER_ROOT="$(CURDIR)" \
			$(TERLAN_VALIDATION_VM) run $(TERLAN_SHARED_HELPER_IMAGE) --script-eval


# Validates the installer contract.
installer-contract-check: | terlan-compiler-bootstrap
	target/debug/terlc test scripts/self_validation/installer_contract


# Validates 100% language feature test coverage.
language-feature-coverage-100-check: comprehension-guards-source-check shape-implications-check
	$(RUST_TEST) -p terlan --lib --features validation-union language_feature_full_coverage_test
	$(TERLAN_QUALITY) language-feature-coverage-100
	$(TERLC) test tests/language/LanguageFeatureCoverageTest.terl


# Validates 100% operator test coverage.
operator-coverage-100-check:
	$(RUST_TEST) -p terlan --lib --features validation-union operator_full_coverage_test
	$(TERLAN_QUALITY) operator-coverage-100
	$(TERLC) test tests/operator/OperatorCoverageTest.terl
	$(TERLC) test tests/comparison/ComparisonTest.terl


# Validates core type contracts via the tree-sitter grammar.
core-type-contracts-check: tree-sitter-package-check tree-sitter-cli-check


# Validates type alias shorthand syntax.
type-alias-shorthand-check: tree-sitter-cli-check
	target/debug/terlc test std/core/AtomTest.terl


# Validates compiler purity/effect metadata.
compiler-purity-metadata-check:
	$(TERLC) test \
		std/core/EffectTest.terl \
		tests/language/PurityEffectsTest.terl \
		tests/fixtures/purity_template/PurityTemplateTest.terl


# Validates the LALRPOP grammar contract.
lalrpop-grammar-contract-check:
	target/debug/terlc test scripts/self_validation/LalrpopGrammarContractTest.terl


# Validates parity between the LALRPOP grammar and the hand-written parser.
lalrpop-parser-parity-check: tree-sitter-package-check tree-sitter-cli-check editor-check lalrpop-grammar-contract-check
	$(POLICY_CARGO_TEST) --locked -p terlan --lib compiler::syntax:: -- --test-threads=1

.PHONY: generated-contract-artifacts release-proof-evidence-check
# Umbrella target gathering generated syntax-contract artifacts
generated-contract-artifacts: syntax-contract-artifacts


# Checks the syntax conformance corpus against the EBNF grammar contract
syntax-contract-check: validate-ebnf tree-sitter-package-check tree-sitter-cli-check
	$(TERLC_EXACT_TEST) compiler::syntax::syntax_conformance_test::shared_syntax_conformance_corpus_matches_contract -- --exact
	grep -F 'ImplicationConstraint ::= "=>" StructuralEvidenceShape .' docs/grammar/TERLAN_SYNTAX_SPEC.ebnf
	grep -F 'The implication arrow is accepted only as generic-parameter shorthand' docs/grammar/README.md
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_decl_test::trait_declarations::parses_structural_implication_in_function_generic_parameter -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::formatter::formatter_test::imports_and_docs::formatter_preserves_structural_generic_implication -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_implication_arrow_as_runtime_expression_operator -- --exact


# Placeholder aggregating prerequisites for typed template interpolation
typed-template-interpolation-check: shape-implications-check tree-sitter-package-check tree-sitter-cli-check


# Placeholder target for WASM core-IR lowering checks
wasm-coreir-lowering-check:


# Placeholder target for WASM runtime execution checks
wasm-runtime-exec-check:


# Placeholder target for WASM contract discovery checks
wasm-contract-discovery-check:


# Checks the OXC JS-emission boundary via the quality tool
oxc-boundary-check:
	$(TERLAN_QUALITY) oxc-boundary


# Runs adversarial Rust tests and the TVM platform-matrix release-artifact fuzzing
adversarial-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib adversarial -- --nocapture
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-adversarial


# Measures line coverage of terlc via cargo-llvm-cov against the configured minimum
coverage-check:
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
		echo "coverage-check requires cargo-llvm-cov; install with: cargo install cargo-llvm-cov --locked"; \
		exit 127; \
	}
	$(CARGO) build --quiet -p terlan --bin terlan-vm
	TERLAN_VM_RUNNER=$(COVERAGE_VM_RUNNER) $(CARGO) llvm-cov --quiet -p terlan --bin terlc --ignore-filename-regex '$(COVERAGE_IGNORE_FILENAME_REGEX)' --fail-under-lines $(COVERAGE_MIN)


# Checks Erlang backend classification via Rust tests and the quality tool
erlang-backend-classification-check:
	$(RUST_TEST) -p terlan --lib --features validation-union erlang_backend_classification_test
	$(TERLAN_QUALITY) erlang-backend-classification


# Checks the native VM image artifact format via Rust tests and the quality tool
terlan-vm-artifact-format-check:
	$(RUST_TEST) -p terlan --lib native_image_test
	$(TERLAN_QUALITY) vm-artifact-format


# Verifies tail calls lower to constant-stack loops across native, Cranelift, and JS backends
tail-recursion-lowering-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::tail_position_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::tail_position_source_test::source_case_tail_recursion_executes_one_million_edges_on_a_small_stack -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::cranelift::managed_stack_map_test::managed_tail_parameter_live_across_cranelift_safepoint_emits_precise_stack_map -- --exact
	$(RUST_TEST) -p terlan --lib commands::emit_js::tail_recursion_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::hot_reload_pins_in_flight_generation_until_its_last_lease_drops -- --exact


# Verifies recursive functions carry termination evidence and yield under scheduling
termination-productivity-analysis-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::core_ir::termination::termination_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::value_lifecycle::value_lifecycle_test::recursive_const_functions_require_core_termination_evidence -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_exposes_validated_termination_evidence -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::tail_position_source_test::source_case_tail_recursion_executes_one_million_edges_on_a_small_stack -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler::yielding_c_fun_beam_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_parallel_messages_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_process_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::supervision::supervision_test


# Verifies shadowed binding identities stay distinct across lowering, JS emission, and LSP
binding-shadowing-safety-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::binding_identity_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::syntax::syntax_output::shapes_test::tests::rejects_duplicate_bindings_created_by_shape_expansion -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::binding_identity_source_test::native_lowering_preserves_outer_and_nested_binding_identities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::emit_js::binding_identity_emit_test::javascript_preserves_outer_and_nested_binding_identities -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_exposes_validated_binding_identity_evidence -- --exact
	$(RUST_TEST) -p terlan --features validation-union --lib binding_navigation_test


# Verifies the JSON VM-IR artifact format is fully rejected in favor of native TVM images
no-tvm-json-runtime-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_tvm_json_artifact_rejections


# Verifies the legacy VM-IR interpreter path is fully rejected
no-vmir-interpreter-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --test tvm_transition_rejection -- --exact no_vmir_interpreter_rejections


# Verifies every command rejects the runtime fallback and cleans up its legacy sidecars
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


# Runs the quality gate for typed template render mode, including editor link tooling
typed-template-render-mode-check: vm-live-template-client-protocol-check typed-template-interpolation-check
	node editors/vscode/test/template_links_test.js
	$(TERLAN_QUALITY) typed-template-render-mode


# Tests `terlc run` argument validation and native-image execution defaulting to VM
terlan-vm-run-command-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_defaults_to_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_accepts_vm_and_rejects_erlang_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::validate_run_args_rejects_unsupported_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::build_command_for_run_appends_default_vm_target -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::build_command_for_run_preserves_explicit_target -- --exact
		$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::find_native_image_for_source_ignores_other_native_images -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::find_native_image_for_source_rejects_transitional_artifact -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::run::run_test::run_built_native_image_executes_vm_runner -- --exact


# Tests the REPL command rejects explicit runtime-target selection
terlan-vm-repl-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::repl::repl_aot_test::repl_command_rejects_runtime_selection -- --exact


# Tests `terlc test` argument parsing and execution defaulting to the Terlan VM target
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


# Aggregates the core VM HTTP lane checks (framing, routing, sessions, SSE, channels)
terlan-vm-http-lane-check: \
	vm-tcp-framing-check \
	vm-http-stream-serve-check \
	vm-http-router-middleware-check \
	http-session-actor-check \
	vm-http-sse-check \
	vm-http-live-channel-source-check


# Runs the self-validation inventory test for all-terlan-tests-vm coverage
all-terlan-tests-vm-inventory-check:
	target/debug/terlc test scripts/self_validation/AllTerlanTestsVmInventoryTest.terl


# Aggregates VM-targeted test suites covering run/repl/test commands, shape guards, and stdlib release tests
all-terlan-tests-vm-check: all-terlan-tests-vm-inventory-check terlan-vm-run-command-check terlan-vm-repl-check terlan-vm-test-command-check flexible-shape-guards-check language-feature-coverage-100-check operator-coverage-100-check pattern-matching-support-check stdlib-release-tests-vm-default-check stdlib-release-tests


# Verifies terlc doctor diagnostics for VM-pivot project hazards
terlc-doctor-vm-pivot-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::parse_doctor_args_defaults_to_current_directory -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::parse_doctor_args_rejects_unknown_option -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_accepts_clean_vm_project -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_reports_vm_pivot_hazards -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::doctor::doctor_test::doctor_project_reports_summary_compiler_contract_mismatch -- --exact


# Alias gating on the Erlang backend classification check
no-default-erlang-emission-check: erlang-backend-classification-check


# Aggregates OTP-runtime-absence checks to confirm no default BEAM runtime dependency
no-default-beam-runtime-check: no-implicit-otp-runtime-check terlan-vm-no-otp-runtime-fallback-check


# Verifies the VM has no default Tokio runtime dependency
no-default-tokio-runtime-check:
	$(RUST_TEST) -p terlan --lib --features validation-union no_default_tokio_runtime_test
	$(TERLAN_QUALITY) no-default-tokio-runtime


# Verifies the Rust implementation has no ERTS dependency via the Terlan VM
no-terlan-vm-erts-rust-dependency-check:
	$(RUST_TEST) -p terlan --lib --features validation-union no_terlan_vm_erts_rust_dependency_test
	$(TERLAN_QUALITY) no-terlan-vm-erts-rust-dependency


# Verifies no implicit OTP runtime dependency is introduced
no-implicit-otp-runtime-check:
	$(RUST_TEST) -p terlan --lib --features validation-union no_implicit_otp_runtime_test
	$(TERLAN_QUALITY) no-implicit-otp-runtime


# Verifies the OTP test pipeline inventory is complete and current
otp-test-pipeline-inventory-check:
	$(RUST_TEST) -p terlan --lib --features validation-union otp_test_pipeline_inventory_test
	$(TERLAN_QUALITY) otp-test-pipeline-inventory


# Audits the Erlang suite coverage via the TVM platform matrix after bootstrapping it
terlan-vm-erl-suite-audit-check: | terlan-tvm-platform-matrix-bootstrap
	$(RUST_TEST) -p terlan --lib module_layout_ -- --nocapture
	$(TERLAN_TVM_PLATFORM_MATRIX) erl-suite-audit-self-test
	$(TERLAN_TVM_PLATFORM_MATRIX) erl-suite-audit-check


# Verifies the OTP-to-Terlan stdlib port across BIF, process, coordination, and native modules
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


# Verifies the deprecated callable dot-call syntax is rejected by the parser
callable-syntax-cleanup-check:
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_function_value_dot_call_syntax -- --exact
	$(TERLC_EXACT_TEST) compiler::syntax::parser::parser_expr_test::macros_and_constructors::rejects_parenthesized_function_value_dot_call_syntax -- --exact
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/CallableSyntaxCleanupTest.terl


# Verifies the Terlan lint style profile configuration
terlan-lint-style-profile-check:
	$(RUST_TEST) -p terlan --lib --features validation-union terlan_lint_style_profile_test
	$(TERLAN_QUALITY) terlan-lint-style-profile


# Verifies the JS backend's type emission contract
js-type-emission-contract-check:
	$(RUST_TEST) -p terlan --lib --features validation-union js_type_emission_contract_test
	$(TERLAN_QUALITY) js-type-emission-contract


# Verifies core runtime conformance for vectors, JSON, and native image round-trips
terlan-runtime-conformance-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::vector::vector_test::vector_mutations_update_storage -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::json::json_test::parse_and_stringify_round_trip_json_text -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact


# Verifies release-train build artifacts, syntax profile, and Postgres config handling
terlan-release-train-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::build_test::tests::artifact_test::vm_artifacts::build_command_emits_js_module_and_manifest_for_single_file -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib formal_pipeline::formal_pipeline_test::interface_foundations::compile_syntax_module_with_core_v0_profile_accepts_covered_subset -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native::postgres_test::config_builders_set_pool_limits_and_timeouts -- --exact


# Verifies the OTP reference inventory is complete and current
otp-reference-inventory-check:
	$(RUST_TEST) -p terlan --lib --features validation-union otp_reference_inventory_test
	$(TERLAN_QUALITY) otp-reference-inventory


# Aggregates checks proving the VM has no OTP runtime fallback path
terlan-vm-no-otp-runtime-fallback-check: otp-reference-inventory-check otp-test-pipeline-inventory-check erlang-backend-classification-check terlan-vm-external-repo-boundary-check


# Verifies Hex package target metadata
hex-target-metadata-check:
	$(RUST_TEST) -p terlan --lib --features validation-union hex_target_metadata_test
	$(TERLAN_QUALITY) hex-target-metadata


# Verifies no-std native target feasibility via the dedicated feasibility tool
native-no-std-target-feasibility-check: hex-target-metadata-check | terlan-quality-tools-bootstrap
	target/debug/terlan-native-target-feasibility


# Verifies the device target planner, gated on no-std feasibility
device-target-planner-check: native-no-std-target-feasibility-check
	$(TERLAN_QUALITY) device-target-planner


# Verifies package manifest git-source dependency parsing and validation
terlan-package-git-source-check:
	$(RUST_TEST) -p terlan --lib --features validation-union package_git_source_test
	$(TERLAN_QUALITY) package-git-source
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::project_manifest::project_manifest_test::project_manifest_rejects_git_dependency_without_rev -- --exact


# Verifies the package lockfile contract
terlan-package-lockfile-check:
	$(RUST_TEST) -p terlan --lib --features validation-union package_lockfile_contract_test
	$(TERLAN_QUALITY) package-lockfile-contract


# Milestone gate confirming the registry MR0 public contract checks all pass
milestone-registry-public-contract: registry-protocol-schema-check registry-package-archive-check registry-lockfile-resolver-check
	@echo "Registry MR0 public contract milestone passed"


# Verifies source-map debug info generation
source-map-debug-info-check: package-build-artifact-isolation-check
	$(TERLAN_QUALITY) source-map-debug-info


# Verifies the compiler's incremental build cache
compiler-incremental-cache-check: source-map-debug-info-check
	$(TERLAN_QUALITY) compiler-incremental-cache


# Verifies watch-mode hot reload behavior
watch-mode-hot-reload-check: compiler-incremental-cache-check
	$(TERLAN_QUALITY) watch-mode-hot-reload


# Verifies AOT developer hot reload is atomic and safe on failed edits
aot-developer-hot-reload-check: watch-mode-hot-reload-check
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::developer_reload_is_atomic_compatible_and_failed_edit_safe -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features validation-union quality::aot_developer_hot_reload::aot_developer_hot_reload_test
	$(TERLAN_QUALITY) aot-developer-hot-reload


# Verifies the Terlan VM internal crate boundary
terlan-vm-internal-crate-check:
	$(RUST_TEST) -p terlan --lib --features validation-union terlan_vm_internal_crate_test
	$(TERLAN_QUALITY) terlan-vm-internal-crate


# Checks the VM does not leak implementation details across the external repo boundary
terlan-vm-external-repo-boundary-check:
	$(RUST_TEST) -p terlan --lib --features validation-union terlan_vm_external_repo_boundary_test
	$(TERLAN_QUALITY) terlan-vm-external-repo-boundary

.PHONY: generated-package-contract-check
# Checks the generated C++ package contract report and test fixture
generated-package-contract-check: cpp-binding-generator-check
	test -s target/quality/cpp-binding-generator.gen_report.json
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/GeneratedPackageContractTest.terl
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate generated-package-contract-check --state passed --reason-code none

.PHONY: external-package-execution-matrix-check
# Self-tests and regenerates the external package execution matrix
external-package-execution-matrix-check:
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) self-test
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) generate \
		--profile "$${TERLAN_EXTERNAL_PACKAGE_PROFILE:-baseline}"

.PHONY: static-route-boundary-check
# Checks the static route boundary
static-route-boundary-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/StaticRouteBoundaryTest.terl

.PHONY: html-boundary-check
# Checks the HTML boundary
html-boundary-check:
	$(CURDIR)/target/debug/terlc test scripts/self_validation/HtmlBoundaryTest.terl


# Checks adversarial coverage of the scheduler under memory pressure
achamp-adversarial-coverage-check: vm-memory-heap-pressure-check
	$(RUST_TEST) -p terlan --lib --features validation-union achamp_adversarial_coverage_test
	$(TERLAN_QUALITY) achamp-adversarial-coverage


# Checks that documentation code blocks compile and run against the VM
executable-docs-vm-check: | terlan-repository-validation-bootstrap
	$(RUST_TEST) -p terlan --lib --features validation-union executable_docs_vm_test
	$(TERLAN_QUALITY) executable-docs-vm
	$(EXACT_CARGO_TEST) -p terlan --lib tests::doc_test::readme_hello_world_terlan_block_compiles -- --exact
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
	TERLAN_REPOSITORY_VALIDATION_TERLC="$(CURDIR)/target/debug/terlc" \
		$(TERLAN_REPOSITORY_VALIDATION) readme-hello-world


# Checks documentation code blocks are executable, gated on the VM docs check
docs-codeblock-executable-check: executable-docs-vm-check


# Self-tests runtime release dependencies via the platform matrix
runtime-release-dependency-self-test: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) runtime-release-dependency-self-test


# Checks the changelog's public scope
changelog-public-scope-check:
	target/debug/terlc test scripts/self_validation/ChangelogPublicScopeTest.terl


# Checks internal documentation
internal-docs-check:
	$(RUST_TEST) -p terlan --lib --features validation-union internal_docs_test
	$(TERLAN_QUALITY) internal-docs


# Checks module README files
module-readme-check:
	$(RUST_TEST) -p terlan --lib --features validation-union module_readmes_test
	$(TERLAN_QUALITY) module-readmes


# Checks Rust doc comments
rustdoc-check:
	$(RUST_TEST) -p terlan --lib --features validation-union rustdoc_
	$(TERLAN_QUALITY) rust-docs


# Alias for the full publish pipeline
release: publish

.PHONY: publish publish-local publish-local-check publish-release-input-check publish-workflow-input-check publish-upload browser-release-check
# Acquires the publication lock, validates branch/version, then runs publish-local
publish: release-plan publish-source-preflight
	@set -eu; \
	git_common=$$(git rev-parse --path-format=absolute --git-common-dir); \
	test -d "$$git_common" && test ! -L "$$git_common"; \
	publication_lock="$$git_common/terlan-publication.lock"; \
	test ! -L "$$publication_lock"; \
	test ! -e "$$publication_lock" || test -f "$$publication_lock"; \
	exec 7>>"$$publication_lock"; \
	flock --exclusive --nonblock 7 || { echo 'another publication or commit owns this repository' >&2; exit 1; }; \
	test /dev/fd/7 -ef "$$publication_lock"; \
	test "$$(git branch --show-current)" = main || { echo 'publish requires main' >&2; exit 1; }; \
	test "$(VERSION)" = "$(RELEASE_VERSION)" || { echo 'VERSION must match workspace.package.version' >&2; exit 1; }; \
	bash scripts/publication_process_preflight.sh "$(CURDIR)"; \
	case "$${MAKEFLAGS%% *}" in *n*) $(MAKE) --no-print-directory -n publish-local VERSION="$(VERSION)" PUBLISH_REVISION="$$(git rev-parse HEAD)"; exit ;; esac; \
	test ! -L target && test ! -L target/quality; \
	mkdir -p target/quality; \
	test ! -L target/quality/release-control.lock; \
	test ! -e target/quality/release-control.lock || test -f target/quality/release-control.lock; \
	flock --nonblock target/quality/release-control.lock \
		$(MAKE) --no-print-directory publish-local VERSION="$(VERSION)" PUBLISH_REVISION="$$(git rev-parse HEAD)"


# Runs the browser build check via the JS browser git hook script
browser-release-check:
	CARGO_TARGET_DIR="$(CURDIR)/target" bash scripts/git_hooks/js_browser.sh


# Aliases the top-level clean target to the CLI's clean target
clean: cli-clean

.PHONY: web-service-foundation-contract-check web-service-foundation-runtime-check web-service-foundation-adapter-check web-service-foundation-pruning-check web-service-foundation-check

.PHONY: typed-css-check
# Runs the typed CSS checker script
typed-css-check:
	@scripts/check_typed_css.sh

# Included last: targets below may reference variables defined above (e.g.
# VM_HTTP_BENCHMARK_COMPARABILITY_DEPS, LEAN_PROOF_CLOSEOUT_DEPS,
# RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES), which must be parsed first.
include mk/bootstrap-checks.mk
include mk/language-feature-checks.mk
include mk/std-checks.mk
include mk/web-checks.mk
include mk/rust-quality-checks.mk
include mk/self-host-checks.mk
include mk/lean-proof-checks.mk
include mk/accelerator-checks.mk
include mk/registry-package-checks.mk
include mk/vm-checks.mk
include mk/release-publish.mk


