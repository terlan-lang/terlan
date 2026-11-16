# ---- GPU/tensor acceleration, ndarray, and native-binding checks ----
.PHONY: native-binding-consistency-check
# Verifies generated C/C++ bindings compile and match deterministic regeneration
native-binding-consistency-check:
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::fixtures_and_generation::structured_c_metadata_generates_real_ffi_package -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::cpp_binding_generator::generator::generator_test::fixtures_and_generation::structured_cpp_metadata_generates_real_cxx_package -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::pkg_config_distribution::checked_libpq_package_matches_deterministic_regeneration -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::c_abi_binding_generator::c_abi_binding_generator_test::ownership_adapters::integration::generated_c_adapter_compiles_and_enforces_public_protocol -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib commands::bind::cpp_binding_generator::generator::generator_test::fixtures_and_generation::generated_cxx_adapter_compiles_and_enforces_public_protocol -- --exact
	$(EXACT_CARGO_TEST) --locked -p terlan --lib runtime::vm::package_native_helper::transport_test


# Checks native boundary terminology stays consistent
native-boundary-terminology-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_boundary_terminology_test
	$(TERLAN_QUALITY) native-boundary-terminology


# Checks the native boundary security test suite
native-boundary-security-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_boundary_security_test
	$(TERLAN_QUALITY) native-boundary-security


# Checks the native binding generator's contract test suite
native-binding-generator-contract-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools native_binding_generator_contract_test
	$(TERLAN_QUALITY) native-binding-generator-contract

# Rust warnings are denied by rust-clippy-check. Keep these test requests in
# the canonical suite environment so coverage can be verified without replay.


# Checks the C++ binding generator and records the result in the external package matrix
cpp-binding-generator-check: native-binding-generator-contract-check cpp-binding-metadata-extractor-check cpp-package-consumer-check
	$(RUST_TEST) -p terlan --lib cpp_binding_generator
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cpp-binding-generator-check --state passed --reason-code none


# Checks the committed C++ metadata extractor fixture is valid
cpp-binding-metadata-extractor-check:
	target/debug/terlc test \
		scripts/self_validation/CppMetadataExtractorTest.terl \
		--name committed_cpp_metadata_contract_is_valid


# Checks live-extracted C++ metadata matches the committed fixture (opt-in)
cpp-binding-metadata-extractor-live-check: cpp-binding-metadata-extractor-check
	TERLAN_CPP_METADATA_LIVE=1 target/debug/terlc test \
		scripts/self_validation/CppMetadataExtractorTest.terl \
		--name opt_in_live_cpp_metadata_matches_fixture


# Checks the C++ binding build plan, gated on the binding generator check
cpp-binding-build-plan-check: cpp-binding-generator-check


# Checks C++ binding value records, gated on generator and metadata checks
cpp-binding-value-record-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check


# Checks C++ binding handling of copied containers, gated on the binding generator check
cpp-binding-copied-containers-check: cpp-binding-generator-check


# Checks C++ binding enum generation, gated on generator and metadata checks
cpp-binding-enum-check: cpp-binding-generator-check cpp-binding-metadata-extractor-check


# Checks C++ binding exception handling, gated on the binding generator check
cpp-binding-exception-check: cpp-binding-generator-check


# Checks a generated C++ package executes and rejects stale handles
cpp-package-consumer-check:
	$(RUST_TEST) -p terlan --lib generated_cpp_git_package_executes_and_rejects_stale_handles -- --ignored


# Checks the C ABI binding generator
c-abi-binding-generator-check: native-binding-generator-contract-check
	$(RUST_TEST) -p terlan --lib c_abi_binding_generator



terlan-ndarray-abi-check: export RUSTFLAGS := -D warnings
# Checks the ndarray package's C ABI, denying Rust warnings
terlan-ndarray-abi-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" abi-check \
		TERLC="$(abspath target/debug/terlc)"

.PHONY: terlan-ndarray-package-check
# Checks the ndarray package's packaging rules
terlan-ndarray-package-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" package-check \
		TERLC="$(abspath target/debug/terlc)"

.PHONY: terlan-ndarray-operations-check
# Checks the ndarray package's tensor operations
terlan-ndarray-operations-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" operations-check \
		TERLC="$(abspath target/debug/terlc)"


# Checks the ndarray package's BLAS integration
terlan-ndarray-blas-check: c-abi-binding-generator-check
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" blas-check \
		TERLC="$(abspath target/debug/terlc)"


# Checks ndarray DLPack interop for native tensor exchange
ndarray-dlpack-interop-check: c-abi-binding-generator-check
	$(RUST_TEST) -p terlan --lib native_exchange_ -- --nocapture
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" dlpack-check \
		TERLC="$(abspath target/debug/terlc)"


# Checks the ndarray package release build against polars and pytorch dependents
terlan-ndarray-release-check: c-abi-binding-generator-check
	$(RUST_TEST) -p terlan --lib native_exchange_ -- --nocapture
	@test -f "$(TERLAN_NDARRAY_DIR)/Makefile" || { \
		echo "error[terlan_ndarray_package_missing]: expected package checkout at $(TERLAN_NDARRAY_DIR)" >&2; \
		exit 1; \
	}
	$(MAKE) -C "$(TERLAN_NDARRAY_DIR)" release-check \
		TERLC="$(abspath target/debug/terlc)" \
		POLARS="$(abspath $(if $(TERLAN_POLARS_DIR),$(TERLAN_POLARS_DIR),../terlan-polars))" \
		PYTORCH="$(abspath $(if $(TERLAN_PYTORCH_DIR),$(TERLAN_PYTORCH_DIR),../terlan-pytorch))" \
		LIBTORCH="$(abspath $(if $(TERLAN_PYTORCH_LIBTORCH),$(TERLAN_PYTORCH_LIBTORCH),../terlan-pytorch/vendor/libtorch-2.13.0+cpu/libtorch))"

.PHONY: libpq-c-abi-check
# Checks the libpq C ABI binding and its native-codegen runtime integration
libpq-c-abi-check: c-abi-binding-generator-check
	$(CARGO) test -p terlan-libpq --all-targets --offline
	$(CARGO) test -p terlan --lib runtime::native::postgres::libpq::libpq_test --no-default-features --features native-codegen
	$(CARGO) check -p terlan --bin terlan-vm --no-default-features --features native-codegen


# Checks CUDA package availability detection
cuda-package-availability-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools cuda_package_availability::tests
	$(TERLAN_QUALITY) cuda-package-availability
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cuda-package-availability-check --state passed --reason-code none


# Checks the CUDA package contract against the configured CUDA toolchain
cuda-package-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools cuda_package_availability::tests
	$(TERLAN_QUALITY) cuda-package-check
	TERLAN_CUDA_PACKAGE_DIR="$(abspath $(TERLAN_CUDA_DIR))" \
	TERLAN_CUDA_TERLC="$(CURDIR)/target/debug/terlc" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/CudaPackageContractTest.terl
	$(TERLAN_EXTERNAL_PACKAGE_MATRIX) record \
		--gate cuda-package-check --state passed --reason-code none


# Checks the accelerator hard contract via repository validation
accelerator-hard-contract-check: | terlan-repository-validation-bootstrap
	TERLAN_REPOSITORY_ROOT="$(CURDIR)" \
		$(TERLAN_REPOSITORY_VALIDATION) accelerator-hard-contract


# Checks the accelerator boundary baseline against the frozen CUDA baseline
accelerator-boundary-baseline-check:
	$(MAKE) -C "$(TERLAN_CUDA_DIR)" cuda-baseline-freeze-check \
		TERLC="$(CURDIR)/target/debug/terlc"
	TERLAN_ACCELERATOR_PACKAGE_DIR="$(TERLAN_CUDA_DIR)" \
	TERLAN_ACCELERATOR_BOUNDARY_OUTPUT="$(CURDIR)/target/quality/accelerator-boundary-baseline.json" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/AcceleratorBoundaryBaselineTest.terl


# Checks accelerator package metadata generation
accelerator-package-metadata-check:
	$(RUST_TEST) -p terlan --lib accelerator
	TERLAN_ACCELERATOR_PACKAGE_DIR="$(TERLAN_CUDA_DIR)" \
	TERLAN_COMPILER="$(CURDIR)/target/debug/terlc" \
	TERLAN_ACCELERATOR_METADATA_OUTPUT="$(CURDIR)/target/quality/accelerator-package-metadata.json" \
		$(CURDIR)/target/debug/terlc test \
			scripts/self_validation/AcceleratorPackageMetadataTest.terl


# Checks generated AcceleratorValue code matches the committed CUDA package sources
accelerator-value-contract-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib accelerator
	target/debug/terlan-accelerator-value-contract \
		"$(CURDIR)/target/quality"
	$(CURDIR)/target/debug/terlc check \
		"$(CURDIR)/target/quality/generated/AcceleratorValue.terl"
	target/debug/terlan-accelerator-value-contract \
		"$(CURDIR)/target/quality/cuda-generated" \
		--module cuda.AcceleratorValue \
		--descriptor "$(TERLAN_CUDA_DIR)/accelerator.toml"
	cmp \
		"$(CURDIR)/target/quality/cuda-generated/cuda/AcceleratorValue.terl" \
		"$(TERLAN_CUDA_DIR)/src/cuda/AcceleratorValue.terl"
	cmp \
		"$(CURDIR)/target/quality/cuda-generated/generated/accelerator_value.rs" \
		"$(TERLAN_CUDA_DIR)/native/rust/src/generated/accelerator_value.rs"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorValueContractTest.terl


# Checks accelerator target admission against the CUDA package descriptor
accelerator-target-admission-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib accelerator
	target/debug/terlan-accelerator-target-admission \
		"$(CURDIR)/target/quality/accelerator-target-plan.json" \
		"$(TERLAN_CUDA_DIR)/accelerator.toml"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorTargetAdmissionTest.terl


# Checks the accelerator IR lowering
accelerator-ir-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::ir
	target/debug/terlan-accelerator-ir \
		"$(CURDIR)/target/quality/accelerator-ir-report.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorIrTest.terl


# Checks the accelerator ahead-of-time backend code generation via llc
accelerator-aot-backend-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::aot
	target/debug/terlan-accelerator-aot-backend \
		"$(CURDIR)/target/quality/accelerator-aot-backend.json" \
		"$${TERLAN_ACCELERATOR_LLC:-/usr/bin/llc}"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorAotBackendTest.terl


# Checks accelerator operation placement decisions
accelerator-placement-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::placement
	target/debug/terlan-accelerator-placement \
		"$(CURDIR)/target/quality/accelerator-placement-report.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorPlacementTest.terl


# Checks accelerator operation fusion, gated on the placement check
accelerator-fusion-check: accelerator-placement-check


# Checks accelerator data-transfer elision, gated on the placement check
accelerator-transfer-elision-check: accelerator-placement-check


# Checks accelerator integration with VM resource and capability-worker lifecycle
accelerator-vm-integration-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib runtime::vm::accelerator_operation
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::package_native_helper::package_native_helper_test::package_helper_projects_cuda_handles_into_canonical_vm_resources -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::actor::tests::actor_suspension_test::native_resource_transition_registers_owned_handle_and_cleans_up_on_exit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::capability_worker::pool::pool_test::capability_worker_pool_disposes_late_result_resources_with_bounded_credit -- --exact
	target/debug/terlan-accelerator-vm-integration \
		"$(CURDIR)/target/quality/accelerator-vm-integration.json"
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorVmIntegrationTest.terl


# Checks specialized artifacts correctly include/exclude accelerator code by target
accelerator-specialized-artifact-check: | terlan-quality-tools-bootstrap
	$(RUST_TEST) -p terlan --lib compiler::accelerator::assembly
	target/debug/terlan-accelerator-specialized-artifact \
		"$(CURDIR)/target/quality/accelerator-specialized-artifact.json"
	! strings "$(CURDIR)/target/quality/cpu-only.bin" | rg -i \
		'cuda-driver|terlan-cuda|vm-capability-worker-event-pump|\\.ptx|accelerator\\.execute'
	strings "$(CURDIR)/target/quality/accelerator-selected.bin" | rg \
		'cuda-driver|terlan-cuda|vm-capability-worker-event-pump|\\.ptx|accelerator\\.execute'
	$(CURDIR)/target/debug/terlc test \
		scripts/self_validation/AcceleratorSpecializedArtifactTest.terl


# Checks native boundary runtime/worker resilience against adversarial inputs
native-boundary-runtime-adversarial-check:
	$(RUST_TEST) -p terlan --lib runtime::native_boundary::capability_wire::capability_wire_test
	$(EXACT_CARGO_TEST) --locked -p terlan --lib native_worker::protocol::protocol_test::framing_rejects_oversized_input_and_output -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_disposed_handles_through_terms -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_duplicate_dispose_as_stale_handle -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_access -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_cross_process_resource_disposal -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_enforces_postgres_capability_before_adapter_execution -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::runtime::runtime_test::runtime_rejects_malformed_payload_with_typed_error -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_begin_request_rejects_duplicate_request_id -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_cancel_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_completion_wins_cancellation_race_without_request_id_reuse -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_lifecycle_events_write_native_boundary_report -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_lifecycle_event_history_is_bounded -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_timeout_request_releases_credit_and_rejects_late_reply -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::native_boundary::worker::worker_test::worker_duplicate_dispose_returns_stale_handle_and_releases_credit -- --exact
	$(EXACT_CARGO_TEST) -p terlan --lib runtime::vm::resource::resource_test::resource_table_cleans_up_owner_resources_on_process_exit -- --exact


# Benchmarks the native boundary Postgres baseline
native-boundary-postgres-baseline-benchmark:
	$(TERLAN_BENCHMARK) native-boundary-postgres-baseline


