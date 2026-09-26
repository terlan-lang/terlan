use super::*;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub(super) fn temp_dir(name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terlan_cxx_bind_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

pub(super) fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpp_native_boundary")
}

pub(super) fn fixture_manifest() -> PathBuf {
    fixture_dir().join("native-binding.json")
}

pub(super) fn extractor_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/cpp_metadata_extractor/fixtures")
}

pub(super) fn write_fixture_variant(name: &str, mutate: impl FnOnce(&mut Value)) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("create variant dir");
    fs::copy(
        fixture_dir().join("native_boundary.hpp"),
        root.join("native_boundary.hpp"),
    )
    .expect("copy header");
    fs::copy(
        fixture_dir().join("native_boundary.cc"),
        root.join("native_boundary.cc"),
    )
    .expect("copy source");
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(fixture_manifest()).expect("read fixture metadata"),
    )
    .expect("parse fixture metadata");
    mutate(&mut value);
    let manifest = root.join("native-binding.json");
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&value).expect("render variant"),
    )
    .expect("write variant");
    manifest
}

pub(super) fn symbol_mut<'a>(metadata: &'a mut Value, id: &str) -> &'a mut Value {
    metadata["cpp_metadata"]["symbols"]
        .as_array_mut()
        .expect("symbols")
        .iter_mut()
        .find(|symbol| symbol["id"] == id)
        .expect("fixture symbol")
}

pub(super) fn policy_mut<'a>(manifest: &'a mut Value, id: &str) -> &'a mut Value {
    manifest["mapping"]["symbols"]
        .as_array_mut()
        .expect("mapping symbols")
        .iter_mut()
        .find(|policy| policy["symbol"] == id)
        .expect("fixture policy")
}

pub(super) fn function_mut<'a>(manifest: &'a mut Value, name: &str) -> &'a mut Value {
    manifest["modules"]
        .as_array_mut()
        .expect("modules")
        .iter_mut()
        .flat_map(|module| {
            module["functions"]
                .as_array_mut()
                .expect("module functions")
        })
        .find(|function| function["name"] == name)
        .expect("fixture function")
}

pub(super) fn select_for_binding(manifest: &mut Value, id: &str) {
    let policy = policy_mut(manifest, id);
    policy["disposition"] = Value::String("bind".into());
    policy
        .as_object_mut()
        .expect("mapping policy")
        .remove("rejection");
}

#[test]
pub(super) fn generated_owned_resource_tuple_uses_a_cxx_carrier_and_handle_tuple_protocol() {
    let manifest = write_fixture_variant("owned_resource_tuple", |manifest| {
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.make_native_boundary_pair",
                "cpp_name": "make_native_boundary_pair",
                "source": {"path": "native_boundary.hpp", "line": 18, "column": 33},
                "kind": "function",
                "documentation": "Creates two owned native boundaries.",
                "annotations": ["TERLAN_BIND"],
                "overload_set": "terlan_fixture::make_native_boundary_pair",
                "returns": {
                    "spelling": "std::tuple<NativeBoundary, NativeBoundary>",
                    "canonical": "std::tuple<terlan_fixture::NativeBoundary, terlan_fixture::NativeBoundary>",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "parameters": [{
                    "name": "value",
                    "ty": {
                        "spelling": "std::int64_t",
                        "canonical": "std::int64_t",
                        "is_const": false,
                        "pointer_depth": 0,
                        "reference": "none",
                        "function_pointer": false,
                        "template_dependent": false
                    },
                    "direction": "input"
                }],
                "noexcept": true,
                "template_parameters": [],
                "overload_candidates": 1
            }));
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("policies")
            .push(serde_json::json!({
                "symbol": "function.make_native_boundary_pair",
                "disposition": "bind"
            }));
        let module = manifest["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["module"] == "cpp_fixture.NativeBoundary")
            .expect("native boundary module");
        module["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "pair",
                "operation": "cpp_fixture.native_boundary.pair",
                "cpp_symbol": "function.make_native_boundary_pair",
                "role": "free_function",
                "args": [{"name": "value", "ty": "Int"}],
                "returns": "{NativeBoundary, NativeBoundary}",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Creates two independently owned handles."
            }));
    });
    let out_dir = temp_dir("owned_resource_tuple_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate owned resource tuple");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("tuple adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("tuple adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");

    assert!(header.contains("class TerlanOwnedTupleCppFixtureNativeBoundaryPair final"));
    assert!(header.contains("std::unique_ptr<terlan_fixture::NativeBoundary> value_0_"));
    assert!(source.contains("auto result = terlan_fixture::make_native_boundary_pair(value);"));
    assert!(source.contains("std::get<0>(std::move(result))"));
    assert!(source.contains("std::get<1>(std::move(result))"));
    assert!(bridge.contains("type TerlanOwnedTupleCppFixtureNativeBoundaryPair;"));
    assert!(bridge.contains("terlan_owned_tuple_cpp_fixture_nativeboundary_pair_take_1"));
    assert!(helper.contains("ffi::terlan_owned_tuple_cpp_fixture_nativeboundary_pair("));
    assert!(helper.contains("ok_tuple_handles {}"));
    assert!(helper.contains("HandleValue::CppFixtureNativeBoundaryNativeBoundary(value_1)"));
}

#[test]
pub(super) fn structured_cpp_metadata_generates_real_cxx_package() {
    let out_dir = temp_dir("outputs");

    let summary =
        generate_cpp_bindings(&fixture_manifest(), &out_dir).expect("generate cxx package");

    assert_eq!(
        summary,
        CppBindingGenerationSummary {
            module_count: 2,
            function_count: 21,
            skipped_symbol_count: 11,
        }
    );
    for path in [
        "terlan.toml",
        "src/cpp_fixture/NativeBoundary.terl",
        "src/cpp_fixture/NativeGauge.terl",
        "tests/cpp_fixture/NativeBoundaryTest.terl",
        "tests/cpp_fixture/NativeGaugeTest.terl",
        "native/terlan-native.toml",
        "native/rust/Cargo.toml",
        "native/rust/build.rs",
        "native/rust/src/lib.rs",
        "native/rust/src/bin/native_boundary_helper.rs",
        "native/rust/include/native_boundary.hpp",
        "native/rust/include/terlan_enum_adapters.hpp",
        "native/rust/include/terlan_string_adapters.hpp",
        "native/rust/include/terlan_collection_adapters.hpp",
        "native/rust/include/terlan_exception_adapters.hpp",
        "native/rust/include/terlan_owned_value_adapters.hpp",
        "native/rust/include/terlan_mutation_adapters.hpp",
        "native/rust/cpp/native_boundary.cc",
        "native/rust/cpp/terlan_enum_adapters.cc",
        "native/rust/cpp/terlan_string_adapters.cc",
        "native/rust/cpp/terlan_collection_adapters.cc",
        "native/rust/cpp/terlan_exception_adapters.cc",
        "native/rust/cpp/terlan_owned_value_adapters.cc",
        "native/rust/cpp/terlan_mutation_adapters.cc",
        "bindings/native-binding-manifest.json",
        "bindings/skipped-symbols.json",
    ] {
        assert!(out_dir.join(path).is_file(), "missing generated {path}");
    }
    let package_manifest = fs::read_to_string(out_dir.join("terlan.toml")).expect("terlan.toml");
    assert!(package_manifest.contains("namespace = \"cpp_fixture\""));
    assert!(package_manifest.contains("artifact = \"library\""));
    assert!(package_manifest.contains("[native.rust]"));
    assert!(package_manifest.contains("path = \"native/rust\""));
    assert!(package_manifest.contains("helper = \"native-boundary-helper\""));
    assert!(package_manifest
        .contains("helper_env = \"TERLAN_CPP_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH\""));

    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
    assert!(bridge.contains("#[cxx::bridge(namespace = \"terlan_fixture\")]"));
    assert!(bridge.contains("type NativeBoundary;"));
    assert!(bridge.contains("type NativeGauge;"));
    assert!(!bridge.contains("fn make_native_boundary("));
    assert!(bridge.contains(
        "fn terlan_owned_value_cpp_fixture_nativeboundary_new(value: &[i64]) -> UniquePtr<NativeBoundary>;"
    ));
    assert!(bridge.contains(
        "fn terlan_owned_value_cpp_fixture_nativeboundary_shifted(value: &NativeBoundary, delta: i64) -> UniquePtr<NativeBoundary>;"
    ));
    assert!(bridge.contains("fn make_native_gauge(value: i64) -> UniquePtr<NativeGauge>;"));
    assert!(bridge.contains("fn sum_snapshot_fields(value: i64, doubled: i64) -> i64;"));
    assert!(bridge.contains("fn sum_integer_list(values: &[i64]) -> i64;"));
    assert!(bridge.contains("fn sum_float_list(values: &[f64]) -> f64;"));
    assert!(bridge.contains("type NativeSnapshot;"));
    assert!(bridge.contains("fn make_native_snapshot(value: i64) -> UniquePtr<NativeSnapshot>;"));
    assert!(bridge.contains("fn projected_value(self: &NativeSnapshot) -> i64;"));
    assert!(bridge.contains("fn add(self: Pin<&mut NativeBoundary>, delta: i64);"));
    assert!(bridge.contains("fn doubled(self: &NativeBoundary) -> i64;"));
    assert!(bridge.contains("fn label(self: &NativeBoundary) -> UniquePtr<CxxString>;"));
    assert!(bridge.contains("fn bytes(self: &NativeBoundary) -> UniquePtr<CxxVector<u8>>;"));
    assert!(bridge.contains("fn samples(self: &NativeBoundary) -> UniquePtr<CxxVector<i64>>;"));
    assert!(bridge.contains(
        "fn terlan_enum_cpp_fixture_nativeboundary_mode(value: &NativeBoundary) -> UniquePtr<CxxString>;"
    ));
    assert!(!bridge.contains("BoundaryMode::"));
    assert!(bridge.contains("type TerlanExceptionEnvelope;"));
    assert!(bridge.contains(
        "fn terlan_exception_cpp_fixture_nativeboundary_tripled_or_error(value: &NativeBoundary) -> UniquePtr<TerlanExceptionEnvelope>;"
    ));
    assert!(!bridge.contains("fn tripled_or_throw("));
    assert!(bridge.contains("fn increment(self: Pin<&mut NativeGauge>, delta: i64);"));

    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("CppFixtureNativeBoundaryNativeBoundary("));
    assert!(helper.contains("CppFixtureNativeGaugeNativeGauge("));
    assert!(helper.contains("cpp_fixture.native_boundary.new"));
    assert!(helper.contains("cpp_fixture.native_gauge.new"));
    assert!(helper.contains("cpp_fixture.native_boundary.snapshot"));
    assert!(helper.contains("cpp_fixture.native_boundary.sum_snapshot"));
    assert!(helper.contains("cpp_fixture.native_boundary.sum_integers"));
    assert!(helper.contains("cpp_fixture.native_boundary.sum_floats"));
    assert!(helper.contains("cpp_fixture.native_boundary.owned_snapshot"));
    assert!(helper.contains("owned value projection returned null"));
    assert!(helper.contains("Arg::Record(arg_0)"));
    assert!(helper.contains("arg_0.int(\"NativeSnapshot\", \"value\")"));
    assert!(helper.contains("ok_record"));
    assert!(helper.contains("ok_string"));
    assert!(helper.contains("ok_bytes"));
    assert!(helper.contains("ok_ints"));
    assert!(helper.contains("ok_atom"));
    assert!(!helper.contains(" 41 "));
    assert!(helper.contains("result_ok_int"));
    assert!(helper.contains("result_err"));
    assert!(helper.contains("getrandom::fill(&mut owner)"));
    assert!(helper.contains("cross_owner_handle"));
    assert!(!helper.contains("sensitive upstream exception payload"));
    assert!(helper.contains(
        "the generated wire decoder stays protocol-complete across package-specific operation subsets"
    ));
    assert!(helper.contains(
        "copied-record accessors stay protocol-complete across package-specific field subsets"
    ));

    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("cxx = \"=1.0.197\""));
    assert!(cargo.contains("getrandom = \"=0.3.4\""));
    assert!(cargo.contains("cxx-build = \"=1.0.197\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("cxx_build::bridge(root.join(\"src/lib.rs\"))"));
    assert!(build.contains("root.join(\"cpp/native_boundary.cc\")"));
    assert!(build.contains("build.std(\"c++14\")"));
    assert!(build.contains("build.include(root.join(\"include\"))"));
    assert!(build.contains("build.define(\"TERLAN_CPP_FIXTURE\", Some(\"1\"))"));
    assert!(build.contains("build.file(root.join(\"cpp/terlan_enum_adapters.cc\"))"));
    assert!(build.contains("build.file(root.join(\"cpp/terlan_exception_adapters.cc\"))"));
    assert!(build.contains("build.file(root.join(\"cpp/terlan_owned_value_adapters.cc\"))"));

    let owned_value_adapter =
        fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
            .expect("owned value adapter");
    assert!(owned_value_adapter.contains(
        "std::make_unique<terlan_fixture::NativeBoundary>(make_native_boundary(IntArrayRef(value.data(), value.size())))"
    ));
    assert!(!owned_value_adapter.contains("offset"));
    assert!(owned_value_adapter.contains("catch (...)"));
    assert!(owned_value_adapter
        .contains("std::make_unique<terlan_fixture::NativeBoundary>(value.shifted(delta))"));
    assert!(!owned_value_adapter.contains("std::unique_ptr<NativeBoundary> make_native_boundary"));

    let enum_adapter = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_enum_adapters.cc"))
        .expect("enum adapter");
    assert!(enum_adapter.contains("BoundaryMode::Raw"));
    assert!(enum_adapter.contains("BoundaryMode::Doubled"));
    assert!(enum_adapter.contains("BoundaryMode::Offset"));
    assert!(!enum_adapter.contains("BoundaryMode::Hidden"));
    assert!(!enum_adapter.contains(" = 7"));
    assert!(!enum_adapter.contains(" = 41"));
    assert!(!enum_adapter.contains(" = 99"));

    let exception_adapter =
        fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
            .expect("exception adapter");
    assert!(exception_adapter.contains("catch (...)"));
    assert!(exception_adapter.contains("boundary_operation_failed"));
    assert!(exception_adapter.contains("Native boundary operation failed."));
    assert!(!exception_adapter.contains("sensitive upstream exception payload"));

    let boundary_metadata =
        fs::read_to_string(out_dir.join("native/terlan-native.toml")).expect("metadata");
    assert!(boundary_metadata.contains("target = \"x86_64-unknown-linux-gnu\""));
    assert!(boundary_metadata.contains("language_standard = \"c++14\""));
    assert!(boundary_metadata.contains("mapping_schema = \"terlan.cpp.mapping.v1\""));
    assert!(boundary_metadata.contains("handle_scope = \"worker_random_256\""));
    assert!(boundary_metadata.contains("cross_owner = \"reject\""));
    for field in [
        "[public_adapter]",
        "adapter_abi_version = 1",
        "calling_convention = \"system_v\"",
        "execution_context = \"explicit\"",
        "capability_lifetimes = \"explicit\"",
        "resource_lifetimes = \"execution_context_scoped\"",
        "max_frame_bytes = 1048576",
        "max_transfer_bytes = 16777216",
        "status_model = \"status_values\"",
        "callback_reentrancy = \"forbidden\"",
        "async_completion = \"single_shot\"",
    ] {
        assert!(boundary_metadata.contains(field), "missing `{field}`");
    }
    assert!(helper.contains("const MAX_ADAPTER_FRAME_BYTES: usize = 1048576"));
    assert!(helper.contains("const MAX_ADAPTER_TRANSFER_BYTES: usize = 16777216"));
    assert!(helper.contains("take((MAX_ADAPTER_FRAME_BYTES + 1) as u64)"));
    assert!(helper.contains("struct InboundTransfer"));
    assert!(helper.contains("\"reply_chunk {request_id} {index} {final_chunk}"));
    assert!(helper.contains("\"transfer_too_large\""));
    assert!(helper.contains("last_request_id"));
    assert!(helper.contains("request_not_monotonic"));

    let normalized = fs::read_to_string(out_dir.join("bindings/native-binding-manifest.json"))
        .expect("normalized binding manifest");
    assert!(normalized.contains("\"canonical\": \"std::int64_t\""));
    assert!(normalized.contains("\"direction\": \"input\""));
    assert!(normalized.contains("\"overload_set\": \"terlan_fixture::make_native_boundary\""));
    assert!(normalized.contains("\"-DTERLAN_CPP_FIXTURE=1\""));

    let source = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("Terlan module");
    assert!(source.contains("pub opaque type NativeBoundary."));
    assert!(source.contains("pub struct NativeSnapshot {"));
    assert!(source.contains("value: Int,"));
    assert!(source.contains("doubled: Int"));
    assert!(source.contains("pub native_snapshot(value: Int, doubled: Int): NativeSnapshot ->"));
    assert!(source.contains("NativeSnapshot {value: value, doubled: doubled}."));
    assert!(source.contains("pub sum_snapshot(snapshot: NativeSnapshot): Int -> native."));
    assert!(source.contains("pub sum_integers(values: List[Int]): Int -> native."));
    assert!(source.contains("pub sum_floats(values: List[Float]): Float -> native."));
    assert!(source.contains("pub label(boundary: NativeBoundary): String -> native."));
    assert!(source.contains("pub bytes(boundary: NativeBoundary): std.vm.Bytes.Bytes -> native."));
    assert!(source.contains("pub samples(boundary: NativeBoundary): List[Int] -> native."));
    assert!(source.contains("pub type Raw = Atom[\"raw\"]."));
    assert!(source.contains("pub type Doubled = Atom[\"doubled\"]."));
    assert!(source.contains("pub type Offset = Atom[\"offset\"]."));
    assert!(source
        .contains("pub type BoundaryMode = Atom[\"raw\"] | Atom[\"doubled\"] | Atom[\"offset\"]."));
    assert!(source.contains("pub mode(boundary: NativeBoundary): BoundaryMode -> native."));
    assert!(!source.contains("41"));
    assert!(!source.contains("Hidden"));
    assert!(!source.contains("123"));
    assert!(source.contains(
        "pub tripled_or_error(boundary: NativeBoundary): Result[Int, std.core.Error.Error] -> native."
    ));
    assert!(source.contains("@compiler.native {cpp_fixture.native_boundary.new}"));
    assert!(!source.contains("std.native"));
    assert!(!source.contains("torch"));
    let generated_test =
        fs::read_to_string(out_dir.join("tests/cpp_fixture/NativeBoundaryTest.terl"))
            .expect("generated Terlan test");
    assert!(generated_test.contains("let boundary = new([40]);"));
    assert!(generated_test
        .contains("import cpp_fixture.NativeBoundary.{Doubled, NativeSnapshot, Offset, Raw,"));
    assert!(generated_test
        .contains("copied_mode == Raw or copied_mode == Doubled or copied_mode == Offset"));
    assert!(!generated_test.contains("Atom[\"raw\"]"));
    assert!(
        generated_test.contains("let _contained_tripled_or_error = tripled_or_error(boundary);")
    );
    let gauge_source = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeGauge.terl"))
        .expect("second Terlan module");
    assert!(gauge_source.contains("pub opaque type NativeGauge."));
    assert!(gauge_source.contains("@compiler.native {cpp_fixture.native_gauge.new}"));
    let gauge_test = fs::read_to_string(out_dir.join("tests/cpp_fixture/NativeGaugeTest.terl"))
        .expect("second generated Terlan test");
    assert!(gauge_test.contains("let boundary = new(40);"));
    assert!(gauge_test.contains("increment(boundary, 2);"));
    assert!(gauge_test.contains("observed == 42 and live_count() == 0."));
    assert!(!gauge_test.contains("Bool -> true"));

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
}

#[test]
pub(super) fn generated_module_imports_sibling_owned_types() {
    let manifest = write_fixture_variant("sibling_type_import", |manifest| {
        manifest["modules"][1]["type_imports"] =
            serde_json::json!(["cpp_fixture.NativeBoundary.NativeBoundary"]);
    });
    let out_dir = temp_dir("sibling_type_import_out");

    generate_cpp_bindings(&manifest, &out_dir).expect("generate sibling type import");

    let source = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeGauge.terl"))
        .expect("generated NativeGauge module");
    assert!(source.contains(
        "module cpp_fixture.NativeGauge.\n\nimport type cpp_fixture.NativeBoundary.NativeBoundary.\n\n"
    ));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_module_rejects_unresolved_or_duplicate_type_imports() {
    for (name, imports, expected) in [
        (
            "unresolved_type_import",
            serde_json::json!(["cpp_fixture.Missing.NativeBoundary"]),
            "does not resolve to a sibling module type",
        ),
        (
            "duplicate_type_import",
            serde_json::json!([
                "cpp_fixture.NativeBoundary.NativeBoundary",
                "cpp_fixture.NativeBoundary.NativeBoundary"
            ]),
            "duplicate generated type import",
        ),
        (
            "self_type_import",
            serde_json::json!(["cpp_fixture.NativeGauge.NativeGauge"]),
            "cannot import its own generated type",
        ),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            manifest["modules"][1]["type_imports"] = imports;
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("invalid generated type import must fail");
        assert!(error.contains(expected), "unexpected error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn generated_functions_preserve_validated_public_defaults() {
    let manifest = write_fixture_variant("public_default", |manifest| {
        function_mut(manifest, "shifted")["args"][1]["default"] = Value::String("1".into());
    });
    let out_dir = temp_dir("public_default_out");

    generate_cpp_bindings(&manifest, &out_dir).expect("generate public default");

    let source = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("generated NativeBoundary module");
    assert!(source.contains(
        "pub shifted(boundary: NativeBoundary, delta: Int = 1): NativeBoundary -> native."
    ));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_functions_reject_unsafe_or_nontrailing_public_defaults() {
    let invalid = write_fixture_variant("invalid_public_default", |manifest| {
        function_mut(manifest, "shifted")["args"][1]["default"] = Value::String("0; panic".into());
    });
    let error = generate_cpp_bindings(&invalid, &temp_dir("invalid_public_default_out"))
        .expect_err("unsafe public default must fail");
    assert!(
        error.contains("has invalid default"),
        "unexpected error: {error}"
    );
    fs::remove_dir_all(invalid.parent().expect("variant root")).expect("remove variant");

    let nontrailing = write_fixture_variant("nontrailing_public_default", |manifest| {
        let function = function_mut(manifest, "shifted");
        function["args"][1]["default"] = Value::String("1".into());
        function["args"]
            .as_array_mut()
            .expect("arguments")
            .push(serde_json::json!({"name": "later", "ty": "Int"}));
    });
    let error = generate_cpp_bindings(&nontrailing, &temp_dir("nontrailing_public_default_out"))
        .expect_err("required argument after default must fail");
    assert!(
        error.contains("has required argument `later` after a defaulted argument"),
        "unexpected error: {error}"
    );
    fs::remove_dir_all(nontrailing.parent().expect("variant root")).expect("remove variant");
}

#[test]
pub(super) fn generated_terlan_composition_keeps_native_leaves_private() {
    let manifest = write_fixture_variant("terlan_composition", |manifest| {
        let module = manifest["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["module"] == "cpp_fixture.NativeBoundary")
            .expect("native boundary module");
        let functions = module["functions"].as_array_mut().expect("functions");
        functions.push(serde_json::json!({
            "name": "shifted_private",
            "operation": "cpp_fixture.native_boundary.shifted_private",
            "cpp_symbol": "method.native_boundary.shifted",
            "visibility": "private",
            "role": "immutable_method",
            "args": [
                {"name": "boundary", "ty": "NativeBoundary"},
                {"name": "delta", "ty": "Int"}
            ],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "opaque_handle",
            "documentation": "Private exact CXX leaf."
        }));
        functions.push(serde_json::json!({
            "name": "shifted_twice",
            "operation": "cpp_fixture.native_boundary.shifted_twice",
            "terlan_body": "shifted_private(boundary, delta + delta)",
            "role": "immutable_method",
            "args": [
                {"name": "boundary", "ty": "NativeBoundary"},
                {"name": "delta", "ty": "Int"}
            ],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "opaque_handle",
            "documentation": "Public Terlan composition over one generated CXX leaf."
        }));
    });
    let out = temp_dir("terlan_composition_out");
    generate_cpp_bindings(&manifest, &out).expect("generate Terlan composition");
    let module = fs::read_to_string(out.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("generated module");
    let helper = fs::read_to_string(out.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("generated helper");
    let metadata =
        fs::read_to_string(out.join("native/terlan-native.toml")).expect("native metadata");

    assert!(module.contains(
        "@compiler.native {cpp_fixture.native_boundary.shifted_private}\nshifted_private("
    ));
    assert!(!module.contains("pub shifted_private("));
    assert!(module.contains(
        "pub shifted_twice(boundary: NativeBoundary, delta: Int): NativeBoundary ->\n    shifted_private(boundary, delta + delta)."
    ));
    assert!(helper.contains("cpp_fixture.native_boundary.shifted_private"));
    assert!(!helper.contains("cpp_fixture.native_boundary.shifted_twice"));
    assert!(metadata.contains("cpp_fixture.NativeBoundary.shifted_private"));
    assert!(!metadata.contains("cpp_fixture.NativeBoundary.shifted_twice"));
}

#[test]
pub(super) fn generated_terlan_composition_rejects_ambiguous_native_metadata() {
    let cases = [
        (
            "symbol",
            serde_json::json!("function.live_native_boundary_count"),
            None,
        ),
        ("empty", Value::Null, Some("")),
        ("terminator", Value::Null, Some("live_count().")),
    ];
    for (name, symbol, body) in cases {
        let manifest = write_fixture_variant(&format!("terlan_composition_{name}"), |manifest| {
            let function = function_mut(manifest, "live_count");
            function["cpp_symbol"] = symbol;
            function["terlan_body"] = serde_json::json!(body.unwrap_or("live_count()"));
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir("invalid_terlan_composition"))
            .expect_err("reject invalid Terlan composition");
        assert!(
            error.contains("error[cpp.terlan_body."),
            "unexpected {name} error: {error}"
        );
    }
}

#[test]
pub(super) fn generated_cpp_function_family_expands_exact_metadata_symbols() {
    let manifest = write_fixture_variant("cpp_function_family", |manifest| {
        let policies = manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols");
        policies.retain(|policy| policy["symbol"] != "method.native_boundary.shifted");
        manifest["mapping"]["symbol_families"] = serde_json::json!([{
            "symbols": ["method.native_boundary.shifted"],
            "disposition": "bind"
        }]);
        let module = &mut manifest["modules"][0];
        let functions = module["functions"].as_array_mut().expect("functions");
        functions.retain(|function| function["name"] != "shifted");
        module["function_families"] = serde_json::json!([{
            "operation_prefix": "cpp_fixture.native_boundary",
            "visibility": "private",
            "role": "immutable_method",
            "args": [
                {"name": "boundary", "ty": "NativeBoundary"},
                {"name": "delta", "ty": "Int"}
            ],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "owned_handle",
            "members": [
                {"cpp_symbol": "method.native_boundary.shifted"}
            ]
        }]);
    });
    let out_dir = temp_dir("cpp_function_family_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("expand C++ function family");
    let module = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("read generated family module");
    let normalized = fs::read_to_string(out_dir.join("bindings/native-binding-manifest.json"))
        .expect("read normalized expanded manifest");
    assert!(module.contains("@compiler.native {cpp_fixture.native_boundary.shifted}"));
    assert!(module.contains("shifted(boundary: NativeBoundary, delta: Int): NativeBoundary"));
    assert!(!module.contains("pub shifted("));
    assert!(normalized.contains("\"cpp_symbol\": \"method.native_boundary.shifted\""));
    assert!(normalized.contains("\"visibility\": \"private\""));
    assert!(!normalized.contains("function_families"));
    assert!(!normalized.contains("symbol_families"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn committed_clang_metadata_is_consumed_offline_with_unsafe_facts_visible() {
    let fixture = extractor_fixture_dir();
    let text = fs::read_to_string(fixture.join("expected-metadata.json"))
        .expect("read committed extractor result");
    let metadata: CppMetadata =
        serde_json::from_str(&text).expect("consume normalized Clang metadata");

    assert_eq!(metadata.schema, CPP_METADATA_SCHEMA);
    assert_eq!(metadata.producer.name, "clang-libtooling");
    validate_compile_configuration(&metadata.compile, &fixture).expect("compile provenance");
    for symbol in &metadata.symbols {
        validate_cpp_symbol(symbol).expect("normalized declaration facts");
    }

    assert!(metadata.symbols.iter().any(|symbol| {
        symbol
            .parameters
            .iter()
            .any(|parameter| parameter.ty.pointer_depth > 0)
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol.kind == CppSymbolKind::Enum
            && symbol.cpp_name == "CounterMode"
            && symbol
                .enum_values
                .iter()
                .any(|value| value.name == "Doubled" && value.value == "41")
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol
            .returns
            .as_ref()
            .is_some_and(|returns| returns.reference != CppReferenceKind::None)
    }));
    assert!(metadata
        .symbols
        .iter()
        .any(|symbol| !symbol.template_parameters.is_empty()));
    assert!(metadata
        .symbols
        .iter()
        .any(|symbol| symbol.overload_candidates > 1));
    assert!(metadata.symbols.iter().any(|symbol| {
        !matches!(symbol.kind, CppSymbolKind::Record | CppSymbolKind::Enum) && !symbol.noexcept
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol.parameters.iter().any(|parameter| {
            parameter.direction == CppParameterDirection::Output && parameter.ty.pointer_depth == 1
        })
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol
            .fields
            .iter()
            .any(|field| field.name == "value_" && field.ty.canonical == "long")
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol
            .parameters
            .iter()
            .any(|parameter| parameter.ty.function_pointer)
    }));
    assert!(metadata.symbols.iter().any(|symbol| {
        symbol.direct_calls.iter().any(|call| {
            call.id == "method:extractor_fixture::Counter::value()"
                && call.overload_set == "extractor_fixture::Counter::value"
                && call.kind == CppSymbolKind::Method
        })
    }));
}

/// Compiles one generated C++ adapter and exercises its bounded public protocol.
pub(super) fn compile_and_exercise_generated_cpp_adapter(
    out_dir: &Path,
    target_dir: &Path,
) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = out_dir.join("native/rust/Cargo.toml");
    let test_output = std::process::Command::new(&cargo)
        .args(["test", "--manifest-path"])
        .arg(&manifest)
        .args(["--offline", "--quiet"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run generated cxx tests");
    assert!(
        test_output.status.success(),
        "generated cxx tests failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test_output.stdout),
        String::from_utf8_lossy(&test_output.stderr)
    );

    let build_output = std::process::Command::new(cargo)
        .args(["build", "--manifest-path"])
        .arg(&manifest)
        .args(["--offline", "--quiet", "--bin", "native-boundary-helper"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("build generated helper");
    assert!(
        build_output.status.success(),
        "generated helper failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    let helper = target_dir.join("debug/native-boundary-helper");
    assert!(
        helper.is_file(),
        "missing generated helper {}",
        helper.display()
    );
    crate::commands::bind::cpp_binding_generator::execution_test::assert_generated_helper_replies(
        &helper,
    );
    helper
}

#[test]
pub(super) fn generated_cxx_adapter_compiles_and_enforces_public_protocol() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("adapter_protocol");
    let target_dir = temp_dir("adapter_protocol_target");
    generate_cpp_bindings(&fixture_manifest(), &out_dir).expect("generate cxx package");

    compile_and_exercise_generated_cpp_adapter(&out_dir, &target_dir);

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}

#[test]
pub(super) fn generated_cxx_bridge_compiles_links_owns_and_executes_from_terlan() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("end_to_end");
    let target_dir = temp_dir("end_to_end_target");
    generate_cpp_bindings(&fixture_manifest(), &out_dir).expect("generate cxx package");
    let helper = compile_and_exercise_generated_cpp_adapter(&out_dir, &target_dir);

    let helper_env = "TERLAN_CPP_FIXTURE_NATIVE_BOUNDARY_HELPER_PATH";
    let previous_helper = std::env::var_os(helper_env);
    std::env::set_var(helper_env, &helper);
    let exit_code = crate::commands::test::run(
        crate::CliCommand {
            verb: Some("test".to_string()),
            args: vec![out_dir.join("tests").to_string_lossy().into_owned()],
        },
        crate::CliState::default(),
    );
    if let Some(previous_helper) = previous_helper {
        std::env::set_var(helper_env, previous_helper);
    } else {
        std::env::remove_var(helper_env);
    }
    assert_eq!(exit_code, ExitCode::SUCCESS);

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}

#[test]
pub(super) fn generated_public_contract_is_stable_and_covers_every_required_rejection_family() {
    let first = temp_dir("stable_first");
    let second = temp_dir("stable_second");
    generate_cpp_bindings(&fixture_manifest(), &first).expect("first generation");
    generate_cpp_bindings(&fixture_manifest(), &second).expect("second generation");

    for relative in [
        "terlan.toml",
        "bindings/native-binding-manifest.json",
        "bindings/skipped-symbols.json",
        "docs/cpp_fixture.NativeBoundary.md",
        "docs/cpp_fixture.NativeGauge.md",
        "src/cpp_fixture/NativeBoundary.terl",
        "src/cpp_fixture/NativeGauge.terl",
        "tests/cpp_fixture/NativeBoundaryTest.terl",
        "tests/cpp_fixture/NativeGaugeTest.terl",
    ] {
        assert_eq!(
            fs::read(first.join(relative)).expect("first generated contract"),
            fs::read(second.join(relative)).expect("second generated contract"),
            "generated `{relative}` changed across identical runs"
        );
    }
    let first_text =
        fs::read_to_string(first.join("bindings/skipped-symbols.json")).expect("first skips");
    let second_text =
        fs::read_to_string(second.join("bindings/skipped-symbols.json")).expect("second skips");
    assert_eq!(first_text, second_text);
    let skipped: Value = serde_json::from_str(&first_text).expect("parse skips");
    let reasons = skipped["skipped"]
        .as_array()
        .expect("skip array")
        .iter()
        .map(|entry| entry["reason"].as_str().expect("reason"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        reasons,
        BTreeSet::from([
            "cpp.callback.unsupported",
            "cpp.exception.crossing",
            "cpp.inheritance.unsupported",
            "cpp.lifetime.borrowed",
            "cpp.annotation.unsupported",
            "cpp.overload.ambiguous",
            "cpp.ownership.unknown",
            "cpp.pointer.unsupported",
            "cpp.template.unspecialized",
            "cpp.type.unmapped",
            "cpp.variadic.unsupported",
        ])
    );
    let messages = skipped["skipped"]
        .as_array()
        .expect("skip array")
        .iter()
        .map(|entry| entry["message"].as_str().expect("fixed skip message"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        messages,
        BTreeSet::from([
            "Ambiguous overloads do not have a unique generated name.",
            "Borrowed reference lifetime is not represented by the generated boundary.",
            "Callback lifetime is not represented by the generated boundary.",
            "Preprocessor macros do not have a typed callable ABI.",
            "Raw pointer ownership is not represented by the generated boundary.",
            "Result ownership was not classified by package policy.",
            "The canonical C++ type has no reviewed Terlan mapping.",
            "Uncontained C++ exceptions cannot cross the generated boundary.",
            "Unspecialized templates do not have a concrete generated ABI.",
            "Unsupported inheritance layout cannot cross the generated boundary.",
            "Variadic arguments do not have a stable generated signature.",
        ])
    );
    for entry in skipped["skipped"].as_array().expect("skip array") {
        assert_eq!(
            entry
                .as_object()
                .expect("skip entry")
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["id", "message", "reason", "source", "symbol",]),
            "skip entry contains a free-form field: {entry}"
        );
        assert!(
            entry["source"]
                .as_str()
                .is_some_and(|source| source.starts_with("native_boundary.hpp:")),
            "missing stable source provenance in {entry}"
        );
    }

    fs::remove_dir_all(first).expect("remove first output");
    fs::remove_dir_all(second).expect("remove second output");
}

#[test]
pub(super) fn skipped_symbol_output_ignores_package_authored_free_form_detail() {
    let marker = "PACKAGE_AUTHORED_FREE_FORM_TEXT_MUST_NOT_ESCAPE";
    let manifest = write_fixture_variant("free_form_skip_detail", |manifest| {
        policy_mut(manifest, "unsupported.raw_pointer")["rejection"]["detail"] =
            Value::String(marker.into());
    });
    let out_dir = temp_dir("free_form_skip_detail_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate reviewed skip snapshot");
    let skipped = fs::read_to_string(out_dir.join("bindings/skipped-symbols.json"))
        .expect("read skipped-symbol snapshot");
    assert!(!skipped.contains(marker));
    assert!(!skipped.contains("\"detail\""));
    assert!(skipped.contains(
        "\"message\": \"Raw pointer ownership is not represented by the generated boundary.\""
    ));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn bindable_unsafe_cpp_shapes_fail_with_stable_diagnostic_families() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        ("pointer", "cpp.pointer.unsupported", |metadata| {
            symbol_mut(metadata, "function.make_native_boundary")["parameters"][0]["ty"]
                ["pointer_depth"] = Value::Number(1.into());
        }),
        ("lifetime", "cpp.lifetime.borrowed", |metadata| {
            symbol_mut(metadata, "function.make_native_boundary")["parameters"][0]["ty"]
                ["reference"] = Value::String("lvalue".into());
        }),
        ("template", "cpp.template.unspecialized", |metadata| {
            symbol_mut(metadata, "function.make_native_boundary")["template_parameters"] =
                serde_json::json!(["T"]);
        }),
        ("exception", "cpp.exception.crossing", |metadata| {
            symbol_mut(metadata, "function.make_native_boundary")["noexcept"] = Value::Bool(false);
        }),
        ("macro", "cpp.annotation.unsupported", |metadata| {
            select_for_binding(metadata, "unsupported.macro");
        }),
        ("callback", "cpp.callback.unsupported", |metadata| {
            select_for_binding(metadata, "unsupported.callback");
        }),
        ("variadic", "cpp.variadic.unsupported", |metadata| {
            select_for_binding(metadata, "unsupported.variadic");
        }),
        ("inheritance", "cpp.inheritance.unsupported", |metadata| {
            select_for_binding(metadata, "unsupported.inheritance");
            let policy = policy_mut(metadata, "unsupported.inheritance");
            policy["ownership"] = Value::String("unique".into());
            policy["thread_safety"] = Value::String("thread_confined".into());
        }),
        ("unmapped", "cpp.type.unmapped", |metadata| {
            select_for_binding(metadata, "unsupported.type");
        }),
    ];

    for (name, family, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let out_dir = temp_dir(&format!("{name}_out"));
        let error = generate_cpp_bindings(&manifest, &out_dir).expect_err("shape must fail");
        assert!(
            error.contains(&format!("error[{family}]")),
            "unexpected {name} diagnostic: {error}"
        );
        assert!(error.contains("native_boundary.hpp:"));
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn generated_owned_value_adapter_selects_exact_overload_and_supplies_defaults() {
    let manifest = write_fixture_variant("selected_owned_overload", |manifest| {
        symbol_mut(manifest, "function.make_native_boundary")["overload_candidates"] =
            Value::Number(2.into());
    });
    let out_dir = temp_dir("selected_owned_overload_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate selected overload adapter");
    let adapter =
        fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
            .expect("read selected overload adapter");
    assert!(adapter
        .contains("using TerlanSelectedOverload = NativeBoundary (*)(IntArrayRef, std::int64_t);"));
    assert!(adapter
        .contains("static_cast<TerlanSelectedOverload>(&terlan_fixture::make_native_boundary)"));
    assert!(
        adapter.contains("terlan_selected_overload(IntArrayRef(value.data(), value.size()), 0)")
    );
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_lowers_numeric_cpp_scalars() {
    let manifest = write_fixture_variant("numeric_cpp_scalar", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const Scalar &",
            "canonical": "const c10::Scalar &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["overload_candidates"] = Value::Number(2.into());
    });
    let out_dir = temp_dir("numeric_cpp_scalar_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate numeric scalar adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read numeric scalar adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read numeric scalar adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read numeric scalar bridge");
    assert!(header.contains("const terlan_fixture::NativeBoundary& value, std::int64_t delta"));
    assert!(source.contains(
        "using TerlanSelectedOverload = NativeBoundary (terlan_fixture::NativeBoundary::*)(const Scalar &) const;"
    ));
    assert!(source.contains("(value.*terlan_selected_overload)(c10::Scalar(delta))"));
    assert!(bridge.contains("value: &NativeBoundary, delta: i64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_lowers_present_optional_cpp_scalars() {
    let manifest = write_fixture_variant("optional_numeric_cpp_scalar", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const std::optional<at::Scalar> &",
            "canonical": "const std::optional<c10::Scalar> &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["overload_candidates"] = Value::Number(2.into());
    });
    let out_dir = temp_dir("optional_numeric_cpp_scalar_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional numeric scalar adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read optional numeric scalar adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional numeric scalar adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional numeric scalar bridge");
    assert!(header.contains("const terlan_fixture::NativeBoundary& value, std::int64_t delta"));
    assert!(source.contains(
        "(value.*terlan_selected_overload)(std::optional<c10::Scalar>(c10::Scalar(delta)))"
    ));
    assert!(bridge.contains("value: &NativeBoundary, delta: i64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_lowers_selected_optional_cpp_scalars() {
    let manifest = write_fixture_variant("selected_optional_numeric_cpp_scalar", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const std::optional<at::Scalar> &",
            "canonical": "const std::optional<c10::Scalar> &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["overload_candidates"] = Value::Number(2.into());
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "delta_is_int", "ty": "Bool"},
            {"name": "delta_int", "ty": "Int"},
            {"name": "delta_float", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("selected_optional_numeric_cpp_scalar_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate selected optional numeric scalar adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read selected optional numeric scalar adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read selected optional numeric scalar adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read selected optional numeric scalar bridge");
    assert!(header.contains(
        "const terlan_fixture::NativeBoundary& value, bool delta_is_int, std::int64_t delta_int, double delta_float"
    ));
    assert!(source.contains(
        "std::optional<c10::Scalar>(delta_is_int ? c10::Scalar(delta_int) : c10::Scalar(delta_float))"
    ));
    assert!(bridge
        .contains("value: &NativeBoundary, delta_is_int: bool, delta_int: i64, delta_float: f64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_adapter_retains_ignored_public_argument_without_forwarding_it() {
    let manifest = write_fixture_variant("ignored_public_argument", |manifest| {
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "unused_compatibility", "ty": "Int", "cpp_ignore": true},
            {"name": "delta", "ty": "Int", "cpp_parameter": "delta"}
        ]);
    });
    let out_dir = temp_dir("ignored_public_argument_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate ignored public argument");

    let module = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("read generated module");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read generated adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read generated adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read generated bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read generated native helper");

    assert!(module.contains(
        "pub shifted(boundary: NativeBoundary, unused_compatibility: Int, delta: Int): NativeBoundary"
    ));
    assert!(header.contains("const terlan_fixture::NativeBoundary& value, std::int64_t delta"));
    assert!(source.contains("value.shifted(delta)"));
    assert!(bridge.contains("value: &NativeBoundary, delta: i64"));
    assert!(helper.contains("Arg::Int(_arg_1), Arg::Int(arg_2)"));
    assert!(helper.contains(
        "ffi::terlan_owned_value_cpp_fixture_nativeboundary_shifted(value.as_ref().expect(\"validated non-null handle\"), *arg_2)"
    ));
    assert!(!header.contains("unused_compatibility"));
    assert!(!bridge.contains("unused_compatibility"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn ignored_public_arguments_reject_ambiguous_or_resource_shapes() {
    for (name, ignored) in [
        (
            "ignored_with_cpp_parameter",
            serde_json::json!({
                "name": "unused_compatibility",
                "ty": "Int",
                "cpp_ignore": true,
                "cpp_parameter": "delta"
            }),
        ),
        (
            "ignored_resource",
            serde_json::json!({
                "name": "unused_compatibility",
                "ty": "NativeBoundary",
                "cpp_ignore": true
            }),
        ),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            function_mut(manifest, "shifted")["args"] = serde_json::json!([
                {"name": "boundary", "ty": "NativeBoundary"},
                ignored,
                {"name": "delta", "ty": "Int", "cpp_parameter": "delta"}
            ]);
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("invalid ignored public argument must fail");
        assert!(
            error.contains("error[cpp.type.ignored_parameter_shape]"),
            "unexpected error: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn generated_owned_value_adapter_copies_resource_list_inputs() {
    let manifest = write_fixture_variant("resource_list_input", |manifest| {
        symbol_mut(manifest, "method.native_boundary.shifted")["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const c10::IListRef<NativeBoundary> &",
            "canonical": "const c10::IListRef<terlan_fixture::NativeBoundary> &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "others", "ty": "List[NativeBoundary]"}
        ]);
    });
    let out_dir = temp_dir("resource_list_input_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate resource-list input adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read resource-list adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read resource-list adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read resource-list bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read resource-list helper");
    let collector = "TerlanCppFixtureNativeBoundaryNativeBoundaryResourceListInput";
    assert!(header.contains(&format!("class {collector} final")));
    assert!(header.contains("std::vector<terlan_fixture::NativeBoundary> values_"));
    assert!(source.contains("values_.push_back(value)"));
    assert!(source.contains("value.shifted(delta.values())"));
    assert!(bridge.contains(&format!("type {collector};")));
    assert!(bridge.contains(&format!(
        "delta: &{collector}) -> UniquePtr<NativeBoundary>"
    )));
    assert!(helper.contains("value.strip_prefix(\"lh:\")"));
    assert!(helper.contains("arg_1 @ (Arg::Handles(_) | Arg::EmptyList)"));
    assert!(helper.contains("for handle in arg_handles(arg_1)"));
    assert!(helper.contains("let arg_1_list_ref = arg_1_list.as_ref()"));
    assert!(helper.contains("arg_1_list_ref"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_pointer_adapter_copies_resource_list_inputs() {
    let manifest = write_fixture_variant("owned_pointer_resource_list_input", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const c10::IListRef<NativeBoundary> &",
            "canonical": "const c10::IListRef<terlan_fixture::NativeBoundary> &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["returns"] = serde_json::json!({
            "spelling": "std::unique_ptr<NativeBoundary>",
            "canonical": "std::unique_ptr<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "others", "ty": "List[NativeBoundary]"}
        ]);
    });
    let out_dir = temp_dir("owned_pointer_resource_list_input_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate owned-pointer resource-list input adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read owned-pointer resource-list adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read owned-pointer resource-list bridge");
    assert!(source.contains(
        "std::unique_ptr<terlan_fixture::NativeBoundary> terlan_owned_value_cpp_fixture_nativeboundary_shifted(const terlan_fixture::NativeBoundary& value, const TerlanCppFixtureNativeBoundaryNativeBoundaryResourceListInput& delta) noexcept"
    ));
    assert!(source.contains("return value.shifted(delta.values());"));
    assert!(!source.contains("make_unique<terlan_fixture::NativeBoundary>(value.shifted"));
    assert!(bridge.contains(
        "fn terlan_owned_value_cpp_fixture_nativeboundary_shifted(value: &NativeBoundary, delta: &TerlanCppFixtureNativeBoundaryNativeBoundaryResourceListInput) -> UniquePtr<NativeBoundary>;"
    ));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_wraps_reviewed_enums_in_cpp_optional() {
    let manifest = write_fixture_variant("optional_cpp_enum", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("mode".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "std::optional<BoundaryMode>",
            "canonical": "std::optional<terlan_fixture::BoundaryMode>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "mode", "ty": "BoundaryMode"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_enum_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional enum adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read optional enum adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional enum adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional enum bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read optional enum helper");
    assert!(header.contains("rust::Slice<const std::int64_t> value, std::int64_t mode"));
    assert!(source
        .contains("std::optional<BoundaryMode>(static_cast<terlan_fixture::BoundaryMode>(mode))"));
    assert!(bridge.contains("value: &[i64], mode: i64"));
    assert!(helper.contains("\"raw\" => 7_i64"));
    assert!(helper.contains("\"doubled\" => 41_i64"));
    assert!(helper.contains("\"offset\" => 99_i64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_wraps_integer_compatibility_enums() {
    let manifest = write_fixture_variant("optional_cpp_integer_enum", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("mode".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "std::optional<BoundaryMode>",
            "canonical": "std::optional<terlan_fixture::BoundaryMode>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "mode", "ty": "Int"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_integer_enum_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate integer enum adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read integer enum adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read integer enum bridge");
    assert!(source
        .contains("std::optional<BoundaryMode>(static_cast<terlan_fixture::BoundaryMode>(mode))"));
    assert!(bridge.contains("value: &[i64], mode: i64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_casts_integer_compatibility_enums() {
    let manifest = write_fixture_variant("cpp_integer_enum", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("mode".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "BoundaryMode",
            "canonical": "terlan_fixture::BoundaryMode",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false,
            "enum_type": true
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "mode", "ty": "Int"}
        ]);
    });
    let out_dir = temp_dir("cpp_integer_enum_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate direct integer enum adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read integer enum adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read integer enum bridge");
    assert!(source.contains("static_cast<terlan_fixture::BoundaryMode>(mode)"));
    assert!(bridge.contains("value: &[i64], mode: i64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_adapter_omits_required_middle_optional_and_wraps_resource_reference() {
    let manifest = write_fixture_variant("optional_cpp_resource_middle", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"] = serde_json::json!([
            {
                "name": "offset",
                "ty": {
                    "spelling": "std::optional<std::int64_t>",
                    "canonical": "std::optional<std::int64_t>",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            },
            {
                "name": "boundary",
                "ty": {
                    "spelling": "const std::optional<NativeBoundary> &",
                    "canonical": "const std::optional<terlan_fixture::NativeBoundary> &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input",
                "default": "={}"
            },
            {
                "name": "scale",
                "ty": {
                    "spelling": "double",
                    "canonical": "double",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }
        ]);
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "other", "ty": "NativeBoundary"},
            {"name": "scale", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_resource_middle_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional resource adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read optional resource header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional resource source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional resource bridge");
    assert!(header.contains(
        "const terlan_fixture::NativeBoundary& value, const terlan_fixture::NativeBoundary& boundary, double scale"
    ));
    assert!(source.contains(
        "value.shifted(std::nullopt, std::optional<terlan_fixture::NativeBoundary>(boundary), scale)"
    ));
    assert!(bridge.contains("value: &NativeBoundary, boundary: &NativeBoundary, scale: f64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_adapter_aligns_positive_resource_with_pos_abbreviation() {
    let manifest = write_fixture_variant("optional_cpp_positive_resource", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.shifted");
        symbol["parameters"] = serde_json::json!([
            {
                "name": "weight",
                "ty": {
                    "spelling": "const std::optional<NativeBoundary> &",
                    "canonical": "const std::optional<terlan_fixture::NativeBoundary> &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input",
                "default": "={}"
            },
            {
                "name": "pos_weight",
                "ty": {
                    "spelling": "const std::optional<NativeBoundary> &",
                    "canonical": "const std::optional<terlan_fixture::NativeBoundary> &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input",
                "default": "={}"
            },
            {
                "name": "scale",
                "ty": {
                    "spelling": "double",
                    "canonical": "double",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }
        ]);
        function_mut(manifest, "shifted")["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "positive_weight", "ty": "NativeBoundary"},
            {"name": "scale", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_positive_resource_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate abbreviated optional resource");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional resource source");
    assert!(source.contains(
        "value.shifted(std::nullopt, std::optional<terlan_fixture::NativeBoundary>(pos_weight), scale)"
    ));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_wraps_bool_in_cpp_optional() {
    let manifest = write_fixture_variant("optional_cpp_bool", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("enabled".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "std::optional<bool>",
            "canonical": "std::optional<bool>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "enabled", "ty": "Bool"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_bool_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional bool adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional bool adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional bool bridge");
    assert!(source.contains("std::optional<bool>(enabled)"));
    assert!(bridge.contains("value: &[i64], enabled: bool"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_lowers_optional_presence_pair() {
    let manifest = write_fixture_variant("optional_cpp_presence_pair", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("enabled".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "std::optional<bool>",
            "canonical": "std::optional<bool>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "has_enabled", "ty": "Bool"},
            {"name": "enabled", "ty": "Bool"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_presence_pair_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional presence-pair adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read optional presence-pair adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional presence-pair adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional presence-pair bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read optional presence-pair helper");
    assert!(header.contains("bool has_enabled, bool enabled"));
    assert!(source.contains("has_enabled ? std::optional<bool>(enabled) : std::nullopt"));
    assert!(bridge.contains("has_enabled: bool, enabled: bool"));
    assert!(helper.contains("*arg_1, *arg_2"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_lowers_tagged_scalar_choice() {
    let manifest = write_fixture_variant("cpp_tagged_scalar_choice", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("native_scale".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "const at::Scalar &",
            "canonical": "const c10::Scalar &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "scale_is_int", "ty": "Bool"},
            {"name": "scale_int", "ty": "Int"},
            {"name": "scale_float", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("cpp_tagged_scalar_choice_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate tagged Scalar adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_owned_value_adapters.hpp"))
            .expect("read tagged Scalar adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read tagged Scalar adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read tagged Scalar bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read tagged Scalar helper");
    assert!(header.contains("bool scale_is_int, std::int64_t scale_int, double scale_float"));
    assert!(source.contains("scale_is_int ? c10::Scalar(scale_int) : c10::Scalar(scale_float)"));
    assert!(bridge.contains("scale_is_int: bool, scale_int: i64, scale_float: f64"));
    assert!(helper.contains("*arg_1, *arg_2, *arg_3"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_adapter_preserves_legacy_prefixed_scalar_choice_names() {
    let manifest = write_fixture_variant("cpp_prefixed_scalar_choice", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("value".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "const at::Scalar &",
            "canonical": "const c10::Scalar &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "values", "ty": "List[Int]"},
            {"name": "value_is_int", "ty": "Bool"},
            {"name": "integer_value", "ty": "Int"},
            {"name": "floating_value", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("cpp_prefixed_scalar_choice_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate prefixed Scalar adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read prefixed Scalar adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read prefixed Scalar bridge");
    assert!(
        source.contains("value_is_int ? c10::Scalar(integer_value) : c10::Scalar(floating_value)")
    );
    assert!(bridge.contains("value_is_int: bool, integer_value: i64, floating_value: f64"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_constructs_optional_string_values() {
    let manifest = write_fixture_variant("optional_cpp_string_value", |manifest| {
        manifest["modules"][0]["types"]
            .as_array_mut()
            .expect("types")
            .push(serde_json::json!({
                "name": "Device",
                "cpp_symbol": "record.native_snapshot",
                "kind": "string_value",
                "documentation": "String-constructed fixture value."
            }));
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("device".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "std::optional<NativeSnapshot>",
            "canonical": "std::optional<terlan_fixture::NativeSnapshot>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "device", "ty": "Device"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_string_value_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate optional string value adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional string value adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional string value bridge");
    let public = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("read optional string value public module");
    assert!(source.contains(
        "std::optional<NativeSnapshot>(terlan_fixture::NativeSnapshot(std::string(device.data(), device.size())))"
    ));
    assert!(bridge.contains("value: &[i64], device: &str"));
    assert!(public.contains("pub type Device = String."));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_constructs_present_optional_string_views() {
    let manifest = write_fixture_variant("optional_cpp_string_view", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("rounding_mode".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "::std::optional<c10::string_view>",
            "canonical": "std::optional<std::basic_string_view<char>>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "rounding_mode", "ty": "String"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_string_view_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate present optional string-view adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional string-view adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional string-view bridge");
    assert!(source.contains(
        "::std::optional<c10::string_view>(std::basic_string_view<char>(rounding_mode.data(), rounding_mode.size()))"
    ));
    assert!(bridge.contains("value: &[i64], rounding_mode: &str"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_owned_value_adapter_constructs_present_optional_integer_array_refs() {
    let manifest = write_fixture_variant("optional_cpp_integer_array_ref", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["parameters"][1]["name"] = Value::String("dim".into());
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "OptionalIntArrayRef",
            "canonical": "c10::OptionalArrayRef<long>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"},
            {"name": "dimensions", "ty": "List[Int]"}
        ]);
    });
    let out_dir = temp_dir("optional_cpp_integer_array_ref_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate present optional integer ArrayRef adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read optional integer ArrayRef adapter source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read optional integer ArrayRef bridge");
    assert!(
        source.contains("OptionalIntArrayRef(c10::ArrayRef<std::int64_t>(dim.data(), dim.size()))")
    );
    assert!(bridge.contains("value: &[i64], dim: &[i64]"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_overload_call_normalizes_braced_default_expressions() {
    let manifest = write_fixture_variant("cpp_braced_default_expression", |manifest| {
        let symbol = symbol_mut(manifest, "function.make_native_boundary");
        symbol["overload_candidates"] = serde_json::json!(2);
        symbol["parameters"][1]["default"] = Value::String("={}".into());
        function_mut(manifest, "new")["args"] = serde_json::json!([
            {"name": "value", "ty": "List[Int]"}
        ]);
    });
    let out_dir = temp_dir("cpp_braced_default_expression_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate normalized braced default call");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_owned_value_adapters.cc"))
        .expect("read normalized braced default adapter source");
    assert!(
        source.contains("terlan_selected_overload(IntArrayRef(value.data(), value.size()), {})")
    );
    assert!(!source.contains(", ={})"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_public_contract_preserves_integer_list_defaults() {
    let manifest = write_fixture_variant("cpp_integer_list_default", |manifest| {
        function_mut(manifest, "new")["args"][0]["default"] = Value::String("[-2, -1]".into());
    });
    let out_dir = temp_dir("cpp_integer_list_default_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate integer-list public default");
    let public = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("read integer-list default public module");
    assert!(public.contains("value: List[Int] = [-2, -1]"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_string_projection_uses_extracted_value_stringifier() {
    let manifest = write_fixture_variant("cpp_string_projection", |manifest| {
        manifest["modules"][0]["types"]
            .as_array_mut()
            .expect("types")
            .push(serde_json::json!({
                "name": "Device",
                "cpp_symbol": "record.native_snapshot",
                "kind": "string_value",
                "stringifier": "method.native_snapshot.str",
                "documentation": "String-projected fixture value."
            }));
        let getter = symbol_mut(manifest, "method.native_boundary.mode");
        getter["returns"] = serde_json::json!({
            "spelling": "NativeSnapshot",
            "canonical": "terlan_fixture::NativeSnapshot",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        let mut stringifier =
            symbol_mut(manifest, "method.native_snapshot.projected_value").clone();
        stringifier["id"] = Value::String("method.native_snapshot.str".into());
        stringifier["cpp_name"] = Value::String("str".into());
        stringifier["overload_set"] = Value::String("terlan_fixture::NativeSnapshot::str".into());
        stringifier["returns"] = serde_json::json!({
            "spelling": "std::string",
            "canonical": "std::basic_string<char>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(stringifier);
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .push(serde_json::json!({
                "symbol": "method.native_snapshot.str",
                "disposition": "bind"
            }));
        let function = function_mut(manifest, "mode");
        function["role"] = Value::String("string_projection".into());
        function["returns"] = Value::String("Device".into());
    });
    let out_dir = temp_dir("cpp_string_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate string projection adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_string_adapters.cc"))
        .expect("read string projection adapter");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(source.contains("const auto projected = value.mode()"));
    assert!(source.contains("projected.str()"));
    assert!(bridge.contains(
        "fn terlan_string_cpp_fixture_nativeboundary_mode(value: &NativeBoundary) -> UniquePtr<CxxString>;"
    ));
    assert!(helper.contains("ffi::terlan_string_cpp_fixture_nativeboundary_mode"));
    assert!(helper.contains("ok_string {}"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_scalar_projection_contains_public_inherited_getter() {
    let manifest = write_fixture_variant("cpp_scalar_projection", |manifest| {
        let resource = symbol_mut(manifest, "record.native_boundary");
        resource["inheritance"] = serde_json::json!(["class terlan_fixture::BoundaryBase"]);
        resource["public_inheritance"] = serde_json::json!(["class terlan_fixture::BoundaryBase"]);
        symbol_mut(manifest, "method.native_boundary.tripled_or_throw")["receiver"] =
            Value::String("terlan_fixture::BoundaryBase".into());
        let function = function_mut(manifest, "tripled_or_error");
        function["role"] = Value::String("scalar_projection".into());
        function["returns"] = Value::String("Int".into());
        function
            .as_object_mut()
            .expect("function")
            .remove("fallible");
    });
    let out_dir = temp_dir("cpp_scalar_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate integer projection adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
        .expect("read integer projection adapter");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(source.contains("const terlan_fixture::NativeBoundary& value"));
    assert!(source.contains("const auto result = value.tripled_or_throw()"));
    assert!(helper.contains("ffi::terlan_exception_cpp_fixture_nativeboundary_tripled_or_error"));
    assert!(helper.contains("format!(\"ok_int {}\", envelope.value())"));
    assert!(helper.contains("envelope.code().to_string_lossy()"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_scalar_projection_selects_boolean_envelope_slot() {
    let manifest = write_fixture_variant("cpp_bool_projection", |manifest| {
        symbol_mut(manifest, "method.native_boundary.tripled_or_throw")["returns"] = serde_json::json!({
            "spelling": "bool",
            "canonical": "bool",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        let function = function_mut(manifest, "tripled_or_error");
        function["role"] = Value::String("scalar_projection".into());
        function["returns"] = Value::String("Bool".into());
        function
            .as_object_mut()
            .expect("function")
            .remove("fallible");
    });
    let out_dir = temp_dir("cpp_bool_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate Bool projection adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
        .expect("read scalar projection adapter");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(source.contains("true, 0, 0.0, result, \"\", \"\""));
    assert!(bridge.contains("fn bool_value(self: &TerlanExceptionEnvelope) -> bool;"));
    assert!(helper.contains("format!(\"ok_bool {}\", envelope.bool_value())"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_scalar_projection_omits_extracted_cpp_defaults() {
    let manifest = write_fixture_variant("cpp_defaulted_scalar_projection", |manifest| {
        let method = symbol_mut(manifest, "method.native_boundary.tripled_or_throw");
        method["parameters"] = serde_json::json!([{
            "name": "mode",
            "direction": "input",
            "default": "terlan_fixture::BoundaryMode::Plain",
            "ty": {
                "spelling": "BoundaryMode",
                "canonical": "terlan_fixture::BoundaryMode",
                "is_const": false,
                "pointer_depth": 0,
                "reference": "none",
                "function_pointer": false,
                "template_dependent": false,
                "enum_type": true
            }
        }]);
        let function = function_mut(manifest, "tripled_or_error");
        function["role"] = Value::String("scalar_projection".into());
        function["returns"] = Value::String("Int".into());
        function
            .as_object_mut()
            .expect("function")
            .remove("fallible");
    });
    let out_dir = temp_dir("cpp_defaulted_scalar_projection_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate projection that uses the extracted C++ default");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_exception_adapters.hpp"))
            .expect("read scalar projection adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
        .expect("read scalar projection adapter source");
    assert!(!header.contains("BoundaryMode mode"));
    assert!(source.contains("const auto result = value.tripled_or_throw()"));
    assert!(!source.contains("value.tripled_or_throw(mode)"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_scalar_projection_contains_free_enum_calls_with_mixed_inputs() {
    let manifest = write_fixture_variant("cpp_free_scalar_projection", |manifest| {
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.classify_native_boundary",
                "cpp_name": "classify_native_boundary",
                "source": {"path": "native_boundary.hpp", "line": 24, "column": 1},
                "kind": "function",
                "documentation": "Classifies one boundary through a reviewed enum.",
                "annotations": [],
                "overload_set": "terlan_fixture::classify_native_boundary",
                "returns": {
                    "spelling": "BoundaryMode",
                    "canonical": "terlan_fixture::BoundaryMode",
                    "enum_type": true,
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "parameters": [
                    {
                        "name": "mode",
                        "direction": "input",
                        "ty": {
                            "spelling": "BoundaryMode",
                            "canonical": "terlan_fixture::BoundaryMode",
                            "enum_type": true,
                            "is_const": false,
                            "pointer_depth": 0,
                            "reference": "none",
                            "function_pointer": false,
                            "template_dependent": false
                        }
                    },
                    {
                        "name": "other",
                        "direction": "input",
                        "ty": {
                            "spelling": "const NativeBoundary &",
                            "canonical": "const terlan_fixture::NativeBoundary &",
                            "is_const": true,
                            "pointer_depth": 0,
                            "reference": "lvalue",
                            "function_pointer": false,
                            "template_dependent": false
                        }
                    },
                    {
                        "name": "threshold",
                        "direction": "input",
                        "ty": {
                            "spelling": "double",
                            "canonical": "double",
                            "is_const": false,
                            "pointer_depth": 0,
                            "reference": "none",
                            "function_pointer": false,
                            "template_dependent": false
                        }
                    }
                ],
                "noexcept": false,
                "template_parameters": [],
                "overload_candidates": 1,
                "variadic": false
            }));
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .push(serde_json::json!({
                "symbol": "function.classify_native_boundary",
                "disposition": "bind",
                "exception": {
                    "error_code": "boundary_classification_failed",
                    "message": "Native boundary classification failed."
                }
            }));
        let module = manifest["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["module"] == "cpp_fixture.NativeBoundary")
            .expect("native boundary module");
        module["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "classify",
                "operation": "cpp_fixture.native_boundary.classify",
                "cpp_symbol": "function.classify_native_boundary",
                "role": "scalar_projection",
                "args": [
                    {"name": "mode", "ty": "Int"},
                    {"name": "other", "ty": "NativeBoundary"},
                    {"name": "threshold", "ty": "Float"}
                ],
                "returns": "Int",
                "blocking": "fast",
                "resource": "value",
                "documentation": "Returns the reviewed enum discriminant."
            }));
    });
    let out_dir = temp_dir("cpp_free_scalar_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate free scalar projection adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_exception_adapters.hpp"))
            .expect("read scalar projection adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
        .expect("read scalar projection adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(header.contains(
        "terlan_exception_cpp_fixture_nativeboundary_classify(std::int64_t mode, const NativeBoundary & other, double threshold)"
    ));
    assert!(source.contains(
        "terlan_fixture::classify_native_boundary(static_cast<terlan_fixture::BoundaryMode>(mode), other, threshold)"
    ));
    assert!(source.contains("mode != static_cast<std::int64_t>(terlan_fixture::BoundaryMode::Raw)"));
    assert!(source.contains("result != terlan_fixture::BoundaryMode::Raw"));
    assert!(source.contains("static_cast<std::int64_t>(result), 0.0, false"));
    assert!(bridge.contains(
        "fn terlan_exception_cpp_fixture_nativeboundary_classify(mode: i64, other: &NativeBoundary, threshold: f64)"
    ));
    assert!(helper.contains(
        "ffi::terlan_exception_cpp_fixture_nativeboundary_classify(*arg_0, arg_1_ref, *arg_2)"
    ));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_scalar_projection_permutates_tagged_scalar_and_resource_inputs() {
    let manifest = write_fixture_variant("cpp_permuted_scalar_projection", |manifest| {
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.compare_scalar_boundary",
                "cpp_name": "compare_scalar_boundary",
                "source": {"path": "native_boundary.hpp", "line": 25, "column": 1},
                "kind": "function",
                "documentation": "Compares a Scalar followed by a boundary.",
                "annotations": [],
                "overload_set": "terlan_fixture::compare_scalar_boundary",
                "returns": {
                    "spelling": "bool",
                    "canonical": "bool",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "parameters": [
                    {
                        "name": "scalar",
                        "direction": "input",
                        "ty": {
                            "spelling": "const c10::Scalar &",
                            "canonical": "const c10::Scalar &",
                            "is_const": true,
                            "pointer_depth": 0,
                            "reference": "lvalue",
                            "function_pointer": false,
                            "template_dependent": false
                        }
                    },
                    {
                        "name": "boundary",
                        "direction": "input",
                        "ty": {
                            "spelling": "const NativeBoundary &",
                            "canonical": "const terlan_fixture::NativeBoundary &",
                            "is_const": true,
                            "pointer_depth": 0,
                            "reference": "lvalue",
                            "function_pointer": false,
                            "template_dependent": false
                        }
                    }
                ],
                "noexcept": false,
                "template_parameters": [],
                "overload_candidates": 1,
                "variadic": false
            }));
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .push(serde_json::json!({
                "symbol": "function.compare_scalar_boundary",
                "disposition": "bind",
                "exception": {
                    "error_code": "scalar_boundary_failed",
                    "message": "Scalar boundary comparison failed."
                }
            }));
        let module = manifest["modules"]
            .as_array_mut()
            .expect("modules")
            .iter_mut()
            .find(|module| module["module"] == "cpp_fixture.NativeBoundary")
            .expect("native boundary module");
        module["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "compare_scalar_boundary",
                "operation": "cpp_fixture.native_boundary.compare_scalar_boundary",
                "cpp_symbol": "function.compare_scalar_boundary",
                "role": "scalar_projection",
                "args": [
                    {"name": "boundary", "ty": "NativeBoundary", "cpp_parameter": "boundary"},
                    {"name": "scalar_is_integer", "ty": "Bool", "cpp_parameter": "scalar"},
                    {"name": "scalar_integer", "ty": "Int"},
                    {"name": "scalar_floating", "ty": "Float"}
                ],
                "returns": "Bool",
                "blocking": "fast",
                "resource": "value",
                "documentation": "Exercises a generated public-to-C++ argument permutation."
            }));
    });
    let out_dir = temp_dir("cpp_permuted_scalar_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate permuted scalar projection");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_exception_adapters.hpp"))
            .expect("read scalar projection adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_exception_adapters.cc"))
        .expect("read scalar projection adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(header.contains(
        "compare_scalar_boundary(const NativeBoundary & boundary, bool scalar_is_integer, std::int64_t scalar_integer, double scalar_floating)"
    ));
    assert!(source.contains(
        "terlan_fixture::compare_scalar_boundary(scalar_is_integer ? c10::Scalar(scalar_integer) : c10::Scalar(scalar_floating), boundary)"
    ));
    assert!(bridge.contains(
        "compare_scalar_boundary(boundary: &NativeBoundary, scalar_is_integer: bool, scalar_integer: i64, scalar_floating: f64)"
    ));
    assert!(helper.contains("compare_scalar_boundary(arg_0_ref, *arg_1, *arg_2, *arg_3)"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_int_list_projection_copies_borrowed_array_ref() {
    let manifest = write_fixture_variant("cpp_int_list_projection", |manifest| {
        let getter = symbol_mut(manifest, "method.native_boundary.mode");
        getter["returns"] = serde_json::json!({
            "spelling": "c10::IntArrayRef",
            "canonical": "c10::ArrayRef<long>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        let function = function_mut(manifest, "mode");
        function["role"] = Value::String("int_list_projection".into());
        function["returns"] = Value::String("List[Int]".into());
    });
    let out_dir = temp_dir("cpp_int_list_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate integer-list projection adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_collection_adapters.cc"))
        .expect("read collection adapter");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(source.contains("const auto projected = value.mode()"));
    assert!(source.contains("projected.begin(), projected.end()"));
    assert!(bridge.contains(
        "fn terlan_collection_cpp_fixture_nativeboundary_mode(value: &NativeBoundary) -> UniquePtr<CxxVector<i64>>;"
    ));
    assert!(helper.contains("ffi::terlan_collection_cpp_fixture_nativeboundary_mode"));
    assert!(helper.contains("ok_ints"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_resource_list_projection_owns_elements_atomically() {
    let manifest = write_fixture_variant("cpp_resource_list_projection", |manifest| {
        let method = symbol_mut(manifest, "method.native_boundary.shifted");
        method["noexcept"] = Value::Bool(false);
        method["returns"] = serde_json::json!({
            "spelling": "std::vector<NativeBoundary>",
            "canonical": "std::vector<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        let policy = policy_mut(manifest, "method.native_boundary.shifted");
        policy["exception"] = serde_json::json!({
            "error_code": "resource_list_failed",
            "message": "Could not create the resource list."
        });
        let function = function_mut(manifest, "shifted");
        function["role"] = Value::String("resource_list_projection".into());
        function["returns"] = Value::String("List[NativeBoundary]".into());
    });
    let out_dir = temp_dir("cpp_resource_list_projection_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate owned resource-list projection");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_collection_adapters.hpp"))
            .expect("read collection adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_collection_adapters.cc"))
        .expect("read collection adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(header.contains("class TerlanCppFixtureNativeBoundaryShiftedResourceListResult"));
    assert!(header.contains("std::vector<terlan_fixture::NativeBoundary> values_"));
    assert!(source.contains("value.shifted(delta)"));
    assert!(
        source.contains("return std::make_unique<terlan_fixture::NativeBoundary>(values_[index])")
    );
    assert!(source.contains("\"resource_list_failed\", \"Could not create the resource list.\""));
    assert!(bridge.contains(
        "fn terlan_collection_cpp_fixture_nativeboundary_shifted_resources(value: &NativeBoundary, delta: i64) -> UniquePtr<TerlanCppFixtureNativeBoundaryShiftedResourceListResult>;"
    ));
    assert!(bridge.contains(
        "fn terlan_collection_cpp_fixture_nativeboundary_shifted_resources_element(result: &TerlanCppFixtureNativeBoundaryShiftedResourceListResult, index: usize) -> UniquePtr<NativeBoundary>;"
    ));
    let operation = helper
        .split_once("\"cpp_fixture.native_boundary.shifted\" =>")
        .expect("resource-list operation")
        .1;
    let copy_position = operation
        .find("values.push(value)")
        .expect("copy resource values");
    let publish_position = operation
        .find("self.handles.insert(id, HandleEntry")
        .expect("publish resource handles");
    assert!(copy_position < publish_position);
    assert!(helper.contains("ok_handles"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_free_function_prepends_resource_to_copied_lists() {
    let manifest = write_fixture_variant("cpp_free_resource_lists", |manifest| {
        let symbol = symbol_mut(manifest, "function.sum_integer_list");
        symbol["noexcept"] = Value::Bool(false);
        symbol["parameters"][0]["name"] = Value::String("self".into());
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "at::TensorList",
            "canonical": "c10::ArrayRef<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(serde_json::json!({
                "name": "indexing",
                "ty": {
                    "spelling": "c10::string_view",
                    "canonical": "std::basic_string_view<char>",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }));
        symbol["returns"] = serde_json::json!({
            "spelling": "std::vector<NativeBoundary>",
            "canonical": "std::vector<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        policy_mut(manifest, "function.sum_integer_list")["exception"] = serde_json::json!({
            "error_code": "free_resource_list_failed",
            "message": "Could not transform resource lists."
        });
        let function = function_mut(manifest, "sum_integers");
        function["role"] = Value::String("resource_list_projection".into());
        function["args"] = serde_json::json!([
            {"name": "first", "ty": "NativeBoundary"},
            {"name": "values", "ty": "List[NativeBoundary]", "prepend_resource": true},
            {"name": "indexing", "ty": "String"}
        ]);
        function["returns"] = Value::String("List[NativeBoundary]".into());
        function["resource"] = Value::String("owned_handle".into());
    });
    let out_dir = temp_dir("cpp_free_resource_lists_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate free-function resource-list adapter");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_collection_adapters.cc"))
        .expect("read collection adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(source.contains(
        "terlan_fixture::sum_integer_list(self.values(), c10::string_view(indexing.data(), indexing.size()))"
    ));
    assert!(bridge.contains(
        "fn terlan_collection_cpp_fixture_nativeboundary_sum_integers_resources(self_value: &TerlanCppFixtureNativeBoundaryNativeBoundaryResourceListInput, indexing: &str)"
    ));
    let operation = helper
        .split_once("\"cpp_fixture.native_boundary.sum_integers\" =>")
        .expect("free resource-list operation")
        .1;
    let prepend = operation
        .find("self.live(arg_0")
        .expect("validate prepended resource");
    let list = operation
        .find("for handle in arg_handles(arg_1)")
        .expect("copy remaining resource list");
    assert!(prepend < list);
    assert!(operation.contains(
        "ffi::terlan_collection_cpp_fixture_nativeboundary_sum_integers_resources(arg_1_list_ref, arg_2.as_str())"
    ));
    assert!(operation.contains("values.push(value)"));
    assert!(operation.contains("self.handles.insert(id, HandleEntry"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_resource_list_prepend_requires_matching_resource() {
    let manifest = write_fixture_variant("invalid_resource_list_prepend", |manifest| {
        let symbol = symbol_mut(manifest, "function.sum_integer_list");
        symbol["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "at::TensorList",
            "canonical": "c10::ArrayRef<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        let function = function_mut(manifest, "sum_integers");
        function["args"] = serde_json::json!([
            {"name": "first", "ty": "Int"},
            {"name": "values", "ty": "List[NativeBoundary]", "prepend_resource": true}
        ]);
    });
    let out_dir = temp_dir("invalid_resource_list_prepend_out");
    let error = generate_cpp_bindings(&manifest, &out_dir)
        .expect_err("reject a prepended scalar for a resource list");
    assert!(error.contains("error[cpp.type.resource_list_prepend]"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).expect("remove output");
    }
}

#[test]
pub(super) fn generated_mutable_free_function_contains_reference_result_and_list_input() {
    let manifest = write_fixture_variant("cpp_mutable_free_function", |manifest| {
        let symbol = symbol_mut(manifest, "function.sum_integer_list");
        symbol["noexcept"] = Value::Bool(false);
        symbol["parameters"]
            .as_array_mut()
            .expect("parameters")
            .insert(
                0,
                serde_json::json!({
                    "name": "out",
                    "ty": {
                        "spelling": "NativeBoundary &",
                        "canonical": "terlan_fixture::NativeBoundary &",
                        "is_const": false,
                        "pointer_depth": 0,
                        "reference": "lvalue",
                        "function_pointer": false,
                        "template_dependent": false
                    },
                    "direction": "in_out"
                }),
            );
        symbol["parameters"][1]["ty"] = serde_json::json!({
            "spelling": "at::TensorList",
            "canonical": "c10::ArrayRef<terlan_fixture::NativeBoundary>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(serde_json::json!({
                "name": "self",
                "ty": {
                    "spelling": "const NativeBoundary &",
                    "canonical": "const terlan_fixture::NativeBoundary &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }));
        symbol["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(serde_json::json!({
                "name": "scale",
                "ty": {
                    "spelling": "std::optional<double>",
                    "canonical": "std::optional<double>",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }));
        symbol["returns"] = serde_json::json!({
            "spelling": "const NativeBoundary &",
            "canonical": "const terlan_fixture::NativeBoundary &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        policy_mut(manifest, "function.sum_integer_list")["exception"] = serde_json::json!({
            "error_code": "mutable_free_function_failed",
            "message": "Could not execute mutable free function."
        });
        let function = function_mut(manifest, "sum_integers");
        function["role"] = Value::String("mutable_free_function".into());
        function["args"] = serde_json::json!([
            {"name": "out", "ty": "NativeBoundary"},
            {"name": "values", "ty": "List[NativeBoundary]"},
            {"name": "input", "ty": "NativeBoundary"},
            {"name": "has_scale", "ty": "Bool"},
            {"name": "scale", "ty": "Float"}
        ]);
        function["returns"] = Value::String("Unit".into());
        function["resource"] = Value::String("mutable_handle".into());
    });
    let out_dir = temp_dir("cpp_mutable_free_function_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate mutable free-function adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_mutation_adapters.hpp"))
            .expect("read mutation adapter header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_mutation_adapters.cc"))
        .expect("read mutation adapter source");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read CXX bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read native helper");
    assert!(header.contains("terlan_mutation_cpp_fixture_nativeboundary_sum_integers"));
    assert!(header.contains("NativeBoundary & out"));
    assert!(source.contains(
        "(void)terlan_fixture::sum_integer_list(out, values.values(), self, has_scale ? std::optional<double>(scale) : std::nullopt)"
    ));
    assert!(bridge.contains(
        "fn terlan_resource_cpp_fixture_nativeboundary_nativeboundary_copy(value: &NativeBoundary) -> UniquePtr<NativeBoundary>"
    ));
    assert!(source.contains("mutable_free_function_failed"));
    assert!(bridge.contains("out: Pin<&mut NativeBoundary>"));
    assert!(bridge.contains("self_value: &NativeBoundary"));
    assert!(bridge.contains("has_scale: bool, scale: f64"));
    let operation = helper
        .split_once("\"cpp_fixture.native_boundary.sum_integers\" =>")
        .expect("mutable free-function operation")
        .1;
    let copy_position = operation
        .find("let mut arg_1_list")
        .expect("copy resource list before mutation");
    let resource_copy_position = operation
        .find("let arg_2_copy")
        .expect("copy secondary resource before mutation");
    let borrow_position = operation
        .find("self.live_mut(arg_0")
        .expect("mutable target borrow");
    assert!(copy_position < borrow_position);
    assert!(resource_copy_position < borrow_position);
    assert!(
        operation.contains("ffi::terlan_resource_cpp_fixture_nativeboundary_nativeboundary_copy")
    );
    assert!(operation.contains(
        "ffi::terlan_mutation_cpp_fixture_nativeboundary_sum_integers(value.pin_mut(), arg_1_list_ref, arg_2_ref, *arg_3, *arg_4)"
    ));
    assert!(operation.contains("if envelope.is_ok()"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_mutable_free_function_safely_borrows_two_retained_outputs() {
    let manifest = write_fixture_variant("cpp_multi_output_mutation", |manifest| {
        let symbol = symbol_mut(manifest, "function.sum_integer_list");
        symbol["noexcept"] = Value::Bool(false);
        symbol["parameters"] = serde_json::json!([
            {
                "name": "out",
                "ty": {
                    "spelling": "NativeBoundary &",
                    "canonical": "terlan_fixture::NativeBoundary &",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "in_out"
            },
            {
                "name": "indices",
                "ty": {
                    "spelling": "NativeBoundary &",
                    "canonical": "terlan_fixture::NativeBoundary &",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "in_out"
            },
            {
                "name": "value",
                "ty": {
                    "spelling": "std::int64_t",
                    "canonical": "std::int64_t",
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }
        ]);
        symbol["returns"] = serde_json::json!({
            "spelling": "std::tuple<NativeBoundary &, NativeBoundary &>",
            "canonical": "std::tuple<terlan_fixture::NativeBoundary &, terlan_fixture::NativeBoundary &>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        });
        policy_mut(manifest, "function.sum_integer_list")["exception"] = serde_json::json!({
            "error_code": "multi_output_failed",
            "message": "Could not execute multi-output mutation."
        });
        let function = function_mut(manifest, "sum_integers");
        function["role"] = Value::String("mutable_free_function".into());
        function["args"] = serde_json::json!([
            {"name": "out", "ty": "NativeBoundary"},
            {"name": "indices", "ty": "NativeBoundary", "mutable": true},
            {"name": "value", "ty": "Int"}
        ]);
        function["returns"] = Value::String("Unit".into());
        function["resource"] = Value::String("mutable_handle".into());
    });
    let out_dir = temp_dir("cpp_multi_output_mutation_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate multi-output mutation");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_mutation_adapters.hpp"))
            .expect("mutation header");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");

    assert!(header.contains("NativeBoundary & out, NativeBoundary & indices"));
    assert!(bridge.contains("out: Pin<&mut NativeBoundary>, indices: Pin<&mut NativeBoundary>"));
    let operation = helper
        .split_once("\"cpp_fixture.native_boundary.sum_integers\" =>")
        .expect("multi-output operation")
        .1;
    assert!(operation.contains("if arg_0.id == arg_1.id"));
    assert!(operation.contains("self.handles.remove(&arg_0.id)"));
    assert!(operation.contains("self.handles.remove(&arg_1.id)"));
    assert!(operation.contains("arg_0_value.pin_mut(), arg_1_value.pin_mut(), *arg_2"));
    assert!(operation.contains("self.handles.insert(arg_0.id, arg_0_entry)"));
    assert!(operation.contains("self.handles.insert(arg_1.id, arg_1_entry)"));
}

#[test]
pub(super) fn generated_mutable_free_function_contains_logical_const_resource_mutation() {
    let manifest = write_fixture_variant("cpp_logical_const_mutation", |manifest| {
        let symbol = symbol_mut(manifest, "function.sum_integer_list");
        symbol["noexcept"] = Value::Bool(false);
        symbol["parameters"]
            .as_array_mut()
            .expect("parameters")
            .insert(
                0,
                serde_json::json!({
                    "name": "self",
                    "ty": {
                        "spelling": "const NativeBoundary &",
                        "canonical": "const terlan_fixture::NativeBoundary &",
                        "is_const": true,
                        "pointer_depth": 0,
                        "reference": "lvalue",
                        "function_pointer": false,
                        "template_dependent": false
                    },
                    "direction": "input"
                }),
            );
        symbol["returns"] = serde_json::json!({
            "spelling": "const NativeBoundary &",
            "canonical": "const terlan_fixture::NativeBoundary &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        policy_mut(manifest, "function.sum_integer_list")["exception"] = serde_json::json!({
            "error_code": "logical_const_mutation_failed",
            "message": "Could not execute logical const mutation."
        });
        let function = function_mut(manifest, "sum_integers");
        function["role"] = Value::String("mutable_free_function".into());
        function["args"] = serde_json::json!([
            {"name": "self", "ty": "NativeBoundary"},
            {"name": "values", "ty": "List[Int]"}
        ]);
        function["returns"] = Value::String("Unit".into());
        function["resource"] = Value::String("mutable_handle".into());
    });
    let out_dir = temp_dir("cpp_logical_const_mutation_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate logical-const mutation adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_mutation_adapters.hpp"))
            .expect("read logical-const mutation header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_mutation_adapters.cc"))
        .expect("read logical-const mutation source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read logical-const mutation bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read logical-const mutation helper");
    assert!(header.contains("terlan_fixture::NativeBoundary & self"));
    assert!(source.contains("(void)terlan_fixture::sum_integer_list(self"));
    assert!(source.contains("logical_const_mutation_failed"));
    assert!(bridge.contains("self_value: Pin<&mut NativeBoundary>"));
    assert!(helper.contains("self.live_mut(arg_0"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn generated_mutable_method_contains_logical_const_scalar_choice_mutation() {
    let manifest = write_fixture_variant("cpp_logical_const_method_mutation", |manifest| {
        let symbol = symbol_mut(manifest, "method.native_boundary.add");
        symbol["receiver_mutable"] = Value::Bool(false);
        symbol["noexcept"] = Value::Bool(false);
        symbol["returns"] = serde_json::json!({
            "spelling": "NativeBoundary &",
            "canonical": "terlan_fixture::NativeBoundary &",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        symbol["parameters"] = serde_json::json!([
            {
                "name": "end",
                "ty": {
                    "spelling": "const NativeBoundary &",
                    "canonical": "const terlan_fixture::NativeBoundary &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            },
            {
                "name": "value",
                "ty": {
                    "spelling": "const c10::Scalar &",
                    "canonical": "const c10::Scalar &",
                    "is_const": true,
                    "pointer_depth": 0,
                    "reference": "lvalue",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }
        ]);
        policy_mut(manifest, "method.native_boundary.add")["exception"] = serde_json::json!({
            "error_code": "logical_const_method_failed",
            "message": "Could not execute logical const method mutation."
        });
        let function = function_mut(manifest, "add");
        function["args"] = serde_json::json!([
            {"name": "boundary", "ty": "NativeBoundary"},
            {"name": "end", "ty": "NativeBoundary"},
            {"name": "value_is_int", "ty": "Bool"},
            {"name": "value_int", "ty": "Int"},
            {"name": "value_float", "ty": "Float"}
        ]);
    });
    let out_dir = temp_dir("cpp_logical_const_method_mutation_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("generate logical-const method mutation adapter");
    let header =
        fs::read_to_string(out_dir.join("native/rust/include/terlan_mutation_adapters.hpp"))
            .expect("read method mutation header");
    let source = fs::read_to_string(out_dir.join("native/rust/cpp/terlan_mutation_adapters.cc"))
        .expect("read method mutation source");
    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read method mutation bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read method mutation helper");
    assert!(header
        .contains("NativeBoundary& self_value, const NativeBoundary & end, bool value_is_int"));
    assert!(source.contains(
        "self_value.add(end, value_is_int ? c10::Scalar(value_int) : c10::Scalar(value_float))"
    ));
    assert!(source.contains("logical_const_method_failed"));
    assert!(bridge.contains("self_value: Pin<&mut NativeBoundary>"));
    assert!(helper.contains("ffi::terlan_mutation_cpp_fixture_nativeboundary_add("));
    assert!(helper.contains("value.pin_mut(), arg_1_ref, *arg_2, *arg_3, *arg_4"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn inherited_enum_getters_require_clang_proven_public_inheritance() {
    let private_manifest = write_fixture_variant("private_inherited_enum", |manifest| {
        symbol_mut(manifest, "method.native_boundary.mode")["receiver"] =
            Value::String("terlan_fixture::BoundaryBase".into());
        symbol_mut(manifest, "record.native_boundary")["inheritance"] =
            serde_json::json!(["class terlan_fixture::BoundaryBase"]);
    });
    let private_out = temp_dir("private_inherited_enum_out");
    let error = generate_cpp_bindings(&private_manifest, &private_out)
        .expect_err("reject unproven inherited receiver conversion");
    assert!(
        error.contains("requires a bindable zero-argument const enum getter"),
        "{error}"
    );

    let public_manifest = write_fixture_variant("public_inherited_enum", |manifest| {
        symbol_mut(manifest, "method.native_boundary.mode")["receiver"] =
            Value::String("terlan_fixture::BoundaryBase".into());
        let record = symbol_mut(manifest, "record.native_boundary");
        record["inheritance"] = serde_json::json!(["class terlan_fixture::BoundaryBase"]);
        record["public_inheritance"] = serde_json::json!(["class terlan_fixture::BoundaryBase"]);
    });
    let public_out = temp_dir("public_inherited_enum_out");
    generate_cpp_bindings(&public_manifest, &public_out)
        .expect("accept public inherited receiver conversion");
    let adapter = fs::read_to_string(public_out.join("native/rust/cpp/terlan_enum_adapters.cc"))
        .expect("read inherited enum adapter");
    assert!(adapter.contains("const terlan_fixture::NativeBoundary& value"));
    assert!(adapter.contains("value.mode()"));

    fs::remove_dir_all(private_manifest.parent().expect("variant root"))
        .expect("remove private variant");
    fs::remove_dir_all(public_manifest.parent().expect("variant root"))
        .expect("remove public variant");
    fs::remove_dir_all(public_out).expect("remove public output");
}

#[test]
pub(super) fn bindable_overload_without_generated_adapter_remains_rejected() {
    let manifest = write_fixture_variant("unadapted_overload", |manifest| {
        symbol_mut(manifest, "function.live_native_boundary_count")["overload_candidates"] =
            Value::Number(2.into());
    });
    let out_dir = temp_dir("unadapted_overload_out");
    let error = generate_cpp_bindings(&manifest, &out_dir).expect_err("reject direct overload");
    assert!(error.contains("error[cpp.overload.ambiguous]"), "{error}");
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
}

#[test]
pub(super) fn copied_container_mappings_reject_incompatible_public_types() {
    for (name, symbol, wrong_type) in [
        (
            "string_as_bytes",
            "method.native_boundary.label",
            "std.vm.Bytes.Bytes",
        ),
        ("bytes_as_string", "method.native_boundary.bytes", "String"),
        ("list_as_string", "method.native_boundary.samples", "String"),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            let module = manifest["modules"][0]["functions"]
                .as_array_mut()
                .expect("module functions");
            module
                .iter_mut()
                .find(|function| function["cpp_symbol"] == symbol)
                .expect("copied function")["returns"] = Value::String(wrong_type.into());
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("incompatible copied result must fail");
        assert!(
            error.contains("error[cpp.type.mapping_mismatch]"),
            "unexpected copied mapping diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn symbolic_enum_mappings_reject_unknown_duplicate_and_non_enum_shapes() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        ("enum_unknown", "unknown C++ enumerator", |manifest| {
            manifest["modules"][0]["types"][2]["variants"][0]["cpp_name"] =
                Value::String("Missing".into());
        }),
        ("enum_duplicate", "duplicate public names", |manifest| {
            manifest["modules"][0]["types"][2]["variants"][1]["atom"] = Value::String("raw".into());
        }),
        (
            "enum_result",
            "must return a module-owned enum",
            |manifest| {
                let functions = manifest["modules"][0]["functions"]
                    .as_array_mut()
                    .expect("module functions");
                functions
                    .iter_mut()
                    .find(|function| function["cpp_symbol"] == "method.native_boundary.mode")
                    .expect("enum projection")["returns"] = Value::String("String".into());
            },
        ),
    ];
    for (name, expected, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("invalid enum mapping must fail");
        assert!(
            error.contains(expected),
            "unexpected enum diagnostic: {error}"
        );
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn mapping_and_metadata_provenance_are_required_before_generation() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "mapping_schema",
            "unsupported C++ mapping schema",
            |manifest| {
                manifest["mapping"]["schema"] = Value::String("terlan.cpp.mapping.v0".into());
            },
        ),
        ("target", "must include a target triple", |manifest| {
            manifest["cpp_metadata"]["compile"]["target_triple"] = Value::String(String::new());
        }),
        (
            "standard",
            "unsupported C++ language standard",
            |manifest| {
                manifest["cpp_metadata"]["compile"]["language_standard"] =
                    Value::String("gnu++11".into());
            },
        ),
        (
            "source",
            "requires a non-empty, one-based source location",
            |manifest| {
                symbol_mut(manifest, "function.make_native_boundary")["source"]["line"] =
                    Value::Number(0.into());
            },
        ),
    ];

    for (name, expected, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("incomplete contract must fail");
        assert!(error.contains(expected), "unexpected {name} error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn structured_cpp_contract_requires_complete_type_and_compile_facts() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "canonical",
            "requires declared and canonical spellings",
            |manifest| {
                symbol_mut(manifest, "function.make_native_boundary")["parameters"][0]["ty"]
                    ["canonical"] = Value::String(String::new());
            },
        ),
        ("direction", "missing field `direction`", |manifest| {
            symbol_mut(manifest, "function.make_native_boundary")["parameters"][0]
                .as_object_mut()
                .expect("parameter")
                .remove("direction");
        }),
        (
            "overload_set",
            "requires stable overload-set identity",
            |manifest| {
                symbol_mut(manifest, "function.make_native_boundary")["overload_set"] =
                    Value::String(String::new());
            },
        ),
        ("annotation", "contains an empty annotation", |manifest| {
            symbol_mut(manifest, "function.make_native_boundary")["annotations"] =
                serde_json::json!([""]);
        }),
        (
            "include_root",
            "must resolve to a package-relative directory",
            |manifest| {
                manifest["cpp_metadata"]["compile"]["include_roots"] =
                    serde_json::json!(["../escape"]);
            },
        ),
        (
            "arguments",
            "requires non-empty, NUL-free arguments",
            |manifest| {
                manifest["cpp_metadata"]["compile"]["arguments"] = serde_json::json!([]);
            },
        ),
        ("define", "invalid C++ preprocessor define", |manifest| {
            manifest["cpp_metadata"]["compile"]["defines"] =
                serde_json::json!({"INVALID-NAME": "1"});
        }),
    ];

    for (name, expected, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("incomplete C++ facts must fail");
        assert!(error.contains(expected), "unexpected {name} error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
pub(super) fn package_policy_is_complete_unique_and_separate_from_extracted_metadata() {
    let missing = write_fixture_variant("missing_policy", |manifest| {
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .retain(|policy| policy["symbol"] != "unsupported.type");
    });
    let error = generate_cpp_bindings(&missing, &temp_dir("missing_policy_out"))
        .expect_err("missing policy must fail");
    assert!(error.contains("error[cpp.mapping.missing]"));
    assert!(error.contains("unsupported.type"));
    fs::remove_dir_all(missing.parent().expect("missing root")).expect("remove variant");

    let unknown = write_fixture_variant("unknown_policy", |manifest| {
        policy_mut(manifest, "unsupported.type")["symbol"] = Value::String("unknown.symbol".into());
    });
    let error = generate_cpp_bindings(&unknown, &temp_dir("unknown_policy_out"))
        .expect_err("unknown policy must fail");
    assert!(error.contains("references unknown extracted symbol `unknown.symbol`"));
    fs::remove_dir_all(unknown.parent().expect("unknown root")).expect("remove variant");

    let duplicate = write_fixture_variant("duplicate_policy", |manifest| {
        let duplicate = policy_mut(manifest, "unsupported.type").clone();
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .push(duplicate);
    });
    let error = generate_cpp_bindings(&duplicate, &temp_dir("duplicate_policy_out"))
        .expect_err("duplicate policy must fail");
    assert!(error.contains("duplicate C++ mapping policy for symbol `unsupported.type`"));
    fs::remove_dir_all(duplicate.parent().expect("duplicate root")).expect("remove variant");

    let leaked = write_fixture_variant("leaked_policy", |manifest| {
        symbol_mut(manifest, "record.native_boundary")["ownership"] =
            Value::String("unique".into());
    });
    let error = generate_cpp_bindings(&leaked, &temp_dir("leaked_policy_out"))
        .expect_err("policy fact in extracted metadata must fail");
    assert!(error.contains("unknown field `ownership`"));
    fs::remove_dir_all(leaked.parent().expect("leaked root")).expect("remove variant");
}
