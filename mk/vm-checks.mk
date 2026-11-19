# ---- Terlan VM / AOT runtime checks ----

# Verifies the VM hashmap implementation is deterministic.
vm-deterministic-hashmap-check:
	$(RUST_TEST) -p terlan --lib --features validation-union vm_deterministic_hashmap_test
	$(TERLAN_QUALITY) vm-deterministic-hashmap


# Validates the VM release installation.
vm-release-install-validation-check:
	$(TERLAN_QUALITY) vm-release-install-validation


# Validates the VM release artifact matrix across install/artifact/smoke checks.
vm-release-artifact-matrix-check: \
	vm-release-install-validation-check \
	cli-release-artifact-current \
	release-artifact-installer-smoke
	$(TERLAN_TVM_PLATFORM_MATRIX) release-artifact-matrix


# Alias check that depends on the VM artifact format check
tvm-native-image-format-check: terlan-vm-artifact-format-check


# Checks the direct AOT backend and native-image transitions on the local shard
tvm-direct-aot-backend-check: terlan-vm-artifact-format-check
	$(RUST_TEST) -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_local_shard native_image_transitions_execute_on_the_local_shard -- --exact


# Checks native-IR lowering coverage via Rust tests
tvm-aot-lowering-coverage-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::lowering_coverage_test


# Checks native-IR application admission, static-callable, and closure specialization
tvm-aot-application-closure-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::application_admission_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::static_callable_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::higher_order_specialization_test


# Checks native-IR case-expression lowering via Rust tests
tvm-aot-case-lowering-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::case_lowering_test


# Checks managed operation-ABI and native-IR field projection via Rust tests
tvm-aot-managed-field-projection-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::field_projection_test


# Checks the managed owned-closure representation via Rust tests
tvm-aot-owned-closure-representation-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::managed_closure_test


# Checks managed closure dispatch, conversion, and deterministic artifact bytes
tvm-aot-closure-dispatch-check:
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::managed_closure_dispatch_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::closure_conversion_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::cranelift::managed_callback_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::vm_artifact::native_descriptor_test::lifted_callable_descriptor_separates_captures_from_call_arguments -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::managed_execution_test::executable_metadata_installs_generation_scoped_closure_dispatch -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::descriptor_round_trip_is_canonical_and_deterministic -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::descriptor_rejects_invalid_abi_ids_and_boundary_types -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::deterministic_artifact_test::deterministic_module_emits_reproducible_vm_artifact_bytes -- --exact


# Checks typed process mailbox operations and actor message delivery
tvm-aot-typed-mailbox-check:
	$(RUST_TEST) -p terlan --lib compiler::typeck::core_intrinsic_process_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::trait_negative_test::syntax_output_rejects_actor_message_with_denied_delivery -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::trait_negative_test::syntax_output_accepts_actor_and_node_values_with_default_delivery -- --exact
	$(RUST_TEST) -p terlan --lib compiler::native_ir::cranelift::managed_type_test


# Checks typed process lifecycle transitions and their native-IR lowering
tvm-aot-typed-lifecycle-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_process_test::syntax_output_lowering_keeps_entry_operation_and_erases_value_descriptors -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_typed_process_operations_require_explicit_specialization -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_accepts_explicit_typed_process_lifecycle_operations -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::expression_test::comprehensions_calls_and_collections::syntax_output_rejects_scalar_process_lifecycle_arguments -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::cranelift::managed_type_test::typed_public_lifecycle_operations_lower_to_existing_vm_transitions -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::lowering_coverage_test::intrinsic_families_have_explicit_lowering_dispositions -- --exact
	$(RUST_TEST) -p terlan --lib vm::main_test::native_transition_test


# Checks managed continuation handling via the AOT continuation test suite
tvm-aot-managed-continuation-check:
	$(RUST_TEST) -p terlan --test managed_continuation_aot


# Checks that parked native continuations remain thread-neutral and Send/Sync
tvm-aot-thread-neutral-continuation-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::thread_neutral::thread_neutral_test::parked_native_continuation_is_send_sync_and_static -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::pure_native_transport_test::parked_native_continuation_resumes_after_thread_transfer -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::pure_native::direct_backend::direct_backend_test::direct_backend_parked_state_is_send_sync_and_static -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_suspension_test


# Checks multicore readiness: shard isolation, concurrency, and absence of global locks
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


# Runs the AOT ThreadSanitizer lane when the pinned nightly toolchain is available
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


# Checks the AOT ThreadSanitizer contract via the TVM contract-check tool
tvm-aot-thread-sanitizer-contract-check: | $(TERLAN_TVM_CONTRACT_BOOTSTRAP)
	$(TERLAN_TVM_CONTRACT_CHECK) tsan-self-test

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
	tvm-aot-multicore-readiness-check \
	tvm-aot-c-abi-boundary-check \
	tvm-aot-compilation-time-check \
	tvm-single-image-artifact-check \
	no-tvm-json-runtime-check \
	no-vmir-interpreter-check \
	rust-quality-check

# The default-feature library check has a different build policy from the
# union-feature correctness suite; a passed suite cannot silently replace it.
# Keep the check under the same process owner as the other release producers so
# actual Cargo launch/reap evidence is retained instead of inferred from Make.
AOT_RELEASE_CARGO_CHECK := $(TERLAN_RUST_ORCHESTRATOR) --run-owned --timeout-seconds $(TERLAN_COMPILER_BUILD_TIMEOUT_SECONDS) -- env -u RUSTFLAGS $(CARGO) check -p terlan

# Local gates are always admitted by the current Make graph. Target-level
# deduplication handles overlap within one invocation; no caller-provided
# "already run" flag may suppress correctness work across invocations.
AOT_RELEASE_LOCAL_GATES_TO_RUN := $(AOT_RELEASE_LOCAL_GATES)


# Checks the AOT release closeout contract via the TVM contract-check tool
tvm-aot-release-closeout-contract-check: tvm-aot-thread-sanitizer-contract-check tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_CONTRACT_CHECK) release-self-test

.PHONY: tvm-aot-release-prerequisites
# Records the AOT release closeout via the TVM platform-matrix tool
tvm-aot-release-closeout-check: tvm-aot-release-prerequisites
	$(TERLAN_TVM_PLATFORM_MATRIX) release-record


# Runs all AOT release local gates plus the default-feature cargo check
tvm-aot-release-prerequisites: tvm-aot-release-closeout-contract-check
	$(MAKE) $(AOT_RELEASE_LOCAL_GATES_TO_RUN)
	$(AOT_RELEASE_CARGO_CHECK)


# Verifies the hosted TVM platform-matrix release report before publication
tvm-aot-publish-evidence-check:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	test -s target/quality/tvm-aot-platform-matrix-report.json
	timeout 300s $(TERLAN_TVM_PLATFORM_MATRIX) hosted-release-verify \
		target/quality/tvm-aot-platform-matrix-report.json "$$(git rev-parse HEAD)"


# Checks native-IR static-callable lowering via Rust tests
tvm-aot-static-callable-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::static_callable_test


# Checks native-IR higher-order specialization and captured-lambda lowering
tvm-aot-higher-order-specialization-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::higher_order_specialization_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::static_callable_test::source_captured_lambda_lowers_into_native_application -- --exact


# Checks native-IR application-lowering conformance across many lowering passes
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


# Checks managed memory: atoms, sequences, aggregates, lists, maps, sets, and escapes
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


# Checks the sandboxed capability-worker process and its boundary protocol
tvm-aot-capability-worker-check: tvm-aot-stale-epoch-check | terlan-native-worker-bootstrap
	$(RUST_TEST) -p terlan --lib main_test
	$(RUST_TEST) -p terlan --lib sandbox
	$(RUST_TEST) -p terlan --lib capability_wire
	$(RUST_TEST) -p terlan --lib protocol::protocol_test
	$(RUST_TEST) -p terlan --lib native_worker::protocol::storage::tests
	$(RUST_TEST) -p terlan --lib capability_worker
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::package_native_helper::storage_transport::tests::installed_storage_worker_is_selected_only_from_the_vm_directory -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::package_native_helper::storage_transport::tests::unconfigured_actor_execution_does_not_require_an_installed_storage_worker -- --exact
	TERLAN_TEST_CAPABILITY_WORKER=$(TERLAN_RUST_SUITE_NATIVE_WORKER) $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_process_transport_runs_full_cycle -- --ignored --exact
	TERLAN_TEST_CAPABILITY_WORKER=$(TERLAN_RUST_SUITE_NATIVE_WORKER) $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::capability_worker_sandbox_closes_inherited_descriptor -- --ignored --exact
	TERLAN_TEST_CAPABILITY_WORKER=$(TERLAN_RUST_SUITE_NATIVE_WORKER) $(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::capability_worker::capability_worker_test::storage_process_test::capability_storage_worker_survives_restart_with_durable_checkpoint -- --ignored --exact
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

TERLAN_MANAGED_LIST_PROFILE_RUN = TERLAN_MANAGED_LIST_PROFILE_OUTPUT=$(CURDIR)/target/quality/tvm-managed-list-profile.json $(EXACT_CARGO_TEST) --locked --release -p terlan --lib runtime::native_image::managed::lists::managed_list_profile_benchmark_test::managed_list_profiles_emit_stable_benchmark_report -- --exact --nocapture
# Runs the managed-list profiling benchmark and checks the report was written
tvm-managed-list-profile-benchmark-check:
	$(TERLAN_MANAGED_LIST_PROFILE_RUN)
	test -s target/quality/tvm-managed-list-profile.json

.PHONY: tvm-aot-runtime-workload-benchmark-check

.PHONY: tvm-native-optimizations-check tvm-native-optimizations-benchmark
# Checks native scalar-replacement, atom, list, and string-join optimizations
tvm-native-optimizations-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib scalar_replacement -- --nocapture
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::atoms -- --nocapture
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::lists -- --nocapture
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::string::join_test -- --nocapture


# Benchmarks native atom-table and string-join profiles and checks reports exist
tvm-native-optimizations-benchmark: tvm-aot-runtime-workload-benchmark-check
	TERLAN_ATOM_TABLE_PROFILE_OUTPUT=$(CURDIR)/target/quality/tvm-atom-table-profiles.json $(EXACT_CARGO_TEST) --locked --release -p terlan --lib paired_atom_table_profiles_report_storage_and_lifecycle -- --nocapture
	TERLAN_STRING_JOIN_PROFILE_OUTPUT=$(CURDIR)/target/quality/tvm-string-join-profiles.json $(EXACT_CARGO_TEST) --locked --release -p terlan --lib paired_string_join_profiles_report_memory_and_tail_latency -- --nocapture
	test -s target/quality/tvm-atom-table-profiles.json
	test -s target/quality/tvm-string-join-profiles.json


# Benchmarks AOT runtime workloads and verifies p99 latency evidence is present
tvm-aot-runtime-workload-benchmark-check: | terlan-benchmark-release-bootstrap
	TERLAN_BENCH_AOT_RUNTIME_OUTPUT=$(CURDIR)/target/quality/vm-aot-runtime-workloads.json target/release/terlan-benchmark vm-aot-runtime-workloads
	test -s target/quality/vm-aot-runtime-workloads.json
	@rg -q '"status": "completed"' target/quality/vm-aot-runtime-workloads.json
	@rg -q '"p99_ns":' target/quality/vm-aot-runtime-workloads.json
	TERLAN_JSON_EVIDENCE_PATH=target/quality/vm-aot-runtime-workloads.json \
	TERLAN_JSON_EVIDENCE_REQUIRED='"name": "actor_heap_allocation";;"name": "local_message_round_trip";;"name": "scheduler_yield_cycle";;"name": "actor_local_collection_pause";;"name": "actor_spawn_exit_churn";;"name": "mixed_actor_runtime_tail"' \
		target/debug/terlc test scripts/self_validation/JsonEvidenceContractTest.terl \
			--name selected_json_evidence_holds


# Alias check depending on the direct AOT backend check
tvm-native-image-loader-check: tvm-direct-aot-backend-check


# Checks the native-image loader rejects unsupported transitions
tvm-aot-consumer-check: tvm-native-image-loader-check
	$(RUST_TEST) -p terlan --test tvm_transition_rejection


# Checks that the test command consumes native images without evaluator fallback
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


# Checks that the REPL consumes native images without evaluator or runtime fallback
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


# Checks that the debugger consumes native images without evaluator fallback
tvm-aot-debugger-consumer-check: tvm-aot-consumer-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_renamed_json_target -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::native_debug_session_rejects_stale_source_map -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::debug::debug_test::live_debugger_rejects_missing_process_and_frame_eval_with_stable_errors -- --exact
	@rg -q 'PureNativeExecutionShard::load_image' crates/terlan/src/commands/debug/session.rs
	@rg -q 'inspect_tvm_native_debug' crates/terlan/src/commands/debug/session.rs
# Pure debugger expressions use the production native REPL compilation path.
# Reject retired runtime entry points, not the general verb "evaluate".
	@if rg -n 'struct TerlanVm|runtime::vm::TerlanVm|evaluate_repl_function|apply_closure|\.tvm\.json|PureNativeWorker|Evaluator|evaluator' \
		crates/terlan/src/commands/debug --glob '*.rs' --glob '!**/*test*'; then \
		echo 'error[aot.debugger_consumer]: debugger admission must use a native image without evaluator or serialized-runtime fallback'; \
		exit 1; \
	fi


# Checks native hot-reload generation publication and quarantine behavior
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


# Checks native-image package install and release validation consumer paths
tvm-aot-package-install-consumer-check: tvm-aot-consumer-check
	$(TERLAN_TVM_PACKAGE_CONSUMER) self-test
	$(TERLAN_TVM_PACKAGE_CONSUMER) check
	$(TERLAN_QUALITY) vm-release-install-validation


# Checks that crash support-bundle metadata excludes executable code and source paths
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


# Checks ignore-cores handling matches the non-mutating BEAM-parity replacement
vm-ignore-cores-parity-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::fatal_diagnostics::fatal_diagnostics_ignore_cores_parity_test::ignore_cores_helper_is_replaced_by_explicit_non_mutating_artifact_publication -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::fatal_diagnostics
	@! rg -q 'ignore_core_files|/cores|set_current_dir|set_var.*PWD' crates/terlan/src/runtime/vm/fatal_diagnostics.rs


# Checks IO-vector handling matches BEAM-suite parity
vm-iovec-suite-parity-check:
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::iovec::iovec_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::driver::driver_beam_suite_parity_test
	@rg -q 'let mut pending = vec!\[value\]' crates/terlan/src/runtime/vm/iovec.rs
	@rg -q 'Arc::clone\(bytes\)' crates/terlan/src/runtime/vm/iovec.rs
	@rg -q '0\.\.8_192' crates/terlan/src/runtime/vm/iovec_beam_suite_parity_test.rs
	@rg -q 'write_vectored' crates/terlan/src/runtime/vm/protocol_task_executor/transport.rs crates/terlan/src/commands/serve/hyper_server.rs


# Checks scheduler lock-count/contention telemetry matches BEAM-suite parity
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


# Checks list built-in-function handling matches BEAM-suite parity
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


# Checks the literal-area collector maps to synchronous generation quiescence
vm-literal-area-collector-parity-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::literal_area_collector_helper_maps_to_synchronous_generation_quiescence -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::literal_area_collector_helper_observes_actor_drain_without_polling -- --exact


# Checks LTTng tracing instrumentation matches BEAM-suite parity
vm-lttng-suite-parity-check: vm-iovec-suite-parity-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::driver::lttng_beam_suite_parity_test
	@rg -q 'VM_DRIVER_TRACE_CAPACITY: usize = 4_096' crates/terlan/src/runtime/vm/driver/trace.rs
	@rg -q 'enabled_mask: u8' crates/terlan/src/runtime/vm/driver/trace.rs
	@rg -q 'expired; oldest retained sequence' crates/terlan/src/runtime/vm/driver/trace.rs
	@! rg -q 'org_erlang_otp|erl_driver|std::process|Command::new' crates/terlan/src/runtime/vm/driver.rs crates/terlan/src/runtime/vm/driver/trace.rs


# Verifies native map ops match BEAM map-suite parity without a CoreIR fallback
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


# Verifies native match-spec lowering and actor-trace parity against the BEAM suite
vm-match-spec-suite-parity-check: vm-map-suite-parity-check vm-guard-suite-parity-check vm-table-primitives-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib compiler::native_ir::match_spec_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_call_trace_beam_suite_parity_test
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_trace_meta_beam_suite_parity_test
	@rg -q 'assert_managed_native_object_invocations' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@rg -q 'aot_comprehension_' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@rg -q 'dynamic match-spec call must fail before native linking' crates/terlan/src/compiler/native_ir/match_spec_suite_native_parity_test.rs
	@! rg -q 'match_spec|match-spec' crates/terlan/src/runtime --glob '*.rs'


# Verifies native module_info parity and code-server function introspection
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


# Verifies scheduler mutex producer/writer parity without locks or BEAM thread primitives
vm-mtx-suite-parity-check: vm-multicore-memory-model-check vm-table-primitives-check
	$(CARGO) check -p terlan --bin terlan-vm
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_mtx_beam_suite_parity_test
	@rg -q 'const PRODUCERS: usize = 20' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'const WRITERS: usize = 6' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'ACTOR_MAILBOX_CAPACITY' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@! rg -q 'Mutex|RwLock|sleep|Instant|SystemTime|erl_nif|ethr_' crates/terlan/src/runtime/vm/fixed_scheduler_control_mtx_beam_suite_parity_test.rs


# Verifies concurrent multi-module loading rejects duplicate module identities atomically
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


# Verifies native record/aggregate field projection and term-format parity in actor messaging
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


# Verifies the native boundary has no Erlang NIF-compatible surface
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


# Verifies admitted native images stay sealed and immutable across replacement and cleanup
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


# Verifies the public C ABI boundary hides internal runtime details like the actor heap
tvm-aot-c-abi-boundary-check: native-binding-consistency-check
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::adapter_abi::adapter_abi_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::native_image_test::native_inspection_accepts_real_elf_and_rejects_wrong_target_and_abi -- --exact
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
# Validates the cross-platform release-matrix contract: checksums, packaging, cleanup
tvm-aot-platform-matrix-contract-check: | $(TERLAN_TVM_CONTRACT_BOOTSTRAP)
	$(TERLAN_TVM_CONTRACT_CHECK) self-test
	$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleaseChecksumsTest.terl
	TERLAN_RELEASE_TEST_COMPILER="$(abspath $(TERLAN_BOOTSTRAP_COMPILER))" $(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleasePackageTest.terl
	$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/LinuxCompatibilityTest.terl
	$(TERLAN_BOOTSTRAP_COMPILER) test scripts/self_validation/tvm_aot_platform_matrix/src/tvm_aot_platform_matrix/ReleaseCleanupTest.terl


# Runs the platform release matrix for the current target
tvm-aot-platform-target-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) target


# Aggregates platform release-matrix results into the shared report root
tvm-aot-platform-aggregate-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) aggregate $(TERLAN_TVM_PLATFORM_REPORT_ROOT)


# Runs the platform release matrix as the umbrella target check
tvm-aot-platform-matrix-check: tvm-aot-platform-matrix-contract-check
	$(TERLAN_TVM_PLATFORM_MATRIX) target


# Verifies managed HTTP literal/value/response-bridge lifecycle survives source reload
tvm-aot-http-managed-cycle-check: tvm-aot-consumer-check
	$(RUST_TEST) -p terlan --lib literal_abi_test
	$(RUST_TEST) -p terlan --lib http_values_test
	$(RUST_TEST) -p terlan --lib response_bridge_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_source_reload_resumes_package_cookie_codec -- --exact


# Verifies HTTP request field accessors preserve missing and empty values
tvm-aot-http-request-accessor-check: tvm-aot-http-managed-cycle-check
	$(RUST_TEST) -p terlan --lib operation_abi_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_request_library_test::request_library_reads_its_own_fields_and_preserves_missing_and_empty_values -- --exact


# Verifies HTTP response mutations persist and preserve repeated headers
tvm-aot-http-response-mutation-check: tvm-aot-http-request-accessor-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::response_updates_are_persistent_and_preserve_repeated_headers -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_response_library_test::mutation_authority_test::response_mutators_execute_provider_bodies_and_preserve_command_results -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::response_bridge::response_bridge_test::native_repeated_headers_are_validated_and_preserved -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_source_reload_resumes_package_cookie_codec -- --exact


# Verifies typed HTTP/cookie metadata and security headers stay off transport framing
tvm-aot-http-typed-metadata-check: tvm-aot-http-response-mutation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::retired_http_test::retired_http_operations_reject_before_reading_or_allocating_heap_values -- --exact
	$(RUST_TEST) -p terlan --lib compiler::native_ir::http_response_library_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::http_cookie_library_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::response_policy_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::response_bridge::response_bridge_test::native_security_headers_are_not_claimed_by_transport_framing -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_source_reload_resumes_package_cookie_codec -- --exact


# Verifies the materialized router dispatches string-keyed routes without hyper
tvm-aot-http-router-callable-check: tvm-aot-http-typed-metadata-check
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::source_router::tests
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::case_lowering_test::string_case_patterns_lower_to_managed_value_equality -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_equal_operation_is_value_based_and_checked -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_append_operation_concatenates_validated_values -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact


# Verifies HTTP error handling and scalar projection use unboxed native values
tvm-aot-http-managed-error-check: tvm-aot-http-router-callable-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::aggregate_scalar_projection_returns_an_unboxed_native_word -- --exact
	$(RUST_TEST) -p terlan --lib compiler::native_ir::http_error_library_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact


# Verifies managed HTML template rendering executes through the native handler
tvm-aot-http-template-check: tvm-aot-http-managed-error-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::template_values_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::string_list_join
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_managed_template_html_handler -- --exact


# Verifies validated template render plans flow from the formal pipeline into CoreIR
tvm-aot-http-template-render-plan-check: tvm-aot-http-template-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib formal_pipeline::formal_pipeline_test::checked_evidence::formal_pipeline_carries_validated_template_render_plans_into_core_ir -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::html_escape_operations_preserve_context_and_validate_arity -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_external_template_render_plan -- --exact


# Verifies typed template expressions enforce attribute, option, and URL safety
tvm-aot-http-template-expression-check: tvm-aot-http-template-render-plan-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::template_values_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::operation_abi_test::typed_template_rendering_enforces_attributes_options_and_urls -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_request_executes_complete_typed_template_matrix -- --exact


# Verifies JSON body decoding honors the source policy and guarded result matching
tvm-aot-http-body-json-check: tvm-aot-http-template-expression-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::json_library_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_request_library_test::body_json_uses_source_policy_and_ordinary_guarded_result_matching -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::hyper_server::hyper_server_test::json_body::vm_stream_request_decodes_managed_json_body_result -- --exact


# Verifies HTTP session state and lifecycle are owned and persisted by the VM actor runtime
tvm-aot-http-session-check: tvm-aot-http-body-json-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::http_values_test::session_authority_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::native_image::managed::operation_abi::retired_session_test::retired_session_opcodes_cannot_read_or_allocate_heap_values -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::http_session::package_storage::tests
	$(RUST_TEST) -p terlan --lib runtime::vm::protocol_task_executor::maintenance::tests
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_state::tests
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::response_policy_test::session_provider_test
	$(RUST_TEST) -p terlan-http-native --lib session_
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::http_session::http_session_test::session_actor_fixtures::http_session_adapter_functions_delegate_to_actor_runtime -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::observability_and_packages::vm_stream_session_state_and_lifecycle_are_vm_owned -- --exact


# Verifies retired HTTP intrinsics have no runtime layout, type, or executable lowering
tvm-aot-http-managed-boundary-check: tvm-aot-http-session-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::http_imports_do_not_install_runtime_layouts -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::retired_http_calls_have_no_inferred_type_or_executable_lowering -- --exact


# Verifies SSE and websocket upgrades route through materialized middleware with defaults
tvm-aot-http-channel-plan-check: tvm-aot-http-managed-boundary-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::http_values_test::option_authority_test::map_lookup_patterns_support_literals_guards_and_source_option_defaults -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::serve_test::upgrades_and_acme::vm_stream_websocket_upgrade_activates_materialized_router_middleware -- --exact


# Verifies persistent handler shards balance and resume parked actors without migration or locks
tvm-aot-http-persistent-shard-check: tvm-aot-http-channel-plan-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::one_owner_loop_services_multiple_parked_actors_without_migration -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::new_actors_balance_across_shards_and_resume_sticky -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_timer_resumes_after_its_owner_deadline -- --exact
	@! rg -q 'spawn_shard\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'shard\.shutdown\(\)' crates/terlan/src/commands/serve/handler_cache/invocation.rs
	@! rg -q 'Mutex<PureNativeExecutionShard' crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache/
	@rg -q 'sync_channel\(SHARD_INBOX_CAPACITY\)' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs


# Records the Axum baseline HTTP performance benchmark as a comparison artifact
tvm-http-axum-performance-record: terlan-http-benchmark-release-bootstrap
	TERLAN_BENCH_HTTP_AXUM_BIN=$(CURDIR)/target/release/terlan-axum-baseline \
	TERLAN_BENCH_HTTP_AXUM_OUTPUT=$(CURDIR)/target/quality/http-axum-performance.json \
	$(CURDIR)/target/release/terlan-http-framework-benchmark
	test -s target/quality/http-axum-performance.json
	@rg -q '"schema": "terlan-http-framework-performance-v2"' target/quality/http-axum-performance.json
	@rg -q '"memory_snapshots":' target/quality/http-axum-performance.json
	@rg -q '"rounds":' target/quality/http-axum-performance.json

HTTP_PAIRED_OUTPUT ?= $(CURDIR)/target/quality/http-paired-performance.json


# Runs the paired AOT-vs-axum-vs-hyper benchmark and validates its confidence report
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


# Umbrella check aggregating native HTTP invocation prerequisites
tvm-aot-http-native-invocation-check: tvm-aot-http-persistent-shard-check


# Verifies websocket callbacks share the native invocation entry, resume, and cancellation path
tvm-aot-http-websocket-invocation-check: tvm-aot-http-native-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::websocket_invocation::websocket_invocation_test::websocket_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact


# Verifies SSE callbacks share the native invocation entry, resume, and cancellation path
tvm-aot-http-sse-invocation-check: tvm-aot-http-websocket-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler::sse_invocation::sse_invocation_test::sse_callbacks_share_native_invocation_entry_resume_and_cancellation -- --exact


# Verifies hot reload pins an in-flight handler generation until its last lease drops
tvm-aot-http-generation-lifetime-check: tvm-aot-http-sse-invocation-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::hot_reload_pins_in_flight_generation_until_its_last_lease_drops -- --exact


# Verifies production channel pumps preserve VM lifecycle and backpressure contracts
tvm-aot-http-channel-transport-check: tvm-aot-http-generation-lifetime-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::serve::channel_transport::channel_transport_test::production_channel_pumps_preserve_vm_lifecycle_and_pressure_contracts -- --exact


# Verifies requests, sessions, timers, and resources are released on cancellation or exit
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


# Verifies the HTTP AOT lifecycle inventory doc is complete and performance evidence is consistent
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


# Records the checked-CoreIR terlc performance baseline for later AOT comparison
tvm-aot-http-checked-coreir-reference-record: | terlan-benchmark-release-bootstrap
	@test -n "$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN)" || (echo 'error[aot.http.performance]: set TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN to the preserved checked-CoreIR terlc binary' && exit 1)
	target/release/terlan-benchmark http-aot-performance-self-test
	TERLAN_BENCH_HTTP_AOT_LANE=checked-coreir \
	TERLAN_BENCH_HTTP_AOT_TERLC_BIN=$(TERLAN_BENCH_CHECKED_COREIR_TERLC_BIN) \
	TERLAN_BENCH_HTTP_AOT_OUTPUT=$(CURDIR)/../benchmarks/results/http-checked-coreir-performance.json \
	target/release/terlan-benchmark http-aot-performance
	test -s ../benchmarks/results/http-checked-coreir-performance.json


# Verifies a package build emits one TVM image and the native cache rejects corrupt entries
tvm-single-image-artifact-check: tvm-native-image-format-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::vm_artifact::native_cache_test::native_cache_rejects_poisoned_keys_target_drift_and_incomplete_publications -- --exact
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::native_reuse::native_reuse_test
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_package package_build_emits_one_tvm_image_with_qualified_module_exports -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_cache native_aot_cache_verifies_and_recovers_every_required_file -- --exact


# Verifies native build policy selection and incremental image cache reuse
tvm-aot-compilation-time-check: tvm-single-image-artifact-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::specialization_budget_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::codegen_policy_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::args_test::parse_build_args_selects_explicit_native_release_policy -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::args_test::build_command_rejects_release_policy_for_non_vm_target -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --test direct_aot_cache native_codegen_policies_publish_and_reuse_distinct_cache_entries -- --exact
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::parallel_compile_test
	$(RUST_TEST) -p terlan --lib commands::build::source_roots_test
	$(RUST_TEST) -p terlan --lib commands::build::vm_artifact::checked_cache_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::build::build_test::tests::parallel_compilation_test::parallel_frontend_compilation_preserves_one_application_link -- --exact
	$(POLICY_CARGO_TEST) --locked --release -p terlan --test direct_aot_cache vm_aot_timings_report_compile_and_native_artifact_phases -- --exact
	$(POLICY_CARGO_TEST) --locked --release -p terlan --test direct_aot_cache unchanged_repl_generation_reuses_native_image_without_relinking -- --exact

.PHONY: tvm-aot-shard-ownership-check
# Verifies execution shards own actor state locally with no global locks or worker transport
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
# Verifies the shard supervisor drives admission, replacement, and crash-recovery transitions
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
# Verifies stale and duplicate shard operations are rejected using a typed, non-u64 epoch
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


# Verifies injected shard/actor crashes recover correctly and injection stays test-only
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


# Alias umbrella target for the native runtime-transition focused check
tvm-aot-runtime-transition-check: tvm-aot-runtime-transition-focused-check


# Verifies every native actor transition executes locally with strict ownership
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


# Runs the quality gate for the native worker runtime
vm-native-worker-runtime-check: terlan-vm-artifact-format-check stdlib-native-artifacts-check
	$(TERLAN_QUALITY) vm-native-worker-runtime


# Runs the quality gate for the VM's own I/O reactor runtime
vm-io-reactor-runtime-check: vm-native-worker-runtime-check no-default-tokio-runtime-check
	$(TERLAN_QUALITY) vm-io-reactor-runtime


# Verifies supervisor restart strategies handle crashes, backpressure, and shutdown deadlines
vm-supervision-restart-check: vm-supervision-primitives-check vm-timer-deadline-check
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_parent_strategy_restarts_failed_and_sibling_supervisor_subtrees -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_native_boundary_worker_crash_uses_vm_backoff_and_restart -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_handler_pool_memory_exhaustion_restarts_the_group -- --exact
	$(EXACT_CARGO_TEST) -p terlan --test vm_supervision_runtime product_in_flight_shutdown_timeout_cancels_old_actor_and_restarts -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::supervision::source::source_test
	$(TERLC) test tests/language/VmSupervisorPolicyTest.terl tests/language/VmSupervisorRuntimeTest.terl
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_relationship_test::actor_unlinked_child_termination_preserves_parent_mailbox_progress -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_waits_for_clean_exit_and_cancels_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_distinguishes_in_budget_and_overdue_child_termination -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_deadline_forces_typed_exit_and_restarts_child -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_normal_exit_honors_transient_restart_class -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::supervision::shutdown::shutdown_test::supervision_shutdown_rejects_duplicate_and_deadline_overflow_atomically -- --exact
	$(RUST_TEST) -p terlan --lib --features validation-union vm_supervision_restart_test
	$(TERLAN_QUALITY) vm-supervision-restart


# Umbrella check gating HTTP handler dispatch on the I/O reactor runtime
vm-http-handler-dispatch-check: vm-io-reactor-runtime-check


# Runs the quality gate for fair scheduling across HTTP handler invocations
vm-http-handler-scheduler-fairness-check: vm-http-handler-dispatch-check | terlan-benchmark-release-bootstrap
	target/release/terlan-benchmark http-aot-performance-self-test
	$(TERLAN_QUALITY) vm-http-handler-scheduler-fairness


# Runs the quality gate for stateful actor-backed HTTP sessions
vm-http-stateful-actor-session-check: vm-http-handler-scheduler-fairness-check
	$(TERLAN_QUALITY) vm-http-stateful-actor-session


# Runs the quality gate for live template streaming over SSE and websocket channels
vm-live-template-stream-check: \
	vm-http-stateful-actor-session-check \
	vm-http-sse-check \
	vm-http-websocket-source-check \
	vm-http-websocket-queue-check \
	vm-http-websocket-termination-check \
	vm-http-live-channel-source-check
	$(TERLAN_QUALITY) vm-live-template-stream


# Runs the quality gate for the Angular client protocol over live template streams
vm-live-template-client-protocol-check: \
	vm-live-template-stream-check \
	angular-ts-terlan-integration-check \
	angular-ts-namespace-generation-check
	$(TERLAN_QUALITY) vm-live-template-client-protocol


# Runs the quality gate for web security policy covering TLS and cookie handling
vm-web-security-policy-check: \
	web-asset-pipeline-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(TERLAN_QUALITY) vm-web-security-policy


# Runs the quality gate for secret boundaries in web configuration
vm-web-config-secret-boundary-check: \
	vm-web-security-policy-check \
	http-tls-check \
	web-compose-check
	$(TERLAN_QUALITY) vm-web-config-secret-boundary


# Runs the quality gate for end-to-end web observability
vm-web-observability-check: \
	vm-http-serve-config-check \
	vm-runtime-observability-check \
	vm-web-config-secret-boundary-check \
	http-observability-check \
	vm-diagnostics-quality-check
	$(TERLAN_QUALITY) vm-web-observability


# Verifies serve config precedence, validation, and replay-stable fingerprinting
vm-http-serve-config-check:
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_precedence_is_default_manifest_environment_cli -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_unsafe_public_default_before_socket_startup -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_accepts_explicit_public_bind_and_tracks_its_origin -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_malformed_and_ambiguous_values -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_unsupported_protocol_and_bad_environment -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_fingerprint_and_artifact_are_replay_stable -- --exact
	$(TERLC_EXACT_TEST) commands::serve::config::config_test::effective_config_rejects_path_escape_and_zero_pressure_limits -- --exact
	$(CARGO) check -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin


# Verifies bounded, payload-free observability covers every serve domain with clean shutdown
vm-runtime-observability-check:
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_schema_spans_every_vm_serve_domain -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_is_bounded_and_reports_overflow_and_partial_failure -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::traceparent_validation_rejects_malformed_and_zero_identifiers -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_correlates_request_without_recording_payloads -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::shutdown_lifecycle_handles_repeated_signals_and_forced_deadline -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_flushes_replayable_events_metrics_and_traces -- --exact
	$(TERLC_EXACT_TEST) commands::serve::observability::observability_test::observability_rejects_zero_capacity -- --exact
	! rg -n 'dbg!|println!|eprintln!' crates/terlan/src/commands/serve/config.rs crates/terlan/src/commands/serve/observability.rs


# Runs the quality gate for web server lifecycle and health across reload and TLS
vm-web-lifecycle-health-check: \
	vm-web-observability-check \
	web-compose-check \
	http-tls-check \
	vm-source-hot-reload-check
	$(TERLAN_QUALITY) vm-web-lifecycle-health


# Runs the quality gate for the web deployment profile
vm-web-deployment-profile-check: \
	vm-web-lifecycle-health-check \
	http-router-check \
	http-tls-check \
	native-boundary-http-cookie-check
	$(TERLAN_QUALITY) vm-web-deployment-profile


# Runs the quality gate for route schema and generated client consistency
vm-web-route-schema-client-check: \
	vm-web-deployment-profile-check \
	api-schema-check \
	web-profile-preflight
	$(TERLAN_QUALITY) vm-web-route-schema-client


# Verifies the model-sync store against Postgres-backed persistence
vm-model-sync-store-check: \
	vm-web-route-schema-client-check \
	native-boundary-postgres-check \
	db-command-check
	$(TERLC) test std/vm/ModelSyncTest.terl
	$(TERLAN_QUALITY) vm-model-sync-store


# Verifies the persistent actor store across process, timer, resource, and transport layers
vm-persistent-actor-store-check: \
	vm-model-sync-store-check \
	vm-process-model-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check \
	vm-distributed-transport-check
	$(TERLC) check std/vm/PersistentActorTest.terl
	$(TERLAN_QUALITY) vm-persistent-actor-store


# Runs the quality gate for the persistent actor's storage schema
vm-persistent-actor-schema-check: \
	vm-persistent-actor-store-check \
	vm-distributed-state-check \
	vm-distributed-transport-check
	$(TERLAN_QUALITY) vm-persistent-actor-schema


# Runs the quality gate for persistent actor log and state compaction
vm-persistent-actor-compaction-check: \
	vm-persistent-actor-schema-check \
	vm-distributed-state-check \
	vm-resource-ownership-check
	$(TERLAN_QUALITY) vm-persistent-actor-compaction


# Runs the quality gate for restoring persistent actor state after restart
vm-persistent-actor-restore-check: \
	vm-persistent-actor-compaction-check \
	vm-distributed-state-check \
	vm-timer-primitives-check \
	vm-resource-ownership-check
	$(TERLAN_QUALITY) vm-persistent-actor-restore


# Runs the quality gate for persistent-actor storage adapter conformance
vm-persistent-actor-adapter-conformance-check: \
	vm-persistent-actor-restore-check \
	vm-distributed-state-check
	$(TERLAN_QUALITY) vm-persistent-actor-adapter


# Benchmarks persistent actor runtime performance against its budget
vm-persistent-actor-performance-budget-check: vm-persistent-actor-adapter-conformance-check
	TERLAN_BENCH_PERSISTENT_ACTOR_OUTPUT=target/quality/vm-persistent-actor-benchmark.json $(TERLAN_BENCHMARK) vm-persistent-actor-runtime-baseline
	$(TERLAN_QUALITY) vm-persistent-actor-performance


# Runs the quality gate for persistent actor telemetry
vm-persistent-actor-telemetry-check: vm-persistent-actor-performance-budget-check
	$(TERLAN_QUALITY) vm-persistent-actor-telemetry


# Runs the quality gate for persistent actor policy enforcement
vm-persistent-actor-policy-check: vm-persistent-actor-telemetry-check
	$(TERLAN_QUALITY) vm-persistent-actor-policy


# Umbrella check gating ACME TLS support on timer deadlines and base TLS support
vm-http-acme-tls-base-check: vm-timer-deadline-check http-tls-check


# Runs the quality gate for ACME worker migration during certificate renewal
vm-http-acme-worker-migration-check: vm-http-acme-tls-base-check
	$(TERLAN_QUALITY) vm-http-acme-worker


# Runs the quality gate for custody of the ACME certificate cache
vm-http-acme-cache-custody-check: vm-http-acme-worker-migration-check
	$(TERLAN_QUALITY) vm-http-acme-cache-custody


# Runs the quality gate for ACME certificate renewal and rotation
vm-http-acme-renewal-rotation-check: vm-http-acme-cache-custody-check
	$(TERLAN_QUALITY) vm-http-acme-renewal


# Validates manual-cert TLS, HTTP/2 ALPN, and that no stub TLS adapter remains
vm-http-acme-tls-production-check: vm-http-acme-renewal-rotation-check
	$(TERLC_EXACT_TEST) commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::runtime_tls_config_accepts_manual_certificate_tls -- --exact
	$(TERLC_EXACT_TEST) commands::serve::hyper_server::hyper_server_test::vm_owned_tls_serves_http2_selected_by_rustls_alpn -- --exact
	$(CARGO) check -p terlan --bin terlan-serve-runtime --no-default-features --features serve-runtime-bin
	! rg -n 'serve_tls\.adapter_missing|maintained async Hyper TLS adapter is required' crates/terlan/src/commands/serve


# Checks HTTP/2 protocol task execution and deadline/lifecycle cancellation handling
vm-http-protocol-readiness-check: vm-http-acme-tls-production-check
	$(RUST_TEST) -p terlan-http-native --lib http2::tests
	$(RUST_TEST) -p terlan --lib -- runtime::vm::protocol_task_executor::task_group::tests commands::serve::hyper_server::http2::tests
	$(TERLC_EXACT_TEST) runtime::vm::http::deadline_test::vm_http_tcp_server_deadline_cancels_parked_handler_and_closes_stream -- --exact
	$(TERLC_EXACT_TEST) runtime::vm::http::lifecycle_test::vm_http_tcp_server_drain_does_not_count_cancellation_as_completion -- --exact


# Aggregates the full VM HTTP stack check suite
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


# Tests VM HTTP TCP server concurrency: keep-alive, TLS, overload, and drain/shutdown
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


# Tests VM streaming serve dispatch and JS-target function-head diagnostics
vm-http-stream-serve-check:
	$(RUST_TEST) -p terlan --lib vm_stream_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::build_test::tests::js_target_diagnostics_test::build_command_rejects_function_head_pattern_for_js_target -- --exact


# Runs the in-memory VM HTTP transport test suite
vm-http-in-memory-transport-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::http::http_test


# Tests HTTP router middleware concurrency and web-handler route discovery
vm-http-router-middleware-check:
	$(EXACT_CARGO_TEST) -p terlan-http-native --lib routing::concurrency_test::router_middleware_bounded_concurrency_smoke -- --exact
	$(RUST_TEST) -p terlan --lib discover_web_handlers_rejects_ -- --quiet
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::asset_and_response_manifests::write_browser_package_preserves_response_handlers_as_source_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::response_source_execution::generated_response_routes_execute_shadowed_providers_over_http1 -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers_from_modules_extracts_grouped_router_builder_calls -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_prefers_dynamic_handler_over_file_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_request_prefers_dynamic_handler_over_static_response_fallback_without_hyper -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::dynamic_dispatch::vm_stream_request_activates_materialized_router_middleware_without_hyper -- --exact
	@$(TERLC) test std/http/RouterTest.terl


# Runs the VM server-sent-events test suite
vm-http-sse-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::sse::sse_test


# Tests WebSocket adapter frame construction and endpoint channel-limit validation
vm-http-websocket-source-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_adapter_frame_constructors_build_typed_frames -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_adapter_endpoint_validates_channel_limits -- --exact
	@$(TERLC) test std/http/WebSocketTest.terl


# Tests WebSocket upgrade handshake, route discovery, and session binding
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


# Tests WebSocket inbound queue ordering, bounds, and backpressure
vm-http-websocket-queue-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_endpoint_opens_bounded_inbound_queue -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_inbound_queue_preserves_order_and_pressure -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_inbound_queue_rejects_full_and_oversized_frames -- --exact


# Tests the WebSocket binary-payload rejection policy
vm-http-websocket-policy-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_endpoint_declares_binary_payload_rejection_policy -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::upgrade_and_endpoints::vm_websocket_decode_client_frame_rejects_binary_frame -- --exact


# Tests WebSocket upgrade responses over TLS transport
vm-http-websocket-tls-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_tls_upgrade_response_writes_to_peer -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::transport_upgrade::vm_websocket_runtime_send_tls_upgrade_response_rejects_stream_mismatch -- --exact


# Tests WebSocket termination on timeout and on cancellation
vm-http-websocket-termination-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::termination::vm_websocket_runtime_timeout_termination_sends_close_and_reason -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::websocket::websocket_test::termination::vm_websocket_runtime_cancelled_termination_cancels_stream_without_close_frame -- --exact


# Tests SSE/live-channel route discovery and handler-conflict validation
vm-http-live-channel-source-check:
	$(EXACT_CARGO_TEST) -p terlan --lib commands::build::js_browser::js_browser_test::route_fixtures::discover_web_route_manifest_extracts_grouped_sse_routes -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::observability_and_packages::package_validation_test::validate_web_package_rejects_sse_route_conflicting_with_http_handler -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::serve_test::route_dispatch::vm_stream_sse_route_activates_materialized_router_middleware -- --exact
	@$(TERLC) test std/http/RouterTest.terl
	@$(TERLC) test std/http/SseTest.terl
	@$(TERLC) test std/http/WebSocketTest.terl
	@$(TERLC) test std/http/LiveChannelTest.terl


# Tests in-memory VM stream framing fixtures and benchmarks framing throughput
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


# Aliases TCP framing checks to the in-memory stream framing suite
vm-tcp-framing-check: vm-in-memory-stream-check


# Tests static-asset tables, byte-range responses, and HTTP1 streaming backpressure
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


# Runs the VM HTTP request-handling test suite
vm-http-queue-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::http::http_test


# Tests the VM primitive registry and raw TCP accept/read/write/backpressure runtime
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


# Tests the VM process runtime and actor custom-name registry semantics
vm-process-model-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::process
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_send_test::actor_custom_name_registry_preserves_via_registration_routing_and_cleanup_semantics -- --exact


# Runs the VM scheduler contract test suite
vm-scheduler-contract-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler


# Tests actor directory ownership and scheduler requeue/cancellation semantics
vm-actor-mutator-ownership-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_runs_runnable_process_and_requeues_yielded_slice -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact


# Tests actor mailbox parity against the BEAM parallel-messages suite
vm-parallel-messages-suite-parity-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::actor_parallel_messages_beam_suite_parity_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test::actor_directory_accepts_cross_thread_producers_before_owned_integration -- --exact
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@rg -q 'VmActorDirectoryError::MailboxFull' crates/terlan/src/runtime/vm/actor_parallel_messages_beam_suite_parity_test.rs


# Tests native-worker efile and sandbox parity against the BEAM efile suite
vm-efile-suite-parity-check:
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::efile_beam_suite_parity_test::efile_suite_repeated_reads_release_descriptors_and_recover_worker_capacity -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::efile_beam_suite_parity_test::efile_suite_reads_zero_sized_proc_file_repeatedly_without_empty_results -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::sandbox::sandbox_test::sandbox_limit_attestation_is_exact -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::sandbox::sandbox_test::linux_sandbox_command_is_closed_and_bounded -- --exact
	@rg -q 'std\.io\.file\.read_text' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs crates/terlan/src/runtime/native_boundary/dispatch.rs
	@rg -q 'EFILE_REPETITIONS: usize = 10' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs
	@rg -q 'PROC_READ_REPETITIONS: usize = 500' crates/terlan/src/native_worker/efile_beam_suite_parity_test.rs


# Tests native float arithmetic lowering and overflow/division-by-zero parity
vm-float-native-arithmetic-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::float_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib runtime::native_image::managed::operation_abi::float::test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::typeck::core_intrinsic_test::vm_primitive_registry::syntax_output_lowering_to_core_maps_selected_float_alias_to_intrinsic -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::finite_float_patterns_and_equality_execute -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::non_finite_float_pattern_fails_closed -- --exact
	target/debug/terlc check std/core/FloatTest.terl
	@rg -q 'status::FLOAT_OVERFLOW' crates/terlan/src/compiler/native_ir/float_suite_native_parity_test.rs
	@rg -q 'status::FLOAT_DIVISION_BY_ZERO' crates/terlan/src/compiler/native_ir/float_suite_native_parity_test.rs


# Tests closure conversion and dispatch parity against the BEAM fun suite
vm-fun-suite-parity-check: tvm-aot-application-closure-check tvm-aot-owned-closure-representation-check tvm-aot-closure-dispatch-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::fun_suite_native_parity_test
	@rg -q 'function arity mismatch: expected 1 args, found 0' crates/terlan/src/compiler/native_ir/fun_suite_native_parity_test.rs
	@rg -q 'cannot unify Int with' crates/terlan/src/compiler/native_ir/fun_suite_native_parity_test.rs
	@rg -q 'MAX_OWNED_CLOSURE_CAPTURES' crates/terlan/src/compiler/native_ir/closure_conversion.rs
	@rg -q 'ManagedClosureImageGeneration' crates/terlan/src/runtime/native_image/managed/closures.rs


# Tests managed-heap GC, mailbox, and actor-alias semantics against the BEAM gc suite
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


# Tests case-guard lowering and purity checks against the BEAM guard suite
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


# Tests guard lowering parity with codegen optimizations disabled
vm-guard-no-opt-suite-parity-check: vm-guard-suite-parity-check
	$(RUST_TEST) -p terlan --lib compiler::native_ir::guard_no_opt_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::codegen_policy_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib compiler::native_ir::case_lowering_test::scalar_case_source_executes_through_linked_native_object -- --exact
	@rg -q 'Self::Development => "none"' crates/terlan/src/compiler/native_ir/codegen_policy.rs
	@rg -q 'Self::Serve \| Self::Release => "speed"' crates/terlan/src/compiler/native_ir/codegen_policy.rs
	@rg -q 'assert_eq!\(count_calls\(body, "next"\), 1\)' crates/terlan/src/compiler/native_ir/case_lowering_test.rs


# Tests deterministic hashing/bitstring/map parity against the BEAM hash suite
vm-hash-suite-parity-check: vm-deterministic-hashmap-check
	$(RUST_TEST) -p terlan --lib runtime::vm::value::hash_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::bitstring::bitstring_test::bitstring_canonicalizes_storage_and_discards_unrepresented_bytes -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::bitstring::bitstring_test::bitstring_slices_aligned_and_unaligned_ranges_in_network_order -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::map_value::map_value_test::achamp_indexed_map_uses_collision_node_for_equal_hashes -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::map_value::map_value_test::achamp_indexed_map_compresses_long_shared_hash_prefixes -- --exact
	@rg -q 'enum Task' crates/terlan/src/runtime/vm/value_hash.rs
	@rg -q 'FinishMap' crates/terlan/src/runtime/vm/value_hash.rs
	@! rg -q 'DefaultHasher|RandomState' crates/terlan/src/runtime/vm/value_hash.rs


# Aggregates core VM suite checks and runs the hello-world parity and smoke tests
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


# Tests native integer overflow/division-by-zero parity against the BEAM small suite
vm-small-suite-parity-check:
	$(RUST_TEST) -p terlan --lib compiler::native_ir::small_suite_native_parity_test
	$(RUST_TEST) -p terlan --lib compiler::native_ir::scalar_replacement_index_test
	@rg -q 'sadd_overflow' crates/terlan/src/compiler/native_ir/cranelift/expression.rs
	@rg -q 'ssub_overflow' crates/terlan/src/compiler/native_ir/cranelift/expression.rs
	@rg -q 'smul_overflow' crates/terlan/src/compiler/native_ir/cranelift/expression.rs
	@rg -q 'status::DIVISION_BY_ZERO' crates/terlan/src/compiler/native_ir/cranelift/expression.rs
	@rg -q 'status::OVERFLOW' crates/terlan/src/compiler/native_ir/small_suite_native_parity_test.rs


# Tests scheduler topology and smoke behavior against the BEAM smoke suite
vm-smoke-suite-parity-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_smoke_beam_suite_parity_test
	$(EXACT_CARGO_TEST) -p terlan --lib compiler::native_ir::case_lowering_test::dense_and_sparse_integer_dispatch_executes_through_linked_native_object -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	@rg -q 'cfg!\(target_has_atomic = "32"\)' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs
	@rg -q 'cfg!\(target_has_atomic = "64"\)' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs
	@rg -q 'VM_MAX_SCHEDULERS' crates/terlan/src/runtime/vm/scheduler_smoke_beam_suite_parity_test.rs


# Tests mailbox publication ordering and lost-wakeup prevention on multicore
vm-multicore-mailbox-publication-check: vm-process-model-check vm-failure-primitives-check
	$(RUST_TEST) -p terlan --lib runtime::vm::actor_directory::mailbox::mailbox_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor_directory::actor_directory_test::publication_during_execution_prevents_lost_wakeup_park -- --exact
	@rg -q 'ConcurrentQueue::bounded\(ACTOR_MAILBOX_CAPACITY\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	@! rg -q 'ConcurrentQueue::unbounded\(\)' crates/terlan/src/runtime/vm/actor_directory/mailbox.rs
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_signal_beam_suite_parity_test::signal_suite_contended_enqueue_inspection_and_single_wakeup_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_signal_beam_suite_parity_test::signal_suite_message_before_down_order_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::failure::failure_erl_link_parity_test::erl_link_suite_portable_link_monitor_race_contract -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::process::process_registry_test::process_registry_exit_removes_every_name_before_reuse -- --exact


# Tests fixed-scheduler placement, telemetry, and handler-cache shard ownership
vm-multicore-fixed-placement-check: vm-multicore-mailbox-publication-check vm-scheduler-fairness-check rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_telemetry::fixed_scheduler_telemetry_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::shard_owner::shard_owner_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test


# Tests AOT actor migration across scheduler owners during handler invocation
tvm-aot-multicore-migration-check: tvm-aot-runtime-transition-check tvm-managed-memory-check vm-actor-mutator-ownership-check rust-quality-check
	$(RUST_TEST) -p terlan --lib transfer_test
	$(RUST_TEST) -p terlan --lib runtime::vm::fixed_scheduler_control::fixed_scheduler_control_test
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact


# Runs the work-stealing policy test suite
vm-multicore-work-stealing-policy-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::work_stealing::work_stealing_test


# Tests work-stealing across scheduler owners for generated AOT actors
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


# Asserts obsolete multicore work-stealing types and targets were fully removed
vm-multicore-runtime-cleanup-check:
	@! rg -q 'VmWorkStealingRuntime|VmSchedulerStealClaim|VmActorStealClaim' crates/terlan/src
	@! rg -q '^(vm-multicore-work-stealing-owner-check|tvm-aot-multicore-yield-queue-check|tvm-aot-multicore-runnable-steal-check|tvm-aot-multicore-policy-coordination-check):' Makefile
	@! rg -q 'hidden MC-5|next MC-5|staged MC-5|Activated by the MC-6|Used when MC-6' crates/terlan/src/runtime/vm crates/terlan/src/commands/serve/handler_cache.rs crates/terlan/src/commands/serve/handler_cache


# Tests execution-shard epoch guards reject stale I/O completions across migrations
tvm-aot-multicore-io-epoch-check: vm-multicore-work-stealing-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::execution_shard_test::stale_io_completion_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::parked_generated_handler_migrates_one_hundred_times_then_resumes_once -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::persistent_shard_actors_resume_only_from_exact_typed_io_wake -- --exact


# Tests execution-shard epoch guards reject stale or foreign timer ticks
vm-multicore-timer-epoch-check: tvm-aot-multicore-io-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::current_timer_tick_delivers_once_through_shard_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::foreign_shard_timer_tick_fails_before_timer_mutation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::pure_native::execution_shard::timer_ingress_test::stale_timer_tick_cannot_cross_execution_shard_epoch -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_runtime_transfer_test::actor_runtime_transfer_moves_delayed_message_with_exact_timer_deadline -- --exact


# Tests scheduler-owned timer parking/cancellation for generated AOT actors
vm-multicore-timer-scheduler-check: vm-multicore-timer-epoch-check
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_parks_until_scheduler_owned_deadline -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::native_debug_session_admits_built_image_and_resolves_breakpoint -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::debug::debug_test::debug_native_image_json_report_is_stable -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_does_not_block_peer_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_aot_timer_is_cancelled_by_scheduler_shutdown -- --exact


# Tests protocol-task-executor origin checks and reactor completion ownership
vm-multicore-protocol-reactor-check: vm-multicore-timer-scheduler-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::protocol_task_executor::protocol_task_executor_test::protocol_completion_origin_rejects_foreign_and_ambient_threads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::handler_cache_generation_test::immediate_callback_executes_on_its_protocol_owner_without_rpc -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_completion_stays_on_protocol_owner -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_rejects_same_scheduler_foreign_connection -- --exact


# Tests the capability-worker pool's bounded admission and crash recovery
vm-multicore-capability-worker-check: vm-multicore-protocol-reactor-check tvm-aot-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_enforces_bounded_non_reentrant_admission -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_duplicate_completion -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_cancellation_releases_exact_request_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_replaces_crashed_slot_without_capacity_bypass -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_rejects_identity_and_capability_bypass -- --exact


# Tests capability completion publication ordering and dispatch-missing rejection
vm-multicore-capability-completion-check: vm-multicore-capability-worker-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_completes_an_already_parked_generated_request -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_suppresses_late_already_parked_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_capability_completion_is_published_before_owner_dispatch -- --exact
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner.rs crates/terlan/src/runtime/vm/fixed_scheduler_telemetry.rs
	@if rg -n 'capability_dispatch_missing' crates/terlan/src/commands/serve/handler_cache/shard_owner; then \
		echo 'error[multicore.capability]: generated capability suspensions must not be rejected'; \
		exit 1; \
	fi


# Tests the capability event pump's deterministic, bounded completion correlation
vm-multicore-capability-event-pump-check: vm-multicore-capability-completion-check
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_correlates_completion_with_fixed_owner_payload -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_returns_payload_on_backpressure_and_cancellation -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_drains_generation_payloads_on_worker_loss -- --exact
	@rg -q 'VmCapabilityWorkerEventPump' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs
	@if rg -n 'HashMap|mpsc::channel\(\)' crates/terlan/src/runtime/vm/capability_worker/event_pump.rs; then \
		echo 'error[multicore.capability_event_pump]: correlation must remain deterministic and bounded by worker credits'; \
		exit 1; \
	fi


# Tests the capability event pump end-to-end against the real native worker process
vm-multicore-capability-scheduler-check: vm-multicore-capability-event-pump-check | terlan-native-worker-bootstrap
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_event_pump_shutdown_returns_all_pending_payloads -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib native_worker::protocol::protocol_test::worker_admits_declared_filesystem_operation -- --exact
	TERLAN_NATIVE_WORKER=$(TERLAN_RUST_SUITE_NATIVE_WORKER) TERLAN_TEST_AOT_CAPABILITY_PUMP=1 TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX=1 $(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_test::generated_capability_event_pump_executes_real_worker_full_cycle -- --ignored --exact
	TERLAN_NATIVE_WORKER=$(TERLAN_RUST_SUITE_NATIVE_WORKER) $(EXACT_CARGO_TEST) -p terlan --lib commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_capability_worker_wakes_and_resumes_exact_actor -- --ignored --exact
	@rg -q 'GeneratedCapabilityDispatcher' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs
	@rg -q 'CapabilityCompletionPublished' crates/terlan/src/commands/serve/handler_cache/shard_owner/capability_dispatch.rs


# Tests the VM's EPMD-style node discovery, registration, and router publication
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


# Aggregates multicore capability, discovery, timer, and HTTP integration checks
vm-multicore-runtime-integration-check: vm-multicore-capability-scheduler-check vm-epmd-discovery-check vm-timer-deadline-check native-boundary-runtime-adversarial-check vm-http-concurrency-investigation-check rust-quality-check


# Tests multicore replay/telemetry evidence across migration, timers, and debugging
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


# Runs multicore performance benchmarks and records the performance baseline
vm-multicore-performance-check: rust-quality-check
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler_topology::scheduler_topology_test
	$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::multicore_performance_test -- --nocapture
	$(MAKE) vm-multicore-performance-record


# Runs the multicore width-matrix benchmark and validates the performance report
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


# Runs seeded multicore stress tests and validates the deadlock-watchdog report
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


# Runs the platform-matrix ThreadSanitizer self-test contract
vm-multicore-thread-sanitizer-contract-check: | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-tsan-self-test


# Runs multicore code under ThreadSanitizer, falling back to the memory-model gate
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


# Runs the platform-matrix multicore-release self-test contract
vm-multicore-release-contract-check: | $(TERLAN_TVM_CONTRACT_BOOTSTRAP)
	$(TERLAN_TVM_CONTRACT_CHECK) multicore-release-self-test


# Records multicore release closeout evidence and validates its schema
vm-multicore-release-record:
	$(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-record
	test -s target/quality/vm-multicore-release-closeout.json
	@rg -q '"schema": "terlan.vm-multicore-release-closeout.v4"' target/quality/vm-multicore-release-closeout.json
	@rg -q '"decision": "pass"' target/quality/vm-multicore-release-closeout.json
	@rg -q '"source_tree_sha256":' target/quality/vm-multicore-release-closeout.json


# Runs all multicore release-local gates then records the release closeout
vm-multicore-release-check: vm-multicore-release-contract-check
	$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)
	$(MAKE) vm-multicore-release-record

.PHONY: vm-multicore-publish-prerequisites


# Verifies built artifacts exist, then retries sealed release verification
vm-multicore-publish-check:
	test -x $(TERLAN_BOOTSTRAP_VM)
	test -s $(TERLAN_TVM_PLATFORM_MATRIX_IMAGE)
	@attempt=1; \
	while ! timeout 300s $(TERLAN_TVM_PLATFORM_MATRIX) multicore-release-verify; do \
		if test "$$attempt" -ge 3; then \
			echo 'error[vm.multicore.release]: sealed publication verification failed three times' >&2; \
			exit 1; \
		fi; \
		echo "warning[vm.multicore.release]: transient read-only verification failure; retrying ($$attempt/3)" >&2; \
		attempt=$$((attempt + 1)); \
	done


# Tests resource-table cleanup runs on process exit as a final health sanity check
vm-final-health-check:
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact

# Hosted coverage proves test outcomes, but cannot supply test side effects.
# Produce runtime reports together on every fresh publication runner.

.PHONY: vm-runtime-report-artifacts
vm-runtime-report-artifacts:
	$(POLICY_CARGO_TEST) --locked -p terlan --lib -- --exact \
		runtime::vm::memory::memory_test::limits_and_accounting::memory_accounting_writes_deterministic_pressure_report \
		runtime::vm::memory::memory_test::shared_ownership_and_soak::memory_accounting_soak_preserves_ownership_and_writes_benchmark_report \
		runtime::vm::scheduler::scheduler_test::scheduler_writes_fairness_report_with_starvation_evidence \
		runtime::vm::timer::timer_test::cancellation_and_events::timer_table_writes_deadline_report_from_runtime_events \
		runtime::native_boundary::worker::worker_test::worker_lifecycle_events_write_native_boundary_report
	test -s target/quality/vm-memory-pressure-report.json
	test -s target/quality/vm-memory-soak-report.json
	test -s target/quality/vm-scheduler-fairness-report.json
	test -s target/quality/vm-timer-deadline-report.json
	test -s target/quality/vm-native-boundary-report.json


# Verifies the memory-pressure and soak reports were produced
vm-memory-heap-pressure-check: vm-process-model-check vm-resource-ownership-check vm-runtime-report-artifacts
	test -f target/quality/vm-memory-pressure-report.json
	test -f target/quality/vm-memory-soak-report.json


# Tests scheduler/actor accounting charges only on success and writes fairness report
vm-scheduler-fairness-check: vm-memory-heap-pressure-check vm-scheduler-contract-check vm-runtime-report-artifacts
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
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_weighted_classes_preserve_order_and_bound_background_wait -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_rejects_silent_reclassification_of_queued_process -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test::scheduler_cancels_at_preemption_boundary_without_requeueing -- --exact
	$(RUST_TEST) -p terlan --lib runtime::vm::scheduler::scheduler_test
	test -f target/quality/vm-scheduler-fairness-report.json


# Runs the VM actor primitives test suite
vm-actor-primitives-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::actor


# Runs the VM failure and reference primitives test suites
vm-failure-primitives-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::failure
	$(RUST_TEST) -p terlan --lib runtime::vm::reference


# Tests supervisor restart policies, backoff, limits, and failure history
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


# Tests timer scheduling, interval coalescing, overflow, and receive-timeout wakeups
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


# Verifies the timer deadline report was produced
vm-timer-deadline-check: vm-runtime-report-artifacts
	test -f target/quality/vm-timer-deadline-report.json


# Tests resource-table registration, transfer, release, and owner-exit cleanup
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


# Tests VM table-store insert/lookup/delete, access control, and traversal order
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


# Tests code-server module lifecycle: publish, reload, release, and purge
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


# Tests source-level hot reload: compiling, publishing, and rolling back generations
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


# Tests the VM's TETF distribution envelope encoding/decoding and transport validation
vm-distribution-envelope-check:
	$(RUST_TEST) -p terlan --lib runtime::vm::term_format::term_format_runtime_test
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_coordination_builds_tetf_distribution_envelope_with_refs -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_distributed_transport_decodes_declared_atom_payload_before_acceptance -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::coordination::coordination_test::peer_protocol::vm_distributed_transport_rejects_corrupt_or_mismatched_tetf_without_advancing -- --exact


# Tests the distributed VM cluster transport via the ClusterTest script
vm-distributed-transport-check: binary-bitstring-processing-check vm-distribution-envelope-check
	$(TERLC) test std/vm/ClusterTest.terl


# Tests distributed scheduling and fault handling via scheduler/fault test scripts
vm-distributed-scheduling-check: vm-distributed-transport-check
	$(TERLC) check std/vm/SchedulerTest.terl
	$(TERLC) test std/vm/SchedulerTest.terl
	$(TERLC) check std/vm/FaultTest.terl
	$(TERLC) test std/vm/FaultTest.terl


# Tests distributed state and storage behavior via test scripts
vm-distributed-state-check: vm-distributed-scheduling-check
	$(TERLC) check std/vm/DistributedStateTest.terl
	$(TERLC) test std/vm/DistributedStateTest.terl
	$(TERLC) check std/vm/DistributedStorageTest.terl
	$(TERLC) test std/vm/DistributedStorageTest.terl


# Aggregates distributed-state and failure-primitives suites into one parity check
vm-distribution-suite-parity-check: vm-distributed-state-check vm-failure-primitives-check


# Verifies VM debug key compatibility via targeted Rust unit tests
vm-debug-key-compatibility-check:
	$(RUST_TEST) -p terlan --lib key_compatibility_ -- --nocapture


# Verifies Latin-1 source encoding policy via targeted Rust unit tests
vm-latin1-source-policy-check:
	$(RUST_TEST) -p terlan --lib latin1_source_ -- --nocapture


# Verifies retired compiler transforms stay removed via targeted Rust unit tests
vm-compiler-transform-retirement-check:
	$(RUST_TEST) -p terlan --lib compiler_transform_retirement_ -- --nocapture


# Ties source column ownership validation to the transform-retirement check
vm-source-column-ownership-check: \
	vm-compiler-transform-retirement-check


# Placeholder target; no verification steps defined yet
vm-source-provenance-artifact-check:


# Placeholder target; no verification steps defined yet
vm-call-dependency-artifact-check:


# Placeholder target; no verification steps defined yet
vm-executable-source-span-artifact-check:


# Aggregates VM runtime semantics sub-checks and runs targeted actor/timer/scheduler/codec tests
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
	$(EXACT_CARGO_TEST) -p terlan-encoding-native --lib base64::base64_test -- --nocapture
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::process::process_location_test::nested_call_frames_restore_continuations_in_lifo_order -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_test::actor_runtime_selective_receive_retries_after_matching_message_wakes_actor -- --exact


# Verifies VM diagnostics quality via the quality tool and targeted runtime diagnostic tests
vm-diagnostics-quality-check:
	$(RUST_TEST) -p terlan --lib --features validation-union vm_diagnostics_quality_test
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


# Runs the VM coordination Docker self-validation Terlan test
vm-coordination-docker-check:
	target/debug/terlc test scripts/self_validation/VmCoordinationDockerTest.terl


# Checks VM process hibernation parity against BEAM, including GC and scheduler contracts
vm-hibernate-suite-parity-check: vm-gc-suite-parity-check vm-scheduler-contract-check
	$(RUST_TEST) -p terlan --lib runtime::vm::actor::tests::actor_hibernate_beam_suite_parity_test
	@rg -q 'VmProcessState::Hibernated' crates/terlan/src/runtime/vm/process.rs crates/terlan/src/runtime/vm/process/parking.rs
	@rg -q 'hibernate_process' crates/terlan/src/runtime/vm/scheduler.rs
	@rg -q 'hibernate_owner' crates/terlan/src/runtime/native_image/managed/execution/hibernation.rs


# Verifies VM OTP abstractions are exposed correctly through the Terlan stdlib
vm-otp-abstractions-terlan-stdlib-check:
	$(RUST_TEST) -p terlan --lib --features validation-union vm_otp_abstractions_terlan_stdlib_test
	$(TERLAN_QUALITY) vm-otp-abstractions-terlan-stdlib
	$(TERLC) test tests/language/VmServiceActorTest.terl
	$(TERLC) test std/vm/TaskTest.terl


# Verifies VM value ownership classification rules
vm-ownership-classification-check:
	$(RUST_TEST) -p terlan --lib --features validation-union vm_ownership_classification_test
	$(TERLAN_QUALITY) vm-ownership-classification


# Verifies the VM runtime concept inventory is complete and current
vm-runtime-concept-inventory-check:
	$(RUST_TEST) -p terlan --lib --features validation-union vm_runtime_concept_inventory_test
	$(TERLAN_QUALITY) vm-runtime-concept-inventory


# Verifies the VM multicore invariant inventory is complete and current
vm-multicore-invariant-inventory-check:
	$(RUST_TEST) -p terlan --lib --features validation-union multicore_invariant_inventory_test
	$(TERLAN_QUALITY) vm-multicore-invariant-inventory


# Alias gating VM OTP corpus validation on the OTP reference inventory check
vm-otp-corpus-inventory-check: otp-reference-inventory-check

.PHONY: vm-native-boundary-contract-check
# Checks the VM native boundary report artifact exists after the proof track
vm-native-boundary-contract-check: lean-proof-track-check vm-runtime-report-artifacts
	test -f target/quality/vm-native-boundary-report.json

.PHONY: vm-sql-macro-validation-check
# Checks SQL macro validation and the SQL form boundary
vm-sql-macro-validation-check: vm-db-migration-command-check
	$(CURDIR)/target/debug/terlc test scripts/self_validation/SqlFormBoundaryTest.terl
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features validation-union sql
	$(TERLAN_QUALITY) vm-sql-macro-validation
	test -s target/quality/vm-sql-macro-validation-report.json

.PHONY: vm-db-migration-command-check
# Checks the VM database migration command against a live Postgres
vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::db::live_test::run_db_migration_and_snapshot_lifecycle_against_docker_postgres -- --ignored --exact --nocapture
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features validation-union vm_db_migration_command
	$(TERLAN_QUALITY) vm-db-migration-command
	test -s target/quality/vm-db-migration-report.json

.PHONY: vm-dev-dependency-orchestration-check
# Checks VM dev-dependency orchestration
vm-dev-dependency-orchestration-check:
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib commands::dev_dependencies
	env RUSTFLAGS='-D warnings' $(RUST_TEST) -p terlan --lib --features validation-union vm_dev_dependency_orchestration
	$(TERLAN_QUALITY) vm-dev-dependency-orchestration
	test -s target/quality/vm-dev-dependency-report.json

.PHONY: vm-postgres-runtime-check
# Checks the VM Postgres runtime report exists after the full Postgres check suite
vm-postgres-runtime-check: stdlib-postgres-server-check stdlib-postgres-worker-check vm-sql-macro-validation-check vm-native-boundary-contract-check no-default-tokio-runtime-check libpq-c-abi-check
	test -s target/quality/vm-postgres-runtime-report.json


# Benchmarks VM performance against its baseline
vm-performance-baseline-check: achamp-adversarial-coverage-check
	$(RUST_TEST) -p terlan --lib --features validation-union benchmark::database::tests
	$(TERLAN_BENCHMARK) vm-performance-baseline


# Checks VM HTTP performance against the paired Axum benchmark
vm-http-vs-axum-check: tvm-http-paired-performance-check
	TERLAN_VM_BENCHMARK_ROOT="$(CURDIR)" \
	TERLAN_VM_BENCHMARK_GATE=vm-http-vs-axum-check \
		target/debug/terlc test scripts/self_validation/VmBenchmarkFamilyPlanTest.terl
	TERLAN_PROTOCOL_BENCHMARK_MODE=validate \
	TERLAN_PROTOCOL_BENCHMARK_ANCHOR=binary_protocol_concurrency_benchmark \
		target/debug/terlc test scripts/benchmarks/protocol/ProtocolBenchmarkTest.terl \
			--bench --warmup 0 --samples 1 --name repository_protocol_benchmark_gate


# Investigates VM HTTP concurrency across erl, TCP, HTTP-lane, scheduler fairness
vm-http-concurrency-investigation-check: \
	terlan-vm-erl-suite-audit-check \
	vm-tcp-stream-check \
	terlan-vm-http-lane-check \
	vm-scheduler-fairness-check | terlan-tvm-platform-matrix-bootstrap
	$(TERLAN_TVM_PLATFORM_MATRIX) vm-http-aot-concurrency-self-test


# Checks VM HTTP benchmark results are comparable across runs
vm-http-benchmark-comparability-check: $(VM_HTTP_BENCHMARK_COMPARABILITY_DEPS)
	$(TERLAN_QUALITY) vm-http-benchmark-comparability


# Checks the VM HTTP soak-test stability report exists
vm-http-soak-stability-check: vm-http-benchmark-comparability-check vm-multicore-replay-observability-check vm-timer-deadline-check
	test -s $(HTTP_SOAK_REPORT)


# Checks VM semantics/performance against OTP via benchmark comparison
vm-semantics-vs-otp-check: binary-bitstring-processing-check
	TERLAN_VM_BENCHMARK_ROOT="$(CURDIR)" \
	TERLAN_VM_BENCHMARK_GATE=vm-semantics-vs-otp-check \
		target/debug/terlc test scripts/self_validation/VmBenchmarkFamilyPlanTest.terl
	TERLAN_PROTOCOL_BENCHMARK_MODE=validate \
	TERLAN_PROTOCOL_BENCHMARK_ANCHOR=binary_protocol_benchmark \
		target/debug/terlc test scripts/benchmarks/protocol/ProtocolBenchmarkTest.terl \
			--bench --warmup 0 --samples 1 --name repository_protocol_benchmark_gate


