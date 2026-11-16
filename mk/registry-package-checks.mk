# ---- Package registry and resolver checks ----

# Verifies package resolution is reproducible across targets, lockfiles, and git sources
package-resolver-reproducibility-check: device-target-planner-check terlan-package-lockfile-check terlan-package-git-source-check
	$(TERLAN_QUALITY) package-resolver-reproducibility


# Verifies the package registry publish workflow
package-registry-publish-check: package-resolver-reproducibility-check
	$(TERLAN_QUALITY) package-registry-publish


# Verifies the registry protocol schema v1 contract
registry-protocol-schema-check: rust-test-suite
	bash scripts/check_registry_protocol_schema_v1.sh


# Verifies the registry package archive format v1
registry-package-archive-check: rust-test-suite
	bash scripts/check_registry_package_archive_v1.sh


# Verifies the registry lockfile resolver v1 behavior
registry-lockfile-resolver-check: rust-test-suite
	bash scripts/check_registry_lockfile_resolver_v1.sh


# Verifies registry CLI integration after bootstrapping the compiler
registry-cli-integration-check: | terlan-compiler-bootstrap
	bash scripts/check_registry_cli_integration_v1.sh

.PHONY: registry-http-publication-check registry-trusted-resolution-check registry-graph-workflow-check registry-live-frontend-check
# Verifies registry HTTP publication against the sibling terlan-registry repo
registry-http-publication-check: | terlan-compiler-bootstrap
	bash ../terlan-registry/scripts/check-registry-http-publication.sh


# Confirms trusted live/conditional/offline resolution passes
registry-trusted-resolution-check: registry-http-publication-check
	@echo "Registry trusted live/conditional/offline resolution passed"


# Confirms the trusted whole-dependency-graph workflow passes
registry-graph-workflow-check: registry-trusted-resolution-check
	@echo "Registry trusted whole-graph workflow passed"


# Verifies the DB-backed Terlan Angular.ts registry frontend
registry-live-frontend-check: registry-graph-workflow-check
	@$(MAKE) -C ../terlan-registry registry-frontend-check TERLC="$(abspath $(TERLAN_BOOTSTRAP_COMPILER))"
	@echo "Registry DB-backed Terlan Angular.ts frontend passed"


# Verifies the registry against an adversarial package corpus
registry-adversarial-corpus-check: rust-test-suite
	bash scripts/check_registry_cli_integration_v1.sh
	@echo "Registry adversarial package corpus passed"


# Verifies the package capability contract
package-capability-contract-check: package-registry-publish-check
	$(TERLAN_QUALITY) package-capability-contract


# Verifies the package release test matrix
package-release-test-matrix-check: package-capability-contract-check
	$(TERLAN_QUALITY) package-release-test-matrix


# Verifies package API compatibility
package-api-compatibility-check: package-release-test-matrix-check
	$(TERLAN_QUALITY) package-api-compatibility


# Verifies the package CLI workflow
package-cli-workflow-check: package-api-compatibility-check
	$(TERLAN_QUALITY) package-cli-workflow


# Verifies package editor integration
package-editor-integration-check: package-cli-workflow-check
	$(TERLAN_QUALITY) package-editor-integration


# Verifies package cache integrity
package-cache-integrity-check: package-editor-integration-check
	$(TERLAN_QUALITY) package-cache-integrity


# Verifies the package workspace dependency graph
package-workspace-graph-check: package-cache-integrity-check
	$(TERLAN_QUALITY) package-workspace-graph


# Verifies package build artifact isolation
package-build-artifact-isolation-check: package-workspace-graph-check
	$(TERLAN_QUALITY) package-build-artifact-isolation


# Verifies package test execution via the external package matrix self-test and baseline generation
package-test-exec-check:
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) self-test
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) generate \
		--profile baseline --allow-missing

# Adds the external package matrix bootstrap as an order-only prerequisite for these checks
package-test-exec-check cpp-binding-generator-check generated-package-contract-check cuda-package-availability-check cuda-package-check external-package-execution-matrix-check: | terlan-external-package-matrix-bootstrap


