CARGO := cargo
PYTHON := python3 -B
SHELL := bash
.SHELLFLAGS := -eo pipefail -c

.PHONY: check test test-release build release-artifact-current release-artifact-linux release-artifact-smoke release-artifact-installer-smoke publish-preflight publish validate-ebnf workspace-version-check release-version-metadata-check source-extension-check release-boundary-check single-root-contract-check diff-whitespace-check rust-warnings-check rust-quality-check test-hierarchy-check cli-exact-selector-check shared-helper-check installer-contract-check oxc-boundary-check adversarial-check coverage-check release-hardening-check erlang-modernization-inventory-check erlang-modernization-em0-hard-gate erlang-modernization-em0-full-compatibility-gate release-0-0-6-preflight erlang-runtime-matrix-check erlang-runtime-matrix-release-check terlan-vm-artifact-format-check native-binding-generator-contract-check no-default-tokio-runtime-check vm-process-model-check vm-scheduler-contract-check vm-actor-primitives-check vm-failure-primitives-check vm-supervision-primitives-check vm-timer-primitives-check vm-resource-ownership-check vm-table-primitives-check vm-code-server-check vm-distribution-envelope-check terlan-package-git-source-check terlan-package-lockfile-check native-boundary-postgres-baseline-benchmark native-boundary-http-baseline-benchmark terlan-vm-compiler-bridge-check http-runtime-stack-check runtime-release-dependency-self-test changelog-public-scope-check internal-docs-check module-readme-check rustdoc-check clean

include crates/terlan/cli.mk
include std/stdlib.mk
include editors/editor.mk

COVERAGE_MIN ?= 82.40
COVERAGE_IGNORE_FILENAME_REGEX ?= crates/terlan/src/(lsp|vm)/\.\./

ifneq ($(filter publish publish-preflight,$(MAKECMDGOALS)),)
ifndef VERSION
$(error VERSION is required. Use: make $(firstword $(MAKECMDGOALS)) VERSION=<release-version>)
endif
ifneq ($(filter v%,$(VERSION)),)
$(error VERSION must not include the leading v. Use: make $(firstword $(MAKECMDGOALS)) VERSION=$(patsubst v%,%,$(VERSION)))
endif
endif

check:
	$(MAKE) release-boundary-check
	$(MAKE) single-root-contract-check
	$(MAKE) diff-whitespace-check
	$(MAKE) workspace-version-check
	$(MAKE) release-version-metadata-check
	$(MAKE) source-extension-check
	$(MAKE) rust-warnings-check
	$(MAKE) rust-quality-check
	$(MAKE) test-hierarchy-check
	$(MAKE) cli-exact-selector-check
	$(MAKE) shared-helper-check
	$(MAKE) installer-contract-check
	$(MAKE) oxc-boundary-check
	$(MAKE) terlan-vm-artifact-format-check
	$(MAKE) terlc-doctor-vm-pivot-check
	$(MAKE) native-binding-generator-contract-check
	$(MAKE) no-default-tokio-runtime-check
	$(MAKE) vm-process-model-check
	$(MAKE) vm-scheduler-contract-check
	$(MAKE) vm-actor-primitives-check
	$(MAKE) vm-failure-primitives-check
	$(MAKE) vm-supervision-primitives-check
	$(MAKE) vm-timer-primitives-check
	$(MAKE) vm-resource-ownership-check
	$(MAKE) vm-table-primitives-check
	$(MAKE) vm-code-server-check
	$(MAKE) vm-distribution-envelope-check
	$(MAKE) terlan-package-git-source-check
	$(MAKE) terlan-package-lockfile-check
	$(MAKE) adversarial-check
	$(MAKE) http-tls-check
	$(MAKE) http-runtime-stack-check
	$(MAKE) runtime-release-dependency-self-test
	$(MAKE) changelog-public-scope-check
	$(MAKE) internal-docs-check
	$(MAKE) module-readme-check
	$(MAKE) rustdoc-check
	$(MAKE) cli-check
	$(MAKE) stdlib-check
	$(MAKE) editor-check
	$(MAKE) api-schema-check
	$(PYTHON) tools/validate_ebnf.py --strict

test:
	$(MAKE) cli-test

test-release:
	$(MAKE) cli-test-release
	$(MAKE) stdlib-release-check

build:
	$(MAKE) cli-build

validate-ebnf:
	$(PYTHON) tools/validate_ebnf.py --strict

workspace-version-check:
	bash scripts/check_workspace_version_inheritance.sh

release-version-metadata-check:
	bash scripts/check_release_version_metadata.sh

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

rust-quality-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-quality

test-hierarchy-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- test-hierarchy

cli-exact-selector-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- cli-exact-selectors

shared-helper-check:
	$(PYTHON) tools/check_shared_helpers.py

installer-contract-check:
	$(PYTHON) tools/check_installer_contract.py

oxc-boundary-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- oxc-boundary

adversarial-check:
	$(CARGO) test --locked -p terlan adversarial -- --nocapture
	$(PYTHON) tools/check_release_packaging_adversarial.py

coverage-check:
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
		echo "coverage-check requires cargo-llvm-cov; install with: cargo install cargo-llvm-cov --locked"; \
		exit 127; \
	}
	$(CARGO) llvm-cov --locked --workspace --all-targets --ignore-filename-regex '$(COVERAGE_IGNORE_FILENAME_REGEX)' --fail-under-lines $(COVERAGE_MIN)

release-hardening-check:
	$(MAKE) adversarial-check
	$(MAKE) coverage-check

erlang-modernization-inventory-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- erlang-modernization-inventory

erlang-modernization-em0-hard-gate: erlang-modernization-inventory-check

release-0-0-6-preflight:
	$(MAKE) erlang-modernization-em0-hard-gate
	$(MAKE) terlan-vm-compiler-bridge-check

erlang-modernization-em0-full-compatibility-gate:
	@if [ "$${TERLAN_RUN_FULL_OTP_COMPATIBILITY:-}" != "1" ]; then \
		echo "Set TERLAN_RUN_FULL_OTP_COMPATIBILITY=1 to run the full OTP compatibility reference gate."; \
		echo "This command intentionally runs outside the normal release path."; \
		exit 64; \
	fi
	@if [ ! -d ../terlan-vm ]; then \
		echo "Missing sibling ../terlan-vm checkout for full OTP compatibility gate."; \
		exit 66; \
	fi
	@if [ ! -x ../terlan-vm/configure ]; then \
		echo "Sibling ../terlan-vm does not expose an executable ./configure script for full OTP compatibility."; \
		exit 66; \
	fi
	cd ../terlan-vm && export ERL_TOP="$$(pwd)" && ./configure && "$${MAKE:-make}" && "$${MAKE:-make}" test

erlang-runtime-matrix-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- erlang-runtime-matrix

erlang-runtime-matrix-release-check:
	TERLAN_RUNTIME_MATRIX_COMMAND='$(MAKE) test-release' $(MAKE) erlang-runtime-matrix-check

terlan-vm-artifact-format-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-artifact-format

terlc-doctor-vm-pivot-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::parse_doctor_args_defaults_to_current_directory -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::parse_doctor_args_rejects_unknown_option -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_accepts_clean_vm_project -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_vm_pivot_hazards -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_vm_execution_gap_for_checked_coreir -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_summary_compiler_contract_mismatch -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::doctor::doctor_test::doctor_project_reports_battleship_manifest_migration_fix -- --exact

native-binding-generator-contract-check:
	$(CARGO) test -p terlan --bin terlan-quality native_binding_generator_contract_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- native-binding-generator-contract

no-default-tokio-runtime-check:
	$(CARGO) test -p terlan --bin terlan-quality no_default_tokio_runtime_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- no-default-tokio-runtime

vm-process-model-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::process::process_test::process_table_allocates_monotonic_process_ids -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::process::process_test::process_table_sends_ordered_messages_and_wakes_recipient -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::process::process_test::process_selective_receive_preserves_skipped_messages -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::process::process_test::process_exit_clears_mailbox_and_returns_resource_handles -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::process::process_test::process_table_rejects_missing_recipient -- --exact

vm-scheduler-contract-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_runs_runnable_process_and_requeues_yielded_slice -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_blocks_and_wakes_processes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_exits_processes_and_returns_cleanup_handles -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_rejects_missing_blocked_and_exited_enqueue -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_skips_stale_non_runnable_queue_entries -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_cancels_process_before_running_slice -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_reports_missing_stale_queue_entry -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::scheduler::scheduler_test::scheduler_config_clamps_zero_values_and_reports_idle -- --exact

vm-actor-primitives-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_registers_names_idempotently_and_rejects_conflicts -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_send_named_wakes_and_schedules_recipient -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_receive_next_returns_message_or_blocks -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_receive_with_zero_timeout_does_not_block -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_selective_receive_preserves_skipped_messages -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_reports_missing_and_exited_context_diagnostics -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::actor::actor_test::actor_runtime_run_next_delegates_to_scheduler -- --exact

vm-failure-primitives-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_links_processes_idempotently_and_unlinks -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_propagates_abnormal_linked_exit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_does_not_propagate_normal_linked_exit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_delivers_trapped_exit_message -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_delivers_monitor_down_message_and_demonitor_suppresses_it -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_reports_missing_exited_and_self_link_diagnostics -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::failure::failure_test::failure_runtime_duplicate_exit_is_noop -- --exact

vm-supervision-primitives-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_starts_child_and_exposes_inspection_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restarts_only_failed_child_for_one_for_one_policy -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_enforces_restart_limit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_child_diagnostic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_diagnostic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_rejects_duplicate_child_id -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_restart_exits_live_child_before_restarting -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::supervision::supervision_test::supervision_system_reports_missing_supervisor_for_restart_and_snapshot -- --exact

vm-timer-primitives-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_starts_one_shot_timer_and_exposes_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_cancels_timer_and_reports_missing_timer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_fires_due_timers_only_once -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_receive_timeout_wakes_blocked_process -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_exited_process_owner -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::timer::timer_test::timer_table_rejects_missing_process_owner -- --exact

vm-resource-ownership-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_registers_resource_and_exposes_inspection_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_transfers_transferable_resource_between_live_processes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_owner_only_transfer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_after_release -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_wrong_owner_access_transfer_and_release -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_reports_stale_handle_for_transfer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_missing_process_roles -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::resource::resource_test::resource_table_rejects_exited_process_roles -- --exact

vm-table-primitives-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_creates_owner_table_and_exposes_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_inserts_replaces_looks_up_and_deletes_values -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_public_read_allows_reads_but_rejects_non_owner_writes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_owner_only_rejects_non_owner_reads_and_writes -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_public_read_write_allows_non_owner_mutation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::table::table_test::table_store_cleans_up_owner_tables_on_process_exit -- --exact

vm-code-server-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_publishes_initial_generation_and_exposes_snapshot -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_hot_reload_binds_new_processes_to_new_generation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_release_retires_drained_old_generation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_hot_reload_retires_unused_previous_generation_immediately -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::code_server::code_server_test::code_server_reports_missing_module_and_exited_process_diagnostics -- --exact

vm-distribution-envelope-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encodes_primitive_values_with_header -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_rejects_atoms_missing_from_manifest -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_accepts_declared_atoms -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_map_encoding_is_deterministic_by_encoded_key -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_set_encoding_sorts_and_deduplicates_items -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_rejects_runtime_only_values -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encodes_vm_refs_with_kind_node_local_id_and_epoch -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_encodes_distribution_envelope_with_refs_and_payload -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::term_format::term_format_test::tetf_distribution_envelope_rejects_payload_atoms_missing_from_manifest -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_accepts_compatible_peer_with_required_capability -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_rejects_cross_cluster_peer -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_rejects_missing_capability -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_message_ids_are_monotonic -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::coordination::coordination_test::vm_coordination_builds_tetf_distribution_envelope_with_refs -- --exact

terlan-package-git-source-check:
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::build::project_manifest::project_manifest_test::project_manifest_parses_dependency_source_metadata -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlc commands::build::project_manifest::project_manifest_test::project_manifest_rejects_git_dependency_without_rev -- --exact
	$(CARGO) test -p terlan --bin terlan-quality package_git_source_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-git-source

terlan-package-lockfile-check:
	$(CARGO) test -p terlan --bin terlan-quality package_lockfile_contract_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- package-lockfile-contract

native-boundary-postgres-baseline-benchmark:
	$(CARGO) run -p terlan --bin terlan-benchmark --quiet -- native-boundary-postgres-baseline

native-boundary-http-baseline-benchmark:
	$(CARGO) run -p terlan --bin terlan-benchmark --quiet -- native-boundary-http-baseline

terlan-vm-compiler-bridge-check:
	$(MAKE) --no-print-directory cli-terlan-vm-compiler-bridge-check

http-runtime-stack-check:
	$(PYTHON) tools/check_http_runtime_stack.py
	$(TERLC_EXACT_TEST) commands::build::build_test::tests::artifact_test::build_command_emits_http_request_body_json_direct_erlang_lowering -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_get_response -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_serves_static_file_with_query_string -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_omits_static_head_response_body -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_static_parent_path -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_rejects_unmatched_mutating_method -- --exact
	$(TERLC_EXACT_TEST) commands::serve::serve_test::hyper_request_handler_streams_reload_sse_events -- --exact

runtime-release-dependency-self-test:
	$(PYTHON) tools/check_runtime_release_dependencies.py --self-test

changelog-public-scope-check:
	$(PYTHON) tools/check_changelog_public_scope.py

internal-docs-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- internal-docs

module-readme-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- module-readmes

rustdoc-check:
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- rust-docs

release-artifact-current:
	$(MAKE) release-boundary-check
	$(MAKE) release-version-metadata-check
	$(MAKE) source-extension-check
	$(MAKE) cli-release-artifact-current
	$(MAKE) release-artifact-smoke
	$(MAKE) release-artifact-installer-smoke

release-artifact-linux:
	TERLAN_RELEASE_OS=Linux TERLAN_RELEASE_ARCH=x86_64 $(MAKE) release-artifact-current

release-artifact-smoke:
	$(PYTHON) tools/package_release_artifact.py smoke

release-artifact-installer-smoke:
	$(PYTHON) tools/package_release_artifact.py installer-smoke

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
	@if git ls-remote --exit-code --tags origin "refs/tags/v$(VERSION)" >/dev/null 2>&1; then \
		echo "remote tag v$(VERSION) already exists"; \
		exit 1; \
	fi
	@if [ "$(VERSION)" = "0.0.6" ]; then \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-hardening-check; \
		$(MAKE) release-0-0-6-preflight; \
		$(MAKE) release-artifact-current; \
	elif [ "$(VERSION)" = "0.0.5" ]; then \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-hardening-check; \
		$(MAKE) release-0-0-5-preflight; \
		$(MAKE) release-artifact-current; \
	elif [ "$(VERSION)" = "0.0.4" ]; then \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-hardening-check; \
		$(MAKE) release-0-0-4-preflight; \
		$(MAKE) release-artifact-current; \
	else \
		$(MAKE) check; \
		$(MAKE) test-release; \
		$(MAKE) release-hardening-check; \
		$(MAKE) release-artifact-current; \
	fi

publish: publish-preflight
	@if ! git rev-parse -q --verify "refs/tags/v$(VERSION)" >/dev/null; then \
		git tag "v$(VERSION)"; \
	fi
	git push origin main
	git push origin "v$(VERSION)"
	@echo "Published tag v$(VERSION). GitHub Actions will build and upload the release artifact."

clean:
	$(MAKE) cli-clean
