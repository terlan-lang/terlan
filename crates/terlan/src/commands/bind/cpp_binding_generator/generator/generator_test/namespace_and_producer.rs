use super::*;

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
