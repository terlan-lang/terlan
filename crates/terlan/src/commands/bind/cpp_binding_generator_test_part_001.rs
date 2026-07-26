use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
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

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cpp_native_boundary")
}

fn fixture_manifest() -> PathBuf {
    fixture_dir().join("native-binding.json")
}

fn extractor_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/cpp_metadata_extractor/fixtures")
}

fn write_fixture_variant(name: &str, mutate: impl FnOnce(&mut Value)) -> PathBuf {
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

fn symbol_mut<'a>(metadata: &'a mut Value, id: &str) -> &'a mut Value {
    metadata["cpp_metadata"]["symbols"]
        .as_array_mut()
        .expect("symbols")
        .iter_mut()
        .find(|symbol| symbol["id"] == id)
        .expect("fixture symbol")
}

fn policy_mut<'a>(manifest: &'a mut Value, id: &str) -> &'a mut Value {
    manifest["mapping"]["symbols"]
        .as_array_mut()
        .expect("mapping symbols")
        .iter_mut()
        .find(|policy| policy["symbol"] == id)
        .expect("fixture policy")
}

fn function_mut<'a>(manifest: &'a mut Value, name: &str) -> &'a mut Value {
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

fn select_for_binding(manifest: &mut Value, id: &str) {
    let policy = policy_mut(manifest, id);
    policy["disposition"] = Value::String("bind".into());
    policy
        .as_object_mut()
        .expect("mapping policy")
        .remove("rejection");
}

#[test]
fn structured_cpp_metadata_generates_real_cxx_package() {
    let out_dir = temp_dir("outputs");

    let summary =
        generate_cpp_bindings(&fixture_manifest(), &out_dir).expect("generate cxx package");

    assert_eq!(
        summary,
        CppBindingGenerationSummary {
            module_count: 2,
            function_count: 20,
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
        "native/rust/include/terlan_exception_adapters.hpp",
        "native/rust/cpp/native_boundary.cc",
        "native/rust/cpp/terlan_enum_adapters.cc",
        "native/rust/cpp/terlan_exception_adapters.cc",
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
    assert!(package_manifest.contains("helper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\""));

    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
    assert!(bridge.contains("#[cxx::bridge(namespace = \"terlan_fixture\")]"));
    assert!(bridge.contains("type NativeBoundary;"));
    assert!(bridge.contains("type NativeGauge;"));
    assert!(bridge.contains("fn make_native_boundary(value: i64) -> UniquePtr<NativeBoundary>;"));
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

    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("cxx = \"=1.0.197\""));
    assert!(cargo.contains("getrandom = \"=0.3.4\""));
    assert!(cargo.contains("cxx-build = \"=1.0.197\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("cxx_build::bridge(root.join(\"src/lib.rs\"))"));
    assert!(build.contains("cpp/native_boundary.cc"));
    assert!(build.contains("build.std(\"c++14\")"));
    assert!(build.contains("build.include(root.join(\"include\"))"));
    assert!(build.contains("build.define(\"TERLAN_CPP_FIXTURE\", Some(\"1\"))"));
    assert!(build.contains("build.file(\"cpp/terlan_enum_adapters.cc\")"));
    assert!(build.contains("build.file(\"cpp/terlan_exception_adapters.cc\")"));

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
    assert!(source.contains("pub type Raw."));
    assert!(source.contains("pub type Doubled."));
    assert!(source.contains("pub type Offset."));
    assert!(!source.contains("pub type Raw = Atom[\"raw\"]."));
    assert!(!source.contains("pub type Doubled = Atom[\"doubled\"]."));
    assert!(!source.contains("pub type Offset = Atom[\"offset\"]."));
    assert!(source.contains("pub type BoundaryMode = Raw | Doubled | Offset."));
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
    assert!(generated_test
        .contains("import cpp_fixture.NativeBoundary.{Doubled, NativeSnapshot, Offset, Raw,"));
    assert!(generated_test
        .contains("copied_mode == Raw or copied_mode == Doubled or copied_mode == Offset"));
    assert!(!generated_test.contains("Atom[\"raw\"]"));
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
fn committed_clang_metadata_is_consumed_offline_with_unsafe_facts_visible() {
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
}

/// Compiles one generated C++ adapter and exercises its bounded public protocol.
fn compile_and_exercise_generated_cpp_adapter(out_dir: &Path, target_dir: &Path) -> PathBuf {
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
    super::execution_test::assert_generated_helper_replies(&helper);
    helper
}

#[test]
fn generated_cxx_adapter_compiles_and_enforces_public_protocol() {
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
fn generated_cxx_bridge_compiles_links_owns_and_executes_from_terlan() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("end_to_end");
    let target_dir = temp_dir("end_to_end_target");
    generate_cpp_bindings(&fixture_manifest(), &out_dir).expect("generate cxx package");
    let helper = compile_and_exercise_generated_cpp_adapter(&out_dir, &target_dir);

    let previous_helper = std::env::var_os("TERLAN_NATIVE_BOUNDARY_HELPER_PATH");
    std::env::set_var("TERLAN_NATIVE_BOUNDARY_HELPER_PATH", &helper);
    let exit_code = crate::commands::test::run(
        crate::CliCommand {
            verb: Some("test".to_string()),
            args: vec![out_dir.join("tests").to_string_lossy().into_owned()],
        },
        crate::CliState::default(),
    );
    if let Some(previous_helper) = previous_helper {
        std::env::set_var("TERLAN_NATIVE_BOUNDARY_HELPER_PATH", previous_helper);
    } else {
        std::env::remove_var("TERLAN_NATIVE_BOUNDARY_HELPER_PATH");
    }
    assert_eq!(exit_code, ExitCode::SUCCESS);

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}

#[test]
fn skipped_symbols_are_sorted_and_cover_every_required_rejection_family() {
    let first = temp_dir("stable_first");
    let second = temp_dir("stable_second");
    generate_cpp_bindings(&fixture_manifest(), &first).expect("first generation");
    generate_cpp_bindings(&fixture_manifest(), &second).expect("second generation");

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
    for entry in skipped["skipped"].as_array().expect("skip array") {
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
fn bindable_unsafe_cpp_shapes_fail_with_stable_diagnostic_families() {
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
        ("overload", "cpp.overload.ambiguous", |metadata| {
            symbol_mut(metadata, "function.make_native_boundary")["overload_candidates"] =
                Value::Number(2.into());
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
fn copied_container_mappings_reject_incompatible_public_types() {
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
fn symbolic_enum_mappings_reject_unknown_duplicate_and_non_enum_shapes() {
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
fn mapping_and_metadata_provenance_are_required_before_generation() {
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
fn structured_cpp_contract_requires_complete_type_and_compile_facts() {
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
fn package_policy_is_complete_unique_and_separate_from_extracted_metadata() {
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

#[test]
fn a_second_package_neutral_surface_generates_independent_names() {
    let manifest = write_fixture_variant("second_namespace", |manifest| {
        manifest["package"]["namespace"] = Value::String("counter_fixture".into());
        manifest["package"]["crate_name"] = Value::String("terlan-counter-fixture".into());
        manifest["cpp_metadata"]["namespace"] = Value::String("counter_fixture_native".into());
        symbol_mut(manifest, "record.native_boundary")["cpp_name"] =
            Value::String("Counter".into());
        symbol_mut(manifest, "function.make_native_boundary")["cpp_name"] =
            Value::String("make_counter".into());
        symbol_mut(manifest, "function.make_native_boundary")["returns"]["spelling"] =
            Value::String("std::unique_ptr<Counter>".into());
        symbol_mut(manifest, "function.make_native_boundary")["returns"]["canonical"] =
            Value::String("std::unique_ptr<Counter>".into());
        for id in [
            "method.native_boundary.value",
            "method.native_boundary.doubled",
            "method.native_boundary.label",
            "method.native_boundary.bytes",
            "method.native_boundary.samples",
            "method.native_boundary.mode",
            "method.native_boundary.tripled_or_throw",
            "method.native_boundary.add",
        ] {
            symbol_mut(manifest, id)["receiver"] = Value::String("Counter".into());
        }
        manifest["modules"][0]["module"] = Value::String("counter_fixture.Counter".into());
        manifest["modules"][0]["types"][0]["name"] = Value::String("Counter".into());
        for function in manifest["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
        {
            let operation = function["operation"].as_str().expect("operation");
            function["operation"] = Value::String(
                operation.replace("cpp_fixture.native_boundary", "counter_fixture.counter"),
            );
            for arg in function["args"].as_array_mut().expect("args") {
                if arg["ty"] == "NativeBoundary" {
                    arg["ty"] = Value::String("Counter".into());
                }
            }
            if function["returns"] == "NativeBoundary" {
                function["returns"] = Value::String("Counter".into());
            }
        }
    });
    let out_dir = temp_dir("second_namespace_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate independent package");

    let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(bridge.contains("namespace = \"counter_fixture_native\""));
    assert!(bridge.contains("type Counter;"));
    assert!(bridge.contains("UniquePtr<Counter>"));
    assert!(helper.contains("cxx::UniquePtr<ffi::Counter>"));
    assert!(out_dir.join("src/counter_fixture/Counter.terl").is_file());
    assert!(out_dir
        .join("tests/counter_fixture/CounterTest.terl")
        .is_file());

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn structured_metadata_rejects_unmaintained_producer_and_unknown_skip_shape() {
    let producer_manifest = write_fixture_variant("producer", |metadata| {
        metadata["cpp_metadata"]["producer"]["name"] = Value::String("handwritten".into());
    });
    let error = generate_cpp_bindings(&producer_manifest, &temp_dir("producer_out"))
        .expect_err("producer must fail");
    assert!(error.contains("expected maintained tooling `clang-libtooling`"));
    fs::remove_dir_all(producer_manifest.parent().expect("producer root"))
        .expect("remove producer variant");

    let shape_manifest = write_fixture_variant("shape", |metadata| {
        policy_mut(metadata, "unsupported.template")["rejection"]["shape"] =
            Value::String("free_form_reason".into());
    });
    let error = generate_cpp_bindings(&shape_manifest, &temp_dir("shape_out"))
        .expect_err("unknown shape must fail");
    assert!(error.contains("unknown variant `free_form_reason`"));
    fs::remove_dir_all(shape_manifest.parent().expect("shape root")).expect("remove shape variant");
}
