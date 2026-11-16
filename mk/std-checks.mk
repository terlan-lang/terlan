# ---- Standard library checks ----

# Validates standard library source file naming conventions.
std-source-naming-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_source_naming_test
	$(TERLAN_QUALITY) std-source-naming


# Validates generated metadata for the standard library.
std-generated-metadata-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_generated_metadata_test
	$(TERLAN_QUALITY) std-generated-metadata


# Alias gating the std/VM parity matrix on the all-terlan-tests-vm check
std-vm-parity-matrix-check: all-terlan-tests-vm-check


# Verifies stdlib tests do not contain dishonest/no-op assertions
std-test-honesty-check:
	$(RUST_TEST) -p terlan --lib --features quality-tools std_test_honesty_test
	$(TERLAN_QUALITY) std-test-honesty


# Runs the std/test framework's table, assertions, and lifecycle suites
std-test-table-check:
	$(EXACT_CARGO_TEST) -p terlan --lib formal_pipeline::formal_pipeline_test::interface_foundations::embedded_std_interfaces_include_float_math_contract -- --exact
	$(RUST_TEST) -p terlan-encoding-native --lib
	target/debug/terlc check std/test/Test.terl
	target/debug/terlc test \
		std/test/AssertionsTest.terl \
		std/test/TableTest.terl \
		std/test/LifecycleTest.terl \
		std/core/IntTest.terl \
		std/core/FloatTest.terl \
		std/core/StringTest.terl \
		std/encoding/Base64Test.terl \
		std/encoding/Md5Test.terl


# Runs property-based tests across stdlib collections, core types, and encoding modules
std-test-property-check:
	target/debug/terlc test \
		std/collections/ListPropertyTest.terl \
		std/collections/MapPropertyTest.terl \
		std/collections/SetPropertyTest.terl \
		std/binary/BinaryPropertyTest.terl \
		std/core/AtomPropertyTest.terl \
		std/core/ErrorPropertyTest.terl \
		std/core/BoolPropertyTest.terl \
		std/core/FloatPropertyTest.terl \
		std/core/IntPropertyTest.terl \
		std/core/ObjectPropertyTest.terl \
		std/core/OptionPropertyTest.terl \
		std/core/OrderingPropertyTest.terl \
		std/core/ResultPropertyTest.terl \
		std/core/StringPropertyTest.terl \
		std/core/UnitPropertyTest.terl \
		std/data/JsonPropertyTest.terl \
		std/encoding/Base64PropertyTest.terl \
		std/io/PathPropertyTest.terl \
		std/net/UriPropertyTest.terl \
		std/range/RangePropertyTest.terl \
		std/random/RandomPropertyTest.terl \
		std/regex/RegexPropertyTest.terl \
		std/test/GenTest.terl \
		std/test/PropertyDistributionTest.terl \
		std/test/PropertyTest.terl \
		std/test/ShrinkTest.terl \
		std/test/StatefulPropertyTest.terl


# Runs the std/range test suite
std-range-check:
	target/debug/terlc test std/range


# Verifies the std/random module via native and Terlan-level tests
std-random-check:
	$(RUST_TEST) -p terlan --lib runtime::native::random::random_test
	target/debug/terlc test std/random


# Runs the std/regex test suite
std-regex-check:
	target/debug/terlc test std/regex


# Verifies 100% stdlib package coverage, gated on the shape-implications check
std-package-coverage-100-check: shape-implications-check
	$(RUST_TEST) -p terlan --lib --features quality-tools std_package_full_coverage_test
	$(TERLAN_QUALITY) std-package-coverage-100

.PHONY: stdlib-postgres-worker-check
# Checks the standard library Postgres source worker
stdlib-postgres-worker-check: | terlan-compiler-bootstrap
	bash scripts/check_postgres_source_worker.sh

.PHONY: stdlib-postgres-server-check
# Checks the standard library Postgres server resumes protocol and generated owners
stdlib-postgres-server-check: | terlan-compiler-bootstrap
	TERLAN_NATIVE_WORKER="$(CURDIR)/target/debug/terlan-native-worker" \
	TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES=1 TERLAN_TEST_AOT_CAPABILITY_PUMP=1 \
		$(RUST_TEST) -p terlan --lib commands::serve::handler_cache::protocol_capability::postgres_server_test::source_postgres_resumes_protocol_and_generated_owners -- --ignored --exact


