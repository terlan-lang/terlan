# Rust structural quality, feedback-loop, and recurring measurement ownership.

.PHONY: rust-format-check rust-locked-binary-check rust-clippy-check
.PHONY: rust-lint-allowance-check rust-workspace-policy-check
.PHONY: rust-code-quality-adversarial-check rust-code-quality-preflight-check
.PHONY: rust-api-boundary-quality-check rust-api-boundary-quality-record
.PHONY: rust-api-boundary-quality-record-budgets rust-warnings-check rust-quality-check
.PHONY: rust-boundary-audit-report
.PHONY: rust-structure-census-check rust-structure-census-record-timings
.PHONY: rust-build-graph-timings-self-test rust-build-graph-timings-record
.PHONY: rust-structure-census-record-closeout-timings
.PHONY: rust-structure-closeout-timings-check
.PHONY: rust-structure-periodic-timings-check rust-dependency-impact-check rust-dependency-impact-record

# Dependency impact performs the broadest structural analysis and retains a
# somewhat larger bounded VM allowance than the narrower quality gates.
TERLAN_RUST_DEPENDENCY_IMPACT_VIRTUAL_MEMORY_KIB := 196608
TERLAN_RUST_BUILD_GRAPH_VIRTUAL_MEMORY_KIB := 196608
.PHONY: rust-module-structure-check rust-build-graph-boundary-check rust-cargo-metadata-report
.PHONY: rust-structure-timings-self-test rust-module-structure-self-test
.PHONY: rust-canonical-type-ownership-check
.PHONY: rust-file-headroom-check
.PHONY: rust-artifact-retention-check
.PHONY: rust-code-quality-0-0-9-milestone-check
.PHONY: code-quality-0-0-7-feedback-loop-check

rust-format-check:
	$(CARGO) fmt --all -- --check

rust-locked-binary-check: rust-clippy-check
	@echo "[rust-locked-binary] clippy compiled every workspace binary in both feature profiles"

rust-clippy-check:
	$(CARGO) clippy --workspace --bins -- -D warnings
	$(CARGO) clippy --workspace --bins --all-features -- -D warnings

rust-lint-allowance-check: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) lint-allowance-self-test
	@ulimit -v $(TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) lint-allowance-check

rust-workspace-policy-check: rust-lint-allowance-check
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) workspace-policy-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) workspace-policy-manifest-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) workspace-policy-member-self-test
	@ulimit -v $(TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) workspace-policy-check

rust-build-graph-timings-self-test: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) build-graph-timings-self-test

rust-build-graph-timings-record: rust-build-graph-timings-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) build-graph-timings-record

rust-code-quality-adversarial-check:
	target/debug/terlc test scripts/self_validation/RustCodeQualityAdversarialTest.terl
	$(RUST_TEST) -p terlan --lib --features quality-tools rustdoc_rejects_
	$(RUST_TEST) -p terlan --lib --features quality-tools rust_quality_rejects_

rust-code-quality-preflight-check: \
	rust-workspace-policy-check \
	rust-code-quality-adversarial-check \
	rust-module-structure-check \
	rust-file-headroom-check \
	rust-format-check \
	rust-locked-binary-check \
	rust-clippy-check \
	rustdoc-check \
	rust-quality-check
	@echo "[rust-code-quality] preflight passed"

rust-boundary-audit-report: | terlan-quality-tools-bootstrap
	@mkdir -p target/quality
	target/debug/terlan-rust-boundary-audit . \
		--api-boundary-input target/quality/rust-api-boundary-input.tsv \
		--shared-helper-input target/quality/rust-shared-helper-input.tsv \
		--structural-input target/quality/rust-structural-input.json \
		> target/quality/rust-boundary-ast.json

rust-cargo-metadata-report:
	@mkdir -p target/quality
	$(CARGO) metadata --format-version 1 > target/quality/rust-cargo-metadata.json

rust-api-boundary-quality-check: \
	rust-lint-allowance-check \
	rust-boundary-audit-report \
	rust-clippy-check \
	terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) api-boundary-self-test
	@ulimit -v $(TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) api-boundary-check

rust-api-boundary-quality-record: rust-boundary-audit-report terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) api-boundary-record

rust-api-boundary-quality-record-budgets: rust-boundary-audit-report terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) api-boundary-record-budgets

rust-warnings-check: rust-clippy-check
	@echo "[rust-warnings] workspace binary warnings are denied by the canonical clippy owner"

rust-quality-check: dormant-runtime-code-check vm-deterministic-hashmap-check
	$(TERLAN_QUALITY) rust-quality

rust-structure-timings-self-test: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timings-self-test

rust-module-structure-self-test: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) module-structure-self-test

rust-structure-census-check: \
	rust-structure-timings-self-test \
	rust-file-headroom-check \
	rust-module-structure-check \
	rust-api-boundary-quality-check \
	rust-canonical-type-ownership-check
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timings-check
	@echo "[rust-structure-census] typed modular structure gates passed"

rust-structure-census-record-timings: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timings-record

rust-structure-census-record-closeout-timings: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timings-record-closeout

rust-structure-closeout-timings-check: rust-structure-timings-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timings-closeout-check

rust-structure-periodic-timings-check: rust-structure-timings-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timing-regression-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) structure-timing-regression-measure

rust-dependency-impact-check: rust-boundary-audit-report rust-cargo-metadata-report terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) dependency-impact-self-test
	@ulimit -v $(TERLAN_RUST_DEPENDENCY_IMPACT_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) dependency-impact-check

rust-dependency-impact-record: rust-boundary-audit-report rust-cargo-metadata-report terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) dependency-impact-record

rust-module-structure-check: rust-module-structure-self-test
	@ulimit -v $(TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) module-structure-check

rust-file-headroom-check: terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) file-headroom-self-test
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) file-headroom-check

.PHONY: rust-artifact-retention-clean-shared-debug
rust-artifact-retention-check:
	TERLAN_CARGO_ARTIFACT_RETENTION_MODE=self-test \
		target/debug/terlc test scripts/self_validation/CargoArtifactRetentionTest.terl
	TERLAN_CARGO_ARTIFACT_RETENTION_MODE=clean-check \
	TERLAN_CARGO_ARTIFACT_RETENTION_TARGET="$(CURDIR)/target" \
	TERLAN_CARGO_ARTIFACT_RETENTION_REPORT="$(CURDIR)/target/quality/cargo-artifact-retention.json" \
		target/debug/terlc test scripts/self_validation/CargoArtifactRetentionTest.terl

rust-artifact-retention-clean-shared-debug:
	TERLAN_CARGO_ARTIFACT_RETENTION_MODE=clean-shared-debug \
	TERLAN_CARGO_ARTIFACT_RETENTION_TARGET="$(CURDIR)/target" \
		target/debug/terlc test scripts/self_validation/CargoArtifactRetentionTest.terl

rust-code-quality-0-0-9-milestone-check: \
	rust-boundary-audit-report \
	rust-dependency-impact-check \
	rust-file-headroom-check \
	terlan-rust-quality-bootstrap
	@ulimit -v $(TERLAN_RUST_QUALITY_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) api-boundary-milestone-0.0.9
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) file-headroom-milestone-0.0.9
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) dependency-impact-milestone-0.0.9

rust-build-graph-boundary-check: \
	rust-dependency-impact-check \
	rust-workspace-policy-check \
	shared-helper-check \
	rust-boundary-audit-report \
	terlan-rust-quality-bootstrap
	TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
		$(TERLAN_RUST_QUALITY) build-graph-boundary-self-test
	@ulimit -v $(TERLAN_RUST_BUILD_GRAPH_VIRTUAL_MEMORY_KIB); \
		timeout $(TERLAN_RUST_QUALITY_TIMEOUT_SECONDS)s env \
			MALLOC_ARENA_MAX=$(TERLAN_RUST_QUALITY_MALLOC_ARENA_MAX) \
			TERLAN_RUST_QUALITY_ROOT="$(CURDIR)" \
			$(TERLAN_RUST_QUALITY) build-graph-boundary-check

.PHONY: build-artifact-budget-record build-artifact-budget-self-test build-artifact-budget-check
# Measurement must precede every test-owning/package gate because it performs a
# clean build and then `cargo clean`. Package isolation is still enforced by the
# consumer below, after the canonical suite has established shared evidence.
build-artifact-budget-record: terlan-artifact-measurement-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SELF_VALIDATION_IMAGE) -- --measure-if-stale

build-artifact-budget-self-test: terlan-artifact-measurement-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SELF_VALIDATION_IMAGE) -- --self-test

build-artifact-budget-check: rust-build-graph-boundary-check package-build-artifact-isolation-check terlan-self-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SELF_VALIDATION_IMAGE)
	TERLAN_CARGO_ARTIFACT_RETENTION_MODE=clean-check \
	TERLAN_CARGO_ARTIFACT_RETENTION_TARGET="$(CURDIR)/target" \
	TERLAN_CARGO_ARTIFACT_RETENTION_REPORT="$(CURDIR)/target/quality/cargo-artifact-retention.json" \
		target/debug/terlc test scripts/self_validation/CargoArtifactRetentionTest.terl

rust-canonical-type-ownership-check: rust-build-graph-boundary-check
	@echo "[rust-canonical-type-ownership] canonical AST ownership passed"

code-quality-0-0-7-feedback-loop-check: rust-module-structure-self-test
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::syntax::parser::parser_adversarial_test::tests::adversarial_module_rejects_ambiguous_constructor_and_struct_shapes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::typeck::adversarial_test::adversarial_typecheck_rejects_generic_arity_mismatch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::application_admission_test::closed_application_passes_admission_and_graph_validation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::actor_dynamic_module::actor_dynamic_module_test::actor_dynamic_module_lifecycle_is_vm_owned_and_exit_driven -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features editor-lsp lsp::binding_navigation::binding_navigation_test::navigation_index_keeps_nested_same_spelled_bindings_separate -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib --features quality-tools quality::lib_test::rustdoc_passes_documented_items -- --exact
