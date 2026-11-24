# ---- Release and publication pipeline ----

# Runs the release closeout "examples" lane and checks its pass/cleanup report.
release-example-projects-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) examples
	test -s target/quality/release-example-projects-report.json
	@rg -q '"decision": "pass"' target/quality/release-example-projects-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-example-projects-report.json


# Runs the release closeout "upgrade" lane and checks its pass/cleanup report.
release-project-upgrade-matrix-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) upgrade
	test -s target/quality/release-project-upgrade-matrix-report.json
	@rg -q '"decision": "pass"' target/quality/release-project-upgrade-matrix-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-project-upgrade-matrix-report.json


# Runs the release closeout "reference-apps" lane and checks its pass/cleanup report.
release-reference-app-suite-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) reference-apps
	test -s target/quality/release-reference-app-suite-report.json
	@rg -q '"decision": "pass"' target/quality/release-reference-app-suite-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-reference-app-suite-report.json


# Runs the release closeout "diagnostic-catalog" lane and checks its pass/cleanup report.
release-diagnostic-catalog-check: terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) diagnostic-catalog
	test -s target/quality/release-diagnostic-catalog-report.json
	@rg -q '"decision": "pass"' target/quality/release-diagnostic-catalog-report.json
	@rg -q '"cleanup_status": "pass"' target/quality/release-diagnostic-catalog-report.json


# Runs the release closeout "compatibility-baseline" lane and checks its pass report.
release-compatibility-baseline-check: release-diagnostic-catalog-check
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) compatibility-baseline
	test -s target/quality/release-compatibility-baseline-report.json
	@rg -q '"decision": "pass"' target/quality/release-compatibility-baseline-report.json


# Runs the release closeout "release-notes" lane and checks its pass report.
release-notes-accuracy-check: release-compatibility-baseline-check
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) release-notes
	test -s target/quality/release-notes-accuracy-report.json
	@rg -q '"decision": "pass"' target/quality/release-notes-accuracy-report.json


# Runs the release closeout "supply-chain" lane and validates the SBOM/unsafe-inventory reports.
release-supply-chain-provenance-check: rust-cargo-metadata-report terlan-release-closeout-bootstrap
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) supply-chain
	test -s target/quality/release-sbom.cargo-metadata.json
	test -s target/quality/release-unsafe-inventory.json
	test -s target/quality/release-supply-chain-provenance-report.json
	@rg -q '"decision": "pass"' target/quality/release-supply-chain-provenance-report.json
	@rg -q '"classification": "reviewed-subsystem-boundary:path"' target/quality/release-unsafe-inventory.json
	@! rg -q '":crates/' target/quality/release-unsafe-inventory.json


# Runs security-boundary Rust tests and quality checks, then the release closeout "security" lane.
release-security-hardening-check: release-supply-chain-provenance-check
	$(RUST_TEST) -p terlan --lib --features validation-union native_boundary_security_test
	$(RUST_TEST) -p terlan --lib --features validation-union vm_web_security_policy_test
	$(RUST_TEST) -p terlan --lib --features validation-union vm_web_config_secret_boundary_test
	$(RUST_TEST) -p terlan --lib --features validation-union package_capability_contract_test
	$(RUST_TEST) -p terlan --lib --features validation-union package_cache_integrity_test
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


# Runs the support-bundle test, the release closeout "support-bundle" lane, and checks no secrets leak.
release-support-bundle-check: release-security-hardening-check
	$(RUST_TEST) -p terlan --lib commands::support_bundle
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) support-bundle
	test -s target/quality/release-support-bundle-report.json
	test -s target/quality/release-support-bundles/installed-project.json
	@rg -q '"decision": "pass"' target/quality/release-support-bundle-report.json
	@! rg -q 'release-secret-must-not-leak|$(CURDIR)|/tmp/terlan-release-' target/quality/release-support-bundles/installed-project.json


# Runs the release closeout "performance" lane against the stored benchmark baseline.
release-performance-baseline-check: release-support-bundle-check
	test -s benchmarks/release_0_0_7.baseline.json
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) performance-unavailable-metrics-self-test
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) performance
	test -s target/quality/release-performance-baseline-report.json
	@rg -q '"decision": "pass"' target/quality/release-performance-baseline-report.json
	@rg -q '"correctness_assertion": "command-status-zero-and-output-contract"' target/quality/release-performance-baseline-report.json


# Runs the release closeout "adversarial" lane against the adversarial corpus.
release-adversarial-corpus-check: release-performance-baseline-check
	test -s tests/release/ADVERSARIAL_CORPUS.tsv
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) adversarial
	test -s target/quality/release-adversarial-corpus-report.json
	@rg -q '"decision": "pass"' target/quality/release-adversarial-corpus-report.json
	@rg -q '"stale_entries": \[\]' target/quality/release-adversarial-corpus-report.json


# Runs the release closeout "mutation" lane and checks no survivors remain.
release-mutation-check: release-adversarial-corpus-check
	test -s tests/release/MUTATIONS.tsv
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) mutation
	test -s target/quality/release-mutation-check-report.json
	@rg -q '"decision": "pass"' target/quality/release-mutation-check-report.json
	@rg -q '"survived": \[\]' target/quality/release-mutation-check-report.json


# Runs lifecycle-health checks and the release closeout "fault" injection lane.
release-fault-injection-check: release-mutation-check
	$(RUST_TEST) -p terlan --lib --features validation-union vm_web_lifecycle_health_test
	$(TERLAN_QUALITY) vm-web-lifecycle-health
	test -s target/quality/vm-multicore-release-closeout.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json
	TERLAN_RELEASE_CLOSEOUT_ROOT=$(CURDIR) $(TERLAN_RELEASE_CLOSEOUT) fault
	test -s target/quality/release-fault-injection-report.json
	@rg -q '"decision": "pass"' target/quality/release-fault-injection-report.json
	@rg -q '"skip_reasons": \[\]' target/quality/release-fault-injection-report.json

TERLAN_READINESS_RUN = $(TERLAN_RELEASE_PROMOTION) readiness --version $(RELEASE_VERSION)
TERLAN_OWNED_READINESS = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-readiness "$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_RELEASE_PROMOTION_IMAGE)" "$(RELEASE_VERSION)"


# Aggregates the closeout lanes into a readiness attestation and checks publication is not required.
release-readiness-attestation-check: release-example-projects-check release-project-upgrade-matrix-check release-reference-app-suite-check release-notes-accuracy-check release-fault-injection-check terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_READINESS_RUN)
	test -s target/quality/release-readiness-attestation-report.json
	@rg -q '"decision": "pass"' target/quality/release-readiness-attestation-report.json
	@rg -q '"publication_required": false' target/quality/release-readiness-attestation-report.json


# Focused invalidation owner: reseal unchanged artifacts and already-passing
# reports without replaying the gates that produced them.
release-readiness-attestation-refresh:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_RELEASE_PROMOTION_IMAGE)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_READINESS_RUN)
	test -s target/quality/release-readiness-attestation-report.json
	@rg -q '"decision": "pass"' target/quality/release-readiness-attestation-report.json
	@rg -q '"publication_required": false' target/quality/release-readiness-attestation-report.json

TERLAN_STAGED_DISTRIBUTION_RUN = $(TERLAN_TVM_PLATFORM_MATRIX) release-staged-distribution
TERLAN_OWNED_STAGED_DISTRIBUTION = $(TERLAN_PREPARATION_OWNER) $(TERLAN_RELEASE_PROMOTION) prepare-staged-distribution "$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"


# Verifies the release and runs the staged-distribution check, requiring no source checkout.
release-staged-distribution-verification-check: release-readiness-attestation-check | terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) verify --version $(RELEASE_VERSION)
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_STAGED_DISTRIBUTION_RUN)
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
		$(TERLAN_STAGED_DISTRIBUTION_RUN)
	test -s target/quality/release-staged-distribution-verification-report.json
	@rg -q '"decision": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"failed_upgrade_rollback": "pass"' target/quality/release-staged-distribution-verification-report.json
	@rg -q '"source_checkout_required": false' target/quality/release-staged-distribution-verification-report.json

# Publication's warm repair uses the same checkpoint as its cold hosted graph.
# Standalone developer verification keeps the normal direct producer.

.PHONY: publish-staged-distribution-prepare release-preparation-staged-distribution-check

publish-staged-distribution-prepare: TERLAN_STAGED_DISTRIBUTION_RUN = $(TERLAN_OWNED_STAGED_DISTRIBUTION)

publish-staged-distribution-prepare: TERLAN_READINESS_RUN = $(TERLAN_OWNED_READINESS)
# Wires the staged-distribution refresh through the publication-owned producers.
publish-staged-distribution-prepare: release-staged-distribution-verification-refresh


# Self-tests the staged-distribution preparation path with a timeout.
release-preparation-staged-distribution-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 1800s $(TERLAN_RELEASE_PROMOTION) preparation-staged-distribution-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"

.PHONY: release-preparation-readiness-check
# Self-tests the readiness preparation path with a timeout.
release-preparation-readiness-check: | terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-readiness-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_RELEASE_PROMOTION_IMAGE)"


# Checks release version/channel metadata for the dev channel.
release-version-metadata-check: | terlan-compiler-bootstrap
	TERLAN_RELEASE_VERSION="$(VERSION)" \
	TERLAN_RELEASE_CHANNEL=dev \
		target/debug/terlc test scripts/self_validation/ReleaseVersionChannelTest.terl


# Alias for the release version/channel metadata check.
release-version-channel-check: release-version-metadata-check


# Bumps the release version by writing the new version/channel metadata.
release-version-bump: | terlan-compiler-bootstrap
	TERLAN_RELEASE_VERSION="$(VERSION)" \
	TERLAN_RELEASE_VERSION_WRITE=1 \
	TERLAN_RELEASE_CHANNEL=dev \
		target/debug/terlc test scripts/self_validation/ReleaseVersionChannelTest.terl


# Validates the release boundary contract.
release-boundary-check: repository-build-release-contract-check
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) release-boundary

RELEASE_ARTIFACT_SET_ROOT ?= target/release-distribution
RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD ?= 0
# One hosted workspace and Make graph own the final compiler/validator bootstrap.
# All underlying checks remain independently callable for local diagnosis.

.PHONY: release-hosted-multicore-evidence-check release-hosted-publication-verifier release-hosted-validation-check
# Aggregates multicore memory-model and release-contract evidence for hosted release.
release-hosted-multicore-evidence-check: vm-multicore-memory-model-check vm-multicore-release-contract-check
	$(MAKE) --no-print-directory vm-multicore-release-record


# Publishes the TVM AOT platform matrix verifier image and its checksums.
release-hosted-publication-verifier: | terlan-tvm-platform-matrix-bootstrap
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE).inputs.sha256
	install -m 0644 $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE) \
		target/quality/tvm-aot-platform-matrix-verifier.tvm
	install -m 0644 $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE).inputs.sha256 \
		target/quality/tvm-aot-platform-matrix-verifier.tvm.inputs.sha256
	cd target/quality && sha256sum \
		tvm-aot-platform-matrix-verifier.tvm \
		tvm-aot-platform-matrix-verifier.tvm.inputs.sha256 \
		> tvm-aot-platform-matrix-verifier.sha256


# Aggregates the hosted release validation checks (platform, artifacts, multicore, closeout).
release-hosted-validation-check: tvm-aot-platform-aggregate-check release-artifact-set-check release-hosted-multicore-evidence-check tvm-aot-release-closeout-contract-check release-hosted-publication-verifier


# Validates the release artifact set, optionally including local payload.
release-artifact-set-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) release-artifact-set \
		"$(RELEASE_ARTIFACT_SET_ROOT)" \
		$(if $(filter 1,$(RELEASE_ARTIFACT_SET_LOCAL_PAYLOAD)),--with-local-payload,)


# Aggregates release code-hygiene checks (warnings, quality, lint, dormant code).
release-code-hygiene-check: \
	rust-warnings-check \
	rust-quality-check \
	rust-file-headroom-check \
	dormant-runtime-code-check \
	vm-deterministic-hashmap-check \
	shared-helper-check \
	terlan-lint-style-profile-check
	$(TERLAN_QUALITY) release-code-hygiene


# Direct proof execution is the release closeout; no second evidence composer
# may re-hash or reseal already validated theorem outcomes.
release-artifacts-closeout-check: lean-proof-lanes-check

.PHONY: release-preparation-proof-check release-preparation-proof-input-check release-preparation-proof-track-check release-preparation-proof-policy-check release-preparation-proof-kernels-check
release-preparation-proof-input-check \
release-preparation-proof-track-check \
release-preparation-proof-policy-check \
release-preparation-proof-kernels-check \
release-preparation-proof-check: proof-repro-check

.PHONY: release-preparation-proof-smoke-check

.PHONY: release-preparation-proof-native-boundary-check
# Validates preparation-proof native-boundary self-test via the release promotion tool
release-preparation-proof-native-boundary-check: | terlan-release-promotion-bootstrap
	timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-proof-native-boundary-self-test


# Validates preparation-proof smoke self-test via the release promotion tool
release-preparation-proof-smoke-check: | terlan-release-promotion-bootstrap
	timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-proof-smoke-self-test

.PHONY: release-preparation-proof-lanes-check
# Validates preparation-proof lanes self-test via the release promotion tool
release-preparation-proof-lanes-check: | terlan-release-promotion-bootstrap
	timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-proof-lanes-self-test

.PHONY: release-preparation-proof-binding-check
# Validates preparation-proof binding self-test via the release promotion tool
release-preparation-proof-binding-check: | terlan-release-promotion-bootstrap
	timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-proof-binding-self-test


# Adds the binding check as an extra prerequisite of the preparation-proof check
release-preparation-proof-check: release-preparation-proof-binding-check


# Exports a flag so the recursive check run also gathers release evidence

release-evidence-refresh: export TERLAN_CHECK_RELEASE_EVIDENCE := 1
# Reruns the full check suite with release-evidence collection enabled
release-evidence-refresh: check


# Verifies the release-preflight report shows a passing candidate-bound decision
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


# Refreshes release evidence then runs the release preflight verification
release-check: release-evidence-refresh
	$(MAKE) --no-print-directory release-preflight RELEASE_VERSION="$(RELEASE_VERSION)"


# Validates one candidate without running the destructive multi-lane artifact
# benchmark. Artifact measurement is diagnostic work owned by the explicit
# build-artifact-budget-record target; release correctness uses the fast policy
# self-test in the canonical check graph and validates packaged artifacts later.
release-candidate-check: editor-release-contract-check terlan-compiler-bootstrap
	bash scripts/clean_build_outputs.sh --check-partials
	$(MAKE) --no-print-directory terlan-quality-tools-bootstrap
	$(MAKE) --no-print-directory \
		TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		TERLAN_VALIDATION_VM_PREBUILT=1 \
		release-proof-evidence-check
	$(MAKE) --no-print-directory \
		TERLAN_BUILD_ARTIFACTS_PREBUILT=1 \
		TERLAN_VALIDATION_VM_PREBUILT=1 \
		check
	bash scripts/clean_build_outputs.sh --check-partials


# Checks proof lane freshness and semantic evidence before expensive builds
release-proof-evidence-check: proof-repro-check lean-proof-track-gap-hygiene-check


# Aggregates adversarial and coverage checks for release hardening
release-hardening-check: adversarial-check coverage-check


# Verifies release pipeline flake detection
release-flake-detection-check: watch-mode-hot-reload-check
	$(TERLAN_QUALITY) release-flake-detection


# Verifies release gate shard resume behavior
release-gate-shard-resume-check: release-flake-detection-check
	$(TERLAN_QUALITY) release-gate-shard-resume


# Verifies release gate duration budget enforcement
release-gate-duration-budget-check: release-gate-shard-resume-check
	$(TERLAN_QUALITY) release-gate-duration-budget


# Verifies the release gate report schema
release-gate-report-schema-check: release-gate-duration-budget-check
	$(TERLAN_QUALITY) release-gate-report-schema


# Records the pre-release snapshot of generated artifacts before freshness gates run
release-generated-artifacts-before: | editor-node-tools-check terlan-tvm-platform-matrix-bootstrap
	@test "$(RELEASE_GENERATED_ARTIFACT_SNAPSHOT_REQUIRED)" = 1 || { \
		echo 'generated-artifact validation requires its public Make target or TERLAN_CHECK_RELEASE_EVIDENCE=1 before shared producers run' >&2; \
		exit 1; \
	}
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-self-test
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-record


# Gates the freshness pass on the before-snapshot and all freshness checks
release-generated-artifacts-freshness-pass: release-generated-artifacts-before $(RELEASE_GENERATED_ARTIFACT_FRESHNESS_GATES)


# Finalizes the release generated-artifacts check and cleans up the snapshot file
release-generated-artifacts-check: release-generated-artifacts-freshness-pass
	$(TERLAN_TVM_PLATFORM_MATRIX) release-generated-artifacts-finalize
	rm -f target/quality/release-generated-artifacts-before.json


# Seals the current release artifact after boundary/version/source checks pass
release-artifact-current: terlan-release-promotion-bootstrap release-boundary-check release-version-metadata-check source-extension-check vm-release-artifact-matrix-check
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) seal $(if $(VERSION),--version "$(VERSION)",)



release-artifact-linux: export TERLAN_RELEASE_OS = Linux

release-artifact-linux: export TERLAN_RELEASE_ARCH = x86_64
# Builds the Linux x86_64 release artifact via the current release pipeline
release-artifact-linux: release-artifact-current


# Smoke-tests the release artifact via the platform matrix
release-artifact-smoke: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-smoke


# Smoke-tests the release artifact installer via the platform matrix
release-artifact-installer-smoke: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-installer-smoke

.PHONY: publish-source-preflight publish-prepare publish-preflight publish publish-release-from-dist release-preparation-recovery-check publish-cache-prune

# Explicit Linux acceptance rehearsal, separate from ordinary promotion self-tests.
# The private fixture owns its Cargo runs; publication never invokes this target.

.PHONY: release-preparation-process-containment-check
# Checks the release-preparation process containment fixture (Cargo-only, private)
release-preparation-process-containment-check:
	$(CARGO) test --locked -p terlan-process-owner

.PHONY: release-preparation-candidate-acceptance-check
# Checks release-preparation candidate acceptance across cold/warm/resume/retry
release-preparation-candidate-acceptance-check:
	$(CARGO) test --locked -p terlan-test-orchestrator --test publish_prepare_make \
		full_candidate_rehearsal_covers_cold_warm_resume_and_upload_retry -- --exact --nocapture


# Checks release-preparation recovery via the preparation-candidate self-test
release-preparation-recovery-check: release-preparation-process-containment-check release-preparation-candidate-acceptance-check | terlan-release-promotion-bootstrap
	timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-candidate-self-test

# One explicit local closeout entry point for the complete preparation-owner
# graph. Make shares common bootstrap prerequisites within this invocation while
# each rehearsal retains its own deterministic fault-injection scenarios.

.PHONY: release-preparation-owner-graph-check
# Runs the complete release-preparation owner rehearsal graph
release-preparation-owner-graph-check: \
	release-preparation-recovery-check \
	release-preparation-contract-check \
	release-preparation-aot-check \
	release-preparation-release-reports-check \
	release-preparation-local-reports-check \
	release-preparation-multicore-check \
	release-preparation-readiness-check \
	release-preparation-staged-distribution-check \
	release-preparation-proof-check \
	release-preparation-proof-smoke-check \
	release-preparation-proof-native-boundary-check \
	release-preparation-proof-lanes-check
	@echo '[release-preparation-owner-graph] all owner rehearsals passed'

.PHONY: release-preparation-multicore-check
# Checks release preparation across the multicore VM/platform matrix bootstrap
release-preparation-multicore-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-multicore-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"

.PHONY: release-preparation-local-reports-check
# Checks release-preparation local report generation
release-preparation-local-reports-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-local-reports-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"

.PHONY: release-preparation-aot-check

.PHONY: release-preparation-release-reports-check
# Checks release-preparation release-report generation
release-preparation-release-reports-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-release-reports-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"


# Checks release-preparation ahead-of-time backend self-test
release-preparation-aot-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-aot-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"

.PHONY: publish-preparation-lock-directory
# Creates the target/quality directory used to lock publish preparation state
publish-preparation-lock-directory:
	mkdir -p target/quality

.PHONY: release-preparation-contract-check
# Checks release-preparation contract self-test
release-preparation-contract-check: | terlan-release-promotion-bootstrap terlan-tvm-platform-matrix-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" timeout 900s $(TERLAN_RELEASE_PROMOTION) preparation-contract-self-test \
		"$(TERLAN_BOOTSTRAP_VM)" "$(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)"

# Explicit Linux acceptance tier: compile actual Terlan proof callers, then reuse
# one tiny completed Rust suite through the production authenticated request bridge.

.PHONY: release-preparation-aot-coverage-check
# Verifies AOT release-refresh coverage on Linux x86_64 via one targeted cargo test
release-preparation-aot-coverage-check: | terlan-compiler-bootstrap
	@test "$$(uname -s)/$$(uname -m)" = Linux/x86_64 || { echo 'AOT coverage acceptance requires Linux x86_64' >&2; exit 1; }
	TERLAN_COVERAGE_AOT_TOOLS="$(CURDIR)/target/debug" \
		timeout 1200s $(CARGO) test -p terlan-test-orchestrator --test make_coverage \
		release_refresh_keeps_shared_gate_nodes_inside_one_live_make_graph -- --exact --nocapture


# Fails publication prep unless the working tree is clean
publish-source-preflight:
	@echo "Preparing Terlan $(VERSION) publication preflight"
	@if source_changes=$$(git status --porcelain); then :; else \
		echo 'cannot determine source checkout state; publication preflight stopped' >&2; \
		exit 1; \
	fi; \
	if [ -n "$$source_changes" ]; then \
		changed_count=$$(printf '%s\n' "$$source_changes" | wc -l | tr -d ' '); \
		echo "publish-preflight failed: working tree is not clean"; \
		echo "changed files: $$changed_count"; \
		echo "first changed files:"; \
		printf '%s\n' "$$source_changes" | sed -n '1,20p'; \
		if [ "$$changed_count" -gt 20 ]; then \
			echo "... $$((changed_count - 20)) more changed files omitted"; \
		fi; \
		echo "next step: review and commit the release contents, then rerun make publish"; \
		exit 1; \
	fi

# Network/tag checks and GitHub authentication are publication-only. Keeping
# them out of publish-source-preflight lets local preparation run without gh or
# a network while publish-preflight still enforces the complete promotion policy.

.PHONY: publish-remote-preflight
# Verifies GitHub CLI auth, branch/ancestry, and remote tag state before publishing
publish-remote-preflight: publish-source-preflight
	@command -v gh >/dev/null 2>&1 || { \
		echo "publish requires GitHub CLI: install gh and run gh auth login"; \
		exit 127; \
	}
	@gh auth status >/dev/null 2>&1 || { \
		echo "publish requires authenticated GitHub CLI: run gh auth login"; \
		exit 1; \
	}
	@branch=$$(git branch --show-current); \
	if [ "$(TERLAN_TAG_PUBLICATION)" != 1 ] && [ "$$branch" != "main" ]; then \
		echo "publication must run from main; current branch is $$branch"; \
		exit 1; \
	fi
	@git fetch --quiet origin main || exit $$?; \
	if [ "$(TERLAN_TAG_PUBLICATION)" = 1 ]; then \
		git merge-base --is-ancestor HEAD origin/main || { echo 'release tag must belong to main history' >&2; exit 1; }; \
	elif ! git merge-base --is-ancestor origin/main HEAD; then \
		echo "origin/main is not an ancestor of HEAD; publication would require a non-fast-forward push"; \
		exit 1; \
	fi
	@tag_ref="refs/tags/v$(VERSION)"; \
	if remote_tags=$$(git ls-remote --tags origin "$$tag_ref" "$$tag_ref^{}"); then :; else \
		echo 'cannot determine remote release tag state; retry after restoring Git access' >&2; \
		exit 1; \
	fi; \
	remote_tag_object_sha=$$(printf '%s\n' "$$remote_tags" | awk -v ref="$$tag_ref" '$$2 == ref { print $$1 }'); \
	remote_tag_sha=$$(printf '%s\n' "$$remote_tags" | awk -v ref="$$tag_ref^{}" '$$2 == ref { print $$1 }'); \
	if [ "$(TERLAN_TAG_PUBLICATION)" = 1 ] && [ -z "$$remote_tags" ]; then \
		echo 'tag publication requires an existing remote annotated tag' >&2; exit 1; \
	fi; \
	if [ -n "$$remote_tags" ] && { [ -z "$$remote_tag_object_sha" ] || [ -z "$$remote_tag_sha" ]; }; then \
		echo "remote tag v$(VERSION) must be annotated with a resolvable commit" >&2; \
		exit 1; \
	fi; \
	if git show-ref --verify --quiet "$$tag_ref"; then \
		local_tag_present=1; \
	else \
		local_tag_status=$$?; \
		test "$$local_tag_status" -eq 1 || exit "$$local_tag_status"; \
		local_tag_present=0; \
	fi; \
	if [ "$$local_tag_present" -eq 0 ] && [ -n "$$remote_tag_object_sha" ]; then \
		git fetch --quiet origin "$$tag_ref:$$tag_ref" || exit $$?; \
		local_tag_present=1; \
	fi; \
	head_sha=$$(git rev-parse HEAD) || exit $$?; \
	if [ "$$local_tag_present" -eq 1 ]; then \
		tag_type=$$(git cat-file -t "$$tag_ref") || exit $$?; \
		if [ "$$tag_type" != tag ]; then \
			echo "local tag v$(VERSION) must be annotated; found $$tag_type"; \
			exit 1; \
		fi; \
		tag_sha=$$(git rev-parse "$$tag_ref^{commit}") || exit $$?; \
		if [ "$$tag_sha" != "$$head_sha" ]; then \
			echo "local tag v$(VERSION) already exists at $$tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "local tag v$(VERSION) already exists at HEAD; continuing"; \
	fi; \
	if [ -n "$$remote_tag_object_sha" ]; then \
		local_tag_object_sha=$$(git rev-parse "$$tag_ref") || exit $$?; \
		if [ "$$remote_tag_object_sha" != "$$local_tag_object_sha" ]; then \
			echo "local and remote annotated tag objects differ for v$(VERSION)"; \
			exit 1; \
		fi; \
		if [ "$$remote_tag_sha" != "$$head_sha" ]; then \
			echo "remote tag v$(VERSION) already exists at $$remote_tag_sha, not HEAD $$head_sha"; \
			exit 1; \
		fi; \
		echo "remote tag v$(VERSION) already exists at HEAD; release upload can be retried"; \
	fi


# Preparation may build and refresh evidence. Publication below never does.
# Keep missing host tools from being discovered after an expensive refresh.
# A successful full refresh already owns readiness and staged installation.
# Only the existing-evidence path needs their focused repair. This local
# decision comes from completed commands, never a caller's skip flag.
publish-prepare: publish-source-preflight
	bash scripts/publication_process_preflight.sh "$(CURDIR)"
	@for tool in cargo rustc node npm jq lake clang flock sha256sum realpath timeout; do \
		command -v "$$tool" >/dev/null 2>&1 || { \
			echo "publication preparation requires $$tool; install it before preparing the candidate" >&2; \
			exit 127; \
		}; \
	done
	@if [ -n "$${JAVA_HOME:-}" ]; then \
		test -x "$$JAVA_HOME/bin/java" || { echo 'JAVA_HOME must point to a usable JDK' >&2; exit 127; }; \
	else \
		command -v java >/dev/null 2>&1 || { echo 'publication preparation requires Java; set JAVA_HOME or add java to PATH' >&2; exit 127; }; \
	fi
	@set -eu; \
	channel=$$(sed -nE 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' rust-toolchain.toml | head -n 1); \
	test -n "$$channel" || { echo 'rust-toolchain.toml has no pinned channel' >&2; exit 1; }; \
	rustc_path=$$(command -v rustc); cargo_path=$$(command -v cargo); \
	test -x "$$rustc_path" && test -x "$$cargo_path" || { echo 'Rust compiler tools are not executable' >&2; exit 127; }; \
	rustc_version=$$(rustc --version | awk '{print $$2}'); cargo_version=$$(cargo --version | awk '{print $$2}'); \
	test "$$rustc_version" = "$$channel" || { echo "rustc $$rustc_version does not match pinned $$channel" >&2; exit 1; }; \
	test "$$cargo_version" = "$$channel" || { echo "cargo $$cargo_version does not match pinned $$channel" >&2; exit 1; }
	@set -eu; \
	# Recursive Make still runs under -n; only traverse, never acquire or write. \
	case "$${MAKEFLAGS%% *}" in \
		*n*) $(MAKE) --no-print-directory publish-prepare-locked VERSION="$(VERSION)"; exit $$? ;; \
	esac; \
	test ! -L target && test ! -L target/quality || exit 1; \
	mkdir -p target/quality; \
	# Distribution restoration shares this lease with every evidence consumer. \
	test ! -L target && test ! -L target/quality && test ! -L target/quality/preparation.lock || exit 1; \
	if test -e target/quality/preparation.lock; then test -f target/quality/preparation.lock; fi; \
	exec 9>target/quality/preparation.lock; \
	timeout 120s flock -w 120 9; \
	test /proc/self/fd/9 -ef target/quality/preparation.lock; \
	export TERLAN_PREPARATION_LOCK_HELD=1; \
	$(MAKE) --no-print-directory publish-prepare-locked VERSION="$(VERSION)"; \
	flock -u 9; \
	exec 9>&-

# Enter only from the enclosing owner. Separate recipes let Make dry-runs show
# the real download/probe graph without executing these external operations.

.PHONY: publish-prepare-locked
# Downloads verified artifacts and drives warm/cold evidence preparation under the held lock
publish-prepare-locked:
	@test "$${TERLAN_PREPARATION_LOCK_HELD:-}" = 1 && \
		test -d target && test ! -L target && \
		test -d target/quality && test ! -L target/quality && \
		test -f target/quality/preparation.lock && test ! -L target/quality/preparation.lock && \
		test /proc/self/fd/9 -ef target/quality/preparation.lock && \
		timeout 120s flock -w 120 9
	@case "$(TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS)" in \
		''|*[!0-9]*) echo 'TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS must be an integer' >&2; exit 2 ;; \
	esac; \
	test "$(TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS)" -ge 3600 || { \
		echo 'TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS must be at least 3600' >&2; \
		exit 2; \
	}
	timeout --kill-after=10s "$(TERLAN_PUBLICATION_INPUT_TIMEOUT_SECONDS)s" \
		bash scripts/download_validated_release_artifacts.sh "$$(git rev-parse HEAD)"
	@if ! timeout 30s dist/terlc --version; then \
		echo 'publication requires a host that can execute the verified Linux x86_64 artifact (Ubuntu 24.04-compatible userspace); use a compatible container if needed' >&2; \
		exit 1; \
	fi
	$(MAKE) --no-print-directory publish-evidence-plan-check
	$(MAKE) --no-print-directory publish-evidence-refresh-plan-check
	@set -eu; \
	if test "$(TERLAN_TAG_PUBLICATION)" = 1; then \
		$(MAKE) --no-print-directory publish-evidence-check || { \
			echo '[publish] validated tag evidence is absent or stale; refusing to regenerate immutable publication inputs' >&2; \
			exit 1; \
		}; \
		$(MAKE) --no-print-directory publish-preparation-warm VERSION="$(VERSION)"; \
	elif $(MAKE) --no-print-directory publish-evidence-check; then \
		$(MAKE) --no-print-directory publish-preparation-warm VERSION="$(VERSION)"; \
	else \
		echo '[publish] candidate evidence is absent or stale; refreshing its owners once'; \
		$(MAKE) --no-print-directory publish-preparation-cold VERSION="$(VERSION)"; \
	fi; \
	$(MAKE) release-preflight RELEASE_VERSION="$(VERSION)"

# One Make process owns each preparation branch and shares all bootstrap nodes.
# These routes do not assert cached success or introduce a prebuilt skip flag.

.PHONY: publish-preparation-cold publish-preparation-warm publish-preparation-checks publish-preparation-verified
# Runs the shared metadata, boundary, extension, and bootstrap checks common to prep
publish-preparation-checks: release-version-metadata-check release-boundary-check source-extension-check terlan-release-promotion-bootstrap publish-preparation-admission


# Refreshes evidence, then confirms it passes the evidence check
publish-preparation-verified: publish-evidence-refresh
	$(MAKE) --no-print-directory publish-evidence-check


# Full cold-path preparation: checks, verified evidence refresh, then cache prune


# Prunes the publication cache under an exclusive lock via the release-promotion tool
publish-cache-prune: | terlan-release-promotion-bootstrap
	flock --exclusive --nonblock target/publication-inputs.lock \
		timeout 120s $(TERLAN_RELEASE_PROMOTION) prune-publication-cache "$$(git rev-parse HEAD)"

# One shared resource admission is the first node of every publication
# preparation branch. Make deduplicates it within a graph; the flock also
# serializes concurrent invocations before they inspect or retire caches.

.PHONY: publish-preparation-admission
# Admits one shared lock and Rust build resources before any preparation branch runs
publish-preparation-admission: publish-preparation-lock-directory rust-build-resource-admission

# This native bootstrap must remain usable when there is insufficient space to
# rebuild the compiler. Audit and prune consume the prebuilt tool without Cargo
# replay; the workspace-support tier owns its tests. This is Linux-only tooling.


# Verifies a prepared release candidate exists and passes the promotion preflight
publish-preflight: publish-remote-preflight
	@test -x $(TERLAN_BOOTSTRAP_VM) \
		&& test -s $(TERLAN_RELEASE_PROMOTION_IMAGE) \
		&& test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE) \
		&& test -s dist/release-candidate.json || { \
			echo 'no prepared release candidate; run make publish-prepare once, then retry make publish' >&2; \
			exit 1; \
		}
	$(MAKE) --no-print-directory publish-evidence-check
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		timeout 300s $(TERLAN_RELEASE_PROMOTION) preflight --version "$(VERSION)"


# Runs the multicore and AOT publish evidence checks
publish-evidence-check:
	$(MAKE) --no-print-directory vm-multicore-publish-check
	$(MAKE) --no-print-directory tvm-aot-publish-evidence-check


# Asserts the evidence-check dry-run plan replays no build or test work
publish-evidence-plan-check:
	@set -eu; \
	plan=$$(mktemp); \
	trap 'rm -f "$$plan"' EXIT; \
	env -u MAKEFLAGS -u MAKEOVERRIDES -u MFLAGS \
		-u TERLAN_RUST_COVERAGE_CONTEXT \
		-u TERLAN_RUST_COVERAGE_SCOPE \
		-u TERLAN_VALIDATION_BOOTSTRAPPED \
		-u TERLAN_BUILD_ARTIFACTS_PREBUILT \
		-u TERLAN_RELEASE_BINARIES_PREBUILT \
		$(MAKE) --no-print-directory -n publish-evidence-check >"$$plan"; \
	if rg -n '(^|[[:space:]])cargo([[:space:]]|$$)|run_exact_cargo_test|terlc (test|build)' "$$plan"; then \
		echo 'error[publish.evidence.plan]: verification replays build or test work' >&2; \
		exit 1; \
	fi; \
	echo '[publish-evidence-plan] zero build and test replays'


# Asserts the evidence-refresh dry-run plan stays within its Cargo invocation budget
publish-evidence-refresh-plan-check:
	@set -eu; \
	plan=$$(mktemp); \
	trap 'rm -f "$$plan"' EXIT; \
	env -u MAKEFLAGS -u MAKEOVERRIDES -u MFLAGS \
		-u TERLAN_RUST_COVERAGE_CONTEXT \
		-u TERLAN_RUST_COVERAGE_SCOPE \
		-u TERLAN_VALIDATION_BOOTSTRAPPED \
		-u TERLAN_BUILD_ARTIFACTS_PREBUILT \
		-u TERLAN_RELEASE_BINARIES_PREBUILT \
		$(MAKE) --no-print-directory -Bn publish-evidence-refresh >"$$plan"; \
	cargo_pattern='(^|[[:space:]])cargo --locked (build|test|check|clippy)([[:space:]]|$$)'; \
	cargo_count=$$(rg -c "$$cargo_pattern" "$$plan" || true); \
	exact_count=$$(rg -c 'run_exact_cargo_test' "$$plan" || true); \
	checkpoint_count=$$(rg -c ' prepare-release-reports([[:space:]]|$$)' "$$plan" || true); \
	if test "$${checkpoint_count:-0}" -ne 1; then \
		echo 'error[publish.evidence.refresh_plan]: expected exactly one release-report graph owner' >&2; \
		exit 1; \
	fi; \
	exact_count=$$(($${exact_count:-0} + $${checkpoint_count:-0})); \
	duplicate_cargo=$$(rg "$$cargo_pattern" "$$plan" | sort | uniq -d || true); \
	if test "$$cargo_count" -gt 6; then \
		echo "error[publish.evidence.refresh_plan]: $$cargo_count Cargo invocations exceed the six-invocation budget" >&2; \
		exit 1; \
	fi; \
	if test "$$exact_count" -gt 5; then \
		echo "error[publish.evidence.refresh_plan]: $$exact_count Cargo selectors exceed the managed-list, two release-AOT, serial-parser, and runtime-report budget" >&2; \
		exit 1; \
	fi; \
	if test -n "$$duplicate_cargo"; then \
		echo 'error[publish.evidence.refresh_plan]: duplicate equivalent Cargo build:' >&2; \
		echo "$$duplicate_cargo" >&2; \
		exit 1; \
	fi; \
	if rg -n 'scripts_TvmAotPlatformMatrix\.tvm.*-- (self-test|tsan-self-test|release-self-test|multicore-release-self-test)([[:space:]]|$$)' "$$plan"; then \
		echo 'error[publish.evidence.refresh_plan]: contract self-test bypasses its preparation owner' >&2; \
		exit 1; \
	fi; \
	echo "[publish-evidence-refresh-plan] cargo=$$cargo_count exact-isolated=$$exact_count duplicate-builds=0"

TERLAN_RELEASE_CONTROL_IMAGE := target/release-control/vm/scripts_Release.tvm
TERLAN_RELEASE_CONTROL := $(CURDIR)/$(TERLAN_BOOTSTRAP_VM) run $(CURDIR)/$(TERLAN_RELEASE_CONTROL_IMAGE) --script-eval --
TERLAN_RELEASE_CONTROL_SOURCE := $(TERLAN_BOOTSTRAP_COMPILER) run scripts/release_control/scripts/Release.terls --


# Reports current release-control status after ensuring the control VM is built
release-status: | terlan-release-control-bootstrap
	$(TERLAN_RELEASE_CONTROL) status "$(VERSION)"


# Prints the release status/plan using the current local compiler
release-plan: publish-source-preflight
	@test -x $(TERLAN_BOOTSTRAP_COMPILER) || { echo 'release planning requires the current local terlc' >&2; exit 1; }
	$(TERLAN_RELEASE_CONTROL_SOURCE) status "$(VERSION)"


# Predicts the release outcome after validating evidence plans are clean
release-predict: publish-source-preflight publish-evidence-plan-check publish-evidence-refresh-plan-check
	@test -x $(TERLAN_BOOTSTRAP_COMPILER) || { echo 'release prediction requires the current local terlc' >&2; exit 1; }
	$(TERLAN_RELEASE_CONTROL_SOURCE) predict "$(VERSION)"
	@echo '[release-predict] hosted checks, retained artifacts, and local execution graph are ready'


# Waits on the release-control wait command under an exclusive lock
release-wait: publish-source-preflight
	$(MAKE) --no-print-directory terlan-release-control-bootstrap
	@test ! -L target && test ! -L target/quality && test ! -L target/quality/release-control.lock
	@mkdir -p target/quality
	flock --nonblock target/quality/release-control.lock \
		$(TERLAN_RELEASE_CONTROL) wait "$(VERSION)"


# Publishes the release tag via release-control under an exclusive lock
release-tag: publish-source-preflight
	$(MAKE) --no-print-directory terlan-release-control-bootstrap
	@test ! -L target && test ! -L target/quality && test ! -L target/quality/release-control.lock
	@mkdir -p target/quality
	flock --nonblock target/quality/release-control.lock \
		$(TERLAN_RELEASE_CONTROL) publish-tag "$(VERSION)"



publish-tag: export TERLAN_TAG_PUBLICATION=1
# Runs the publish preflight, then publishes the release tag from the prepared dist
publish-tag: publish-preflight
	timeout --kill-after=10s 900s bash scripts/publish_release_from_dist.sh "$(VERSION)" </dev/null


# Runs the release-control self-test and acceptance test
release-control-check: | terlan-release-control-bootstrap
	$(TERLAN_RELEASE_CONTROL) self-test
	$(TERLAN_RELEASE_CONTROL) acceptance-test "$(CURDIR)/$(TERLAN_BOOTSTRAP_VM)" "$(CURDIR)/$(TERLAN_RELEASE_CONTROL_IMAGE)"


# Validates the revision, then waits for and submits the release via release-control
publish-local:
	@test -n "$(PUBLISH_REVISION)" && test "$$(git rev-parse HEAD)" = "$(PUBLISH_REVISION)"
	$(MAKE) --no-print-directory publish-local-check
	$(MAKE) --no-print-directory terlan-release-control-bootstrap
	$(TERLAN_RELEASE_CONTROL) wait "$(VERSION)"
	$(TERLAN_RELEASE_CONTROL) submit "$(VERSION)" "$(PUBLISH_REVISION)"


# Requires full coverage on release payload and build-graph input contracts
publish-release-input-check: | terlan-compiler-bootstrap
	$(TERLAN_BOOTSTRAP_COMPILER) test \
		scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleasePayloadInputTest.terl \
		scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleaseBuildGraphTest.terl \
		--coverage --coverage-threshold 100


# Requires full coverage on the workflow contract test
publish-workflow-input-check: | terlan-compiler-bootstrap
	$(TERLAN_BOOTSTRAP_COMPILER) test \
		scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/WorkflowContractTest.terl \
		--coverage --coverage-threshold 100


# Runs CI script tests, preflight tests, security audits, and workflow linting
publish-local-check: publish-release-input-check publish-workflow-input-check rust-security-audit-check node-security-audit-check
	$(TERLAN_BOOTSTRAP_COMPILER) test scripts/ci --coverage --coverage-threshold 100
	bash scripts/publication_process_preflight_test.sh
	scripts/actionlint.sh -color=false .github/workflows/*.yml


# Uploads and promotes the prepared release from dist
publish-upload:
	timeout --kill-after=10s 1800s bash scripts/publish_release_from_dist.sh "$(VERSION)" --promote </dev/null


# Publishes the release artifacts from dist without promoting
publish-release-from-dist:
	timeout --kill-after=10s 900s bash scripts/publish_release_from_dist.sh "$(VERSION)" </dev/null


# Runs the release-promotion self-test and contract checks
release-promotion-pipeline-check: terlan-release-promotion-bootstrap release-preparation-contract-check
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) self-test --report
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) contract


# Dry-runs the release-promotion tool for the given version
release-promotion-dry-run: terlan-release-promotion-bootstrap
	TERLAN_RELEASE_ROOT="$(CURDIR)" \
		$(TERLAN_RELEASE_PROMOTION) dry-run $(if $(VERSION),--version "$(VERSION)",)

