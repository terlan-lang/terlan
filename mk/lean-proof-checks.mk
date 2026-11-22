# ---- Formal (Lean) proof checks ----
# Proof truth comes from executing the current theorem sources against the
# current candidate. No stored digest, snapshot, checksum, or composite
# evidence document participates in admission.
.PHONY: lean-proof-lanes-check lean-proof-feature-binding-contract-check lalrpop-grammar-contract-check

lean-proof-track-check: proof-repro-check lean-proof-track-gap-hygiene-check

lean-proof-parser-shape-check: lalrpop-grammar-contract-check
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=parser_shape/ParserShape.lean \
		$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/LeanProofExecutionTest.terl

lean-proof-native-boundary-check: native-boundary-security-check proof-repro-check
lean-proof-smoke-check: proof-repro-check

lean-proof-lanes-check: proof-repro-check lean-proof-track-pr-gate lean-proof-track-regression-check lean-proof-track-gap-hygiene-check

lean-proof-feature-binding-contract-check: lean-proof-lanes-check
lean-proof-feature-binding-check: lean-proof-feature-binding-contract-check
lean-proof-change-impact-report: lean-proof-feature-binding-check
lean-proof-feature-binding-review: lean-proof-change-impact-report
lean-proof-snapshot-consistency-check: lean-proof-feature-binding-review

lean-proof-counterexample-check:
	$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/LeanProofCounterexampleTest.terl

lean-proof-feature-cull-check:
	$(TERLAN_QUALITY) lean-proof-feature-cull
	TERLAN_LEAN_PROOF_ROOT="$(CURDIR)" \
	TERLAN_LEAN_PROOF_PATH=Terlan/FeatureCull/LegacyBoundaries.lean \
		$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/LeanProofExecutionTest.terl

TERLAN_PROOF_TRACK_RUN = $(TERLAN_QUALITY) lean-proof-track
TERLAN_PROOF_RUNTIME_RUN = $(TERLAN_QUALITY) lean-proof-runtime
TERLAN_PROOF_PR_RUN = $(TERLAN_QUALITY) lean-proof-pr
TERLAN_PROOF_REGRESSION_RUN = $(TERLAN_QUALITY) lean-proof-regression

proof-repro-check: lean-proof-track-runtime-check
	$(TERLAN_PROOF_TRACK_RUN)

proof_repro_check: proof-repro-check

lean-proof-track-pr-gate: proof-repro-check
	$(TERLAN_PROOF_PR_RUN)

lean-proof-track-regression-check: lean-proof-track-pr-gate
	$(TERLAN_PROOF_REGRESSION_RUN)

# Family target names remain stable for callers, but Make shares the one direct
# proof execution node instead of recompiling and resealing identical evidence.
lean-proof-templates-routes-check \
lean-proof-concurrency-check \
lean-proof-collections-check \
lean-proof-wasm-bridge-check \
lean-proof-db-sql-check \
lean-proof-std-package-check \
lean-proof-semantic-kernels-check: proof-repro-check

lean-proof-track-runtime-check: lean-proof-feature-cull-check
	$(TERLAN_PROOF_RUNTIME_RUN)

proof-coverage-release-artifacts-smoke: release-artifacts-closeout-check
proof-readiness-release-mode-check: release-artifacts-closeout-check

lean-proof-track-gap-hygiene-check:
	$(TERLAN_QUALITY) lean-proof-gap-hygiene

lean-proof-track-release-closeout-check: proof-repro-check \
	lean-proof-parser-shape-check \
	lean-proof-native-boundary-check \
	lean-proof-counterexample-check \
	lean-proof-lanes-check

RELEASE_EVIDENCE_GATES := \
	vm-http-benchmark-comparability-check \
	vm-multicore-replay-observability-check \
	release-failure-reproduction-check \
	internal-docs-check \
	release-generated-artifacts-check \
	release-version-channel-check \
	lean-proof-track-release-closeout-check \
	release-artifacts-closeout-check \
	proof-readiness-release-mode-check \
	release-staged-distribution-verification-check
