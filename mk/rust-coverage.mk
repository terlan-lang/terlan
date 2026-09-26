# Only a live, input-bound parent can answer Rust gate coverage requests.
# The old routing variable is accepted only to fail closed through that client;
# it is never sufficient proof that any test passed.
TERLAN_RUST_ORCHESTRATOR ?= $(CURDIR)/target/validation-tools/terlan-test-orchestrator
ifneq ($(strip $(TERLAN_RUST_COVERAGE_CONTEXT)),)
RUST_TEST := $(TERLAN_RUST_ORCHESTRATOR) --coverage-request
EXACT_CARGO_TEST := $(RUST_TEST)
TERLAN_VALIDATION_BOOTSTRAPPED := 1
TERLAN_BUILD_ARTIFACTS_PREBUILT := 1
TERLC := $(CURDIR)/target/debug/terlc
TERLAN_QUALITY := $(CURDIR)/target/debug/terlan-quality
endif
