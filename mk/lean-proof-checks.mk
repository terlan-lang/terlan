# ---- Formal (Lean) proof-lane checks ----
.PHONY: lean-proof-lanes-check lean-proof-feature-binding-contract-check lalrpop-grammar-contract-check
# Shared producers stay in one Make graph. Dependencies, rather than recursive
# serial invocations, preserve runtime -> replay -> smoke -> lane seal ordering.
lean-proof-track-check: lean-proof-lanes-check lean-proof-feature-binding-contract-check


# Validates the parser shape Lean proof.
lean-proof-parser-shape-check: lalrpop-grammar-contract-check
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=parser_shape/ParserShape.lean \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl


# Validates the native boundary Lean proof.
lean-proof-native-boundary-check: native-boundary-security-check
	$(TERLAN_PROOF_NATIVE_BOUNDARY_RUN)


# Runs the Lean proof smoke test.
lean-proof-smoke-check: proof-repro-check lean-proof-native-boundary-check
	$(TERLAN_PROOF_SMOKE_RUN)


# Runs the full set of Lean proof lanes.
lean-proof-lanes-check: lean-proof-smoke-check lean-proof-track-pr-gate lean-proof-track-regression-check lean-proof-semantic-kernels-check
	$(TERLAN_PROOF_LANES_RUN)

TERLAN_PROOF_BINDING_RUN = target/debug/terlc test --incremental scripts/self_validation/LeanProofFeatureBindingTest.terl
TERLAN_PROOF_SNAPSHOT_RUN = target/debug/terlc test --incremental scripts/self_validation/LeanProofSnapshotTest.terl
TERLAN_PROOF_SNAPSHOT_SELECTED_RUN = $(TERLAN_PROOF_SNAPSHOT_RUN) --name selected_snapshot_task_holds
TERLAN_OWNED_PROOF_BINDING = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-binding "target/debug/terlc"


# Validates the Lean proof feature-binding contract.
lean-proof-feature-binding-contract-check: lean-proof-lanes-check
	$(TERLAN_PROOF_BINDING_RUN)


# Diffs the Lean proof feature-binding snapshot.
lean-proof-feature-binding-check: lean-proof-feature-binding-contract-check
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=diff \
		$(TERLAN_PROOF_SNAPSHOT_RUN)


# Reports the change impact on Lean proof feature bindings.
lean-proof-change-impact-report: lean-proof-feature-binding-check
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=impact \
		$(TERLAN_PROOF_SNAPSHOT_SELECTED_RUN)


# Reviews pending Lean proof feature-binding changes.
lean-proof-feature-binding-review: lean-proof-change-impact-report
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=review \
		$(TERLAN_PROOF_SNAPSHOT_SELECTED_RUN)


# Validates Lean proof feature-binding snapshot consistency.
lean-proof-snapshot-consistency-check: lean-proof-feature-binding-review
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_SNAPSHOT_TASK=snapshot \
		$(TERLAN_PROOF_SNAPSHOT_SELECTED_RUN)


# Validates Lean proof counterexample handling.
lean-proof-counterexample-check:
	target/debug/terlc test scripts/self_validation/LeanProofCounterexampleTest.terl


# Validates culled/legacy Lean proof feature boundaries.
lean-proof-feature-cull-check:
	$(TERLAN_QUALITY) lean-proof-feature-cull
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=Terlan/FeatureCull/LegacyBoundaries.lean \
		target/debug/terlc test scripts/self_validation/LeanProofExecutionTest.terl

TERLAN_PROOF_TRACK_RUN = $(TERLAN_QUALITY) lean-proof-track
TERLAN_OWNED_PROOF_TRACK = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-kernels "target/debug/terlan-quality" "$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_SEMANTIC_KERNEL_IMAGE)"
TERLAN_PROOF_SMOKE_RUN = target/debug/terlc test --incremental scripts/self_validation/LeanProofSmokeTest.terl
TERLAN_PROOF_NATIVE_BOUNDARY_RUN = target/debug/terlc test --incremental scripts/self_validation/LeanProofNativeBoundaryTest.terl
TERLAN_OWNED_PROOF_NATIVE_BOUNDARY = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-native-boundary "target/debug/terlc"
TERLAN_OWNED_PROOF_SMOKE = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-smoke "target/debug/terlc"
TERLAN_PROOF_LANES_RUN = target/debug/terlc test --incremental scripts/self_validation/LeanProofLanesTest.terl
TERLAN_OWNED_PROOF_LANES = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-lanes "target/debug/terlc"
TERLAN_PROOF_RUNTIME_RUN = $(TERLAN_QUALITY) lean-proof-runtime
TERLAN_PROOF_PR_RUN = $(TERLAN_QUALITY) lean-proof-pr
TERLAN_PROOF_REGRESSION_RUN = $(TERLAN_QUALITY) lean-proof-regression
TERLAN_OWNED_PROOF_POLICIES = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-policies "target/debug/terlan-quality"


# Validates Lean proof reproducibility tracking.
proof-repro-check: lean-proof-track-runtime-check
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_track
	$(TERLAN_PROOF_TRACK_RUN)


# Underscore alias for proof-repro-check.
proof_repro_check: proof-repro-check


# Runs the Lean proof PR gate check.
lean-proof-track-pr-gate:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_pr_test

	$(TERLAN_PROOF_PR_RUN)
# Runs the Lean semantic-kernel checks for the templates/routes family.
lean-proof-templates-routes-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family templates-routes


# Runs the Lean semantic-kernel checks for the concurrency family.
lean-proof-concurrency-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family concurrency


# Runs the Lean semantic-kernel checks for the collections family.
lean-proof-collections-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family collections


# Runs the Lean semantic-kernel checks for the wasm-bridge family.
lean-proof-wasm-bridge-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family wasm-bridge


# Runs the Lean semantic-kernel checks for the database-sql family.
lean-proof-db-sql-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family database-sql


# Runs the Lean semantic-kernel checks for the std-packages family.
lean-proof-std-package-check:
	TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check-family std-packages

define TERLAN_SEMANTIC_KERNEL_RUN
TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" $(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- self-test
TERLAN_SEMANTIC_KERNEL_ROOT="$(CURDIR)" $(TERLAN_BOOTSTRAP_VM) run $(TERLAN_SEMANTIC_KERNEL_IMAGE) --script-eval -- check
endef


# Runs the Lean semantic-kernel self-test and full check.
lean-proof-semantic-kernels-check: lean-proof-feature-cull-check
	$(TERLAN_SEMANTIC_KERNEL_RUN)


# Validates the Lean proof runtime track.
lean-proof-track-runtime-check: lean-proof-feature-cull-check
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_runtime_test
	$(TERLAN_PROOF_RUNTIME_RUN)


# Validates the Lean proof regression track.
lean-proof-track-regression-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_regression_test
	$(TERLAN_PROOF_REGRESSION_RUN)


# The lane prerequisite includes the ordered runtime/replay/smoke producers.
# Sharing that node avoids resealing the same report in a recursive Make graph.
TERLAN_PROOF_RELEASE_RUN = $(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- closeout
TERLAN_OWNED_PROOF_RELEASE = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-proof-release "$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_PROOF_RELEASE_IMAGE)"


# Prints a summary of the Lean proof release closeout artifacts.
proof-coverage-release-artifacts-smoke: release-artifacts-closeout-check
	@TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) --script-eval -- summary


# Closeout constructs and validates one document, runs its adversarial tests,
# and produces the release-mode report. Do not replay the composer here.
proof-readiness-release-mode-check: release-artifacts-closeout-check


# Checks the lean-proof track for gaps via the Rust test and quality tool
lean-proof-track-gap-hygiene-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools lean_proof_gap_hygiene
	$(TERLAN_QUALITY) lean-proof-gap-hygiene


# Runs the lean-proof track release closeout after all proof lanes pass
lean-proof-track-release-closeout-check: rust-test-suite lalrpop-parser-parity-check lean-proof-parser-shape-check lean-proof-native-boundary-check lean-proof-counterexample-check lean-proof-smoke-check lean-proof-snapshot-consistency-check $(LEAN_PROOF_CLOSEOUT_DEPS) lean-proof-track-gap-hygiene-check
	target/debug/terlan-lean-proof-closeout


RELEASE_EVIDENCE_GATES := \
	vm-http-benchmark-comparability-check \
	vm-multicore-replay-observability-check \
	release-failure-reproduction-check \
	internal-docs-check \
	release-generated-artifacts-check \
	release-version-channel-check \
	lean-proof-snapshot-consistency-check \
	lean-proof-track-release-closeout-check \
	release-artifacts-closeout-check \
	proof-readiness-release-mode-check \
	release-staged-distribution-verification-check

# One live Make graph owns correctness and release evidence prerequisites,
# allowing Make to share common nodes instead of replaying them recursively.


# Records a fresh proof-release baseline via the bootstrap VM
proof-release-baseline: lean-proof-lanes-check | terlan-proof-release-bootstrap
	TERLAN_PROOF_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_BOOTSTRAP_VM) run $(TERLAN_PROOF_RELEASE_IMAGE) \
			--script-eval -- record-baseline


