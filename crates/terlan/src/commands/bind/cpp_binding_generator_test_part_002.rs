
#[test]
fn structured_build_plan_renders_without_shell_commands() {
    let manifest = write_fixture_variant("build_plan", |manifest| {
        manifest["build"]["adapter_headers"] = serde_json::json!(["support.hpp"]);
        manifest["build"]["library_search_paths"] = serde_json::json!(["vendor/lib"]);
        manifest["build"]["linked_libraries"] = serde_json::json!([
            {"name": "fixture_core", "kind": "static"}
        ]);
        manifest["build"]["platform_conditions"] = serde_json::json!([
            {
                "target_os": "linux",
                "target_arch": "x86_64",
                "include_roots": ["platform/linux/include"],
                "defines": {"TERLAN_LINUX": null},
                "library_search_paths": ["platform/linux/lib"],
                "linked_libraries": [{"name": "pthread", "kind": "dynamic"}]
            }
        ]);
        manifest["build"]["rebuild_inputs"] =
            serde_json::json!(["src/lib.rs", "include", "cpp", "vendor/lib"]);
    });
    fs::write(
        manifest.parent().expect("variant root").join("support.hpp"),
        "#pragma once\n",
    )
    .expect("write support header");
    let out_dir = temp_dir("build_plan_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate structured build plan");

    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("cargo:rustc-link-search=native={}"));
    assert!(build.contains("root.join(\"vendor/lib\").display()"));
    assert!(build.contains("cargo:rustc-link-lib=static=fixture_core"));
    assert!(build.contains("CARGO_CFG_TARGET_OS"));
    assert!(build.contains("CARGO_CFG_TARGET_ARCH"));
    assert!(build.contains("build.define(\"TERLAN_LINUX\", None::<&str>)"));
    assert!(build.contains("cargo:rustc-link-lib=dylib=pthread"));
    assert!(build.contains("cargo:rerun-if-changed=vendor/lib"));
    assert!(!build.contains("Command::new"));
    assert!(!build.contains("sh -c"));
    assert_eq!(
        fs::read_to_string(out_dir.join("native/rust/include/support.hpp"))
            .expect("copied support header"),
        "#pragma once\n"
    );

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn structured_build_plan_rejects_unsafe_or_ambiguous_tokens() {
    let cases = [
        (
            "build_parent",
            "library_search_paths",
            serde_json::json!(["../outside"]),
            "generated adapter root",
        ),
        (
            "build_newline",
            "rebuild_inputs",
            serde_json::json!(["cpp\ncargo:rustc-link-lib=evil"]),
            "generated adapter root",
        ),
        (
            "build_absolute",
            "include_roots",
            serde_json::json!(["/usr/include"]),
            "generated adapter root",
        ),
    ];
    for (name, field, value, expected) in cases {
        let manifest = write_fixture_variant(name, |manifest| {
            manifest["build"][field] = value;
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("unsafe build token must fail");
        assert!(error.contains(expected), "unexpected error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }

    let unsafe_header = write_fixture_variant("build_header_parent", |manifest| {
        manifest["build"]["adapter_headers"] = serde_json::json!(["../support.hpp"]);
    });
    let error = generate_cpp_bindings(&unsafe_header, &temp_dir("build_header_parent_out"))
        .expect_err("unsafe adapter header must fail");
    assert!(error.contains("generated adapter root"));
    fs::remove_dir_all(unsafe_header.parent().expect("variant root")).expect("remove variant");

    let missing_header = write_fixture_variant("build_header_missing", |manifest| {
        manifest["build"]["adapter_headers"] = serde_json::json!(["missing.hpp"]);
    });
    let error = generate_cpp_bindings(&missing_header, &temp_dir("build_header_missing_out"))
        .expect_err("missing adapter header must fail");
    assert!(error.contains("does not exist"));
    fs::remove_dir_all(missing_header.parent().expect("variant root")).expect("remove variant");

    let colliding_header = write_fixture_variant("build_header_collision", |manifest| {
        manifest["build"]["adapter_headers"] = serde_json::json!(["nested/native_boundary.hpp"]);
    });
    let collision_root = colliding_header.parent().expect("variant root");
    fs::create_dir(collision_root.join("nested")).expect("create nested fixture directory");
    fs::write(
        collision_root.join("nested/native_boundary.hpp"),
        "#pragma once\n",
    )
    .expect("write colliding support header");
    let error = generate_cpp_bindings(&colliding_header, &temp_dir("build_header_collision_out"))
        .expect_err("colliding adapter header must fail");
    assert!(error.contains("duplicate generated C++ adapter header filename"));
    fs::remove_dir_all(collision_root).expect("remove variant");

    let invalid_library = write_fixture_variant("build_library", |manifest| {
        manifest["build"]["linked_libraries"] =
            serde_json::json!([{"name": "core;rm", "kind": "dynamic"}]);
    });
    let error = generate_cpp_bindings(&invalid_library, &temp_dir("build_library_out"))
        .expect_err("invalid library must fail");
    assert!(error.contains("invalid C++ linked library"));
    fs::remove_dir_all(invalid_library.parent().expect("variant root")).expect("remove variant");

    let duplicate_condition = write_fixture_variant("build_condition", |manifest| {
        manifest["build"]["platform_conditions"] = serde_json::json!([
            {"target_os": "linux"},
            {"target_os": "linux"}
        ]);
    });
    let error = generate_cpp_bindings(&duplicate_condition, &temp_dir("build_condition_out"))
        .expect_err("duplicate condition must fail");
    assert!(error.contains("duplicate C++ platform condition"));
    fs::remove_dir_all(duplicate_condition.parent().expect("variant root"))
        .expect("remove variant");
}

#[test]
fn package_null_failures_use_a_hidden_finite_native_status_probe() {
    let manifest = write_fixture_variant("null_failure", |manifest| {
        let mut probe = manifest["cpp_metadata"]["symbols"]
            .as_array()
            .expect("symbols")
            .iter()
            .find(|symbol| symbol["id"] == "function.live_native_boundary_count")
            .expect("integer function")
            .clone();
        probe["id"] = Value::String("function.hidden_null_failure".into());
        probe["cpp_name"] = Value::String("hidden_null_failure".into());
        probe["overload_set"] = Value::String("terlan_fixture::hidden_null_failure".into());
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(probe);
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping")
            .push(serde_json::json!({
                "symbol": "function.hidden_null_failure",
                "disposition": "bind"
            }));
        manifest["null_failure"] = serde_json::json!({
            "probe_symbol": "function.hidden_null_failure",
            "cases": [
                {
                    "value": 1,
                    "code": "fixture.invalid_argument",
                    "message": "Fixture arguments are invalid."
                },
                {
                    "value": 2,
                    "code": "fixture.native_exception",
                    "message": "Fixture native operation failed."
                }
            ],
            "fallback": {
                "code": "fixture.native_failure",
                "message": "Fixture native operation failed without classification."
            }
        });
    });
    let out_dir = temp_dir("null_failure_out");
    generate_cpp_bindings(&manifest, &out_dir).expect("generate finite null failures");

    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("generated helper");
    assert!(helper.contains("match ffi::hidden_null_failure()"));
    assert!(helper.contains("fixture.invalid_argument"));
    assert!(helper.contains("fixture.native_exception"));
    assert!(helper.contains("fixture.native_failure"));
    assert!(helper.contains("return native_null_failure(\"constructor returned null\")"));
    assert!(!helper.contains("native exception payload"));
    let module = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeBoundary.terl"))
        .expect("generated module");
    assert!(!module.contains("hidden_null_failure"));
    let metadata =
        fs::read_to_string(out_dir.join("native/terlan-native.toml")).expect("native metadata");
    assert!(metadata.contains("null_failure = \"finite_status_probe\""));
    assert!(metadata.contains("native_failure_payloads = false"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn package_null_failure_policy_rejects_untrusted_or_ambiguous_classification() {
    let cases = [
        (
            "unknown_probe",
            serde_json::json!({
                "probe_symbol": "function.missing",
                "cases": [{"value": 1, "code": "fixture.invalid", "message": "Invalid."}],
                "fallback": {"code": "fixture.failure", "message": "Failure."}
            }),
            "unknown symbol",
        ),
        (
            "public_probe",
            serde_json::json!({
                "probe_symbol": "function.live_native_boundary_count",
                "cases": [{"value": 1, "code": "fixture.invalid", "message": "Invalid."}],
                "fallback": {"code": "fixture.failure", "message": "Failure."}
            }),
            "must remain hidden",
        ),
        (
            "duplicate_status",
            serde_json::json!({
                "probe_symbol": "function.live_native_gauge_count",
                "cases": [
                    {"value": 1, "code": "fixture.first", "message": "First."},
                    {"value": 1, "code": "fixture.second", "message": "Second."}
                ],
                "fallback": {"code": "fixture.failure", "message": "Failure."}
            }),
            "duplicate C++ null failure status",
        ),
        (
            "invalid_code",
            serde_json::json!({
                "probe_symbol": "function.live_native_gauge_count",
                "cases": [{"value": 1, "code": "Injected Code", "message": "Invalid."}],
                "fallback": {"code": "fixture.failure", "message": "Failure."}
            }),
            "invalid package error code",
        ),
    ];
    for (name, policy, expected) in cases {
        let hide_gauge_probe =
            policy["probe_symbol"] == Value::String("function.live_native_gauge_count".into());
        let manifest = write_fixture_variant(name, |manifest| {
            manifest["null_failure"] = policy;
            if hide_gauge_probe {
                for module in manifest["modules"].as_array_mut().expect("modules") {
                    module["functions"]
                        .as_array_mut()
                        .expect("functions")
                        .retain(|function| {
                            function["cpp_symbol"] != "function.live_native_gauge_count"
                        });
                }
            }
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("unsafe null failure policy must fail");
        assert!(error.contains(expected), "unexpected error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
fn every_opaque_resource_requires_a_producer_and_independent_disposer() {
    for (name, role, expected) in [
        ("missing_gauge_producer", "constructor", "producer"),
        ("missing_gauge_disposer", "dispose", "disposer"),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            let functions = manifest["modules"][1]["functions"]
                .as_array_mut()
                .expect("second resource functions");
            functions.retain(|function| function["role"] != role);
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("incomplete resource lifecycle must fail");
        assert!(error.contains("cpp_fixture.NativeGauge.NativeGauge"));
        assert!(error.contains(expected), "unexpected error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
fn opaque_resource_accepts_a_reviewed_cross_module_producer() {
    let manifest = write_fixture_variant("cross_module_resource_producer", |manifest| {
        let modules = manifest["modules"].as_array_mut().expect("modules");
        let gauge_functions = modules[1]["functions"]
            .as_array_mut()
            .expect("gauge functions");
        let producer_index = gauge_functions
            .iter()
            .position(|function| function["role"] == "constructor")
            .expect("gauge producer");
        let mut producer = gauge_functions.remove(producer_index);
        producer["role"] = Value::String("free_function".into());
        producer["returns"] = Value::String("cpp_fixture.NativeGauge.NativeGauge".into());
        modules[0]["functions"]
            .as_array_mut()
            .expect("boundary functions")
            .push(producer);
    });
    let out_dir = temp_dir("cross_module_resource_producer_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("qualified cross-module producer must generate");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read generated helper");
    assert!(helper.contains("CppFixtureNativeGaugeNativeGauge(result)"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove generated output");
}

#[test]
fn owned_value_projection_accepts_a_reviewed_cross_module_record() {
    let manifest = write_fixture_variant("cross_module_owned_record", |manifest| {
        let modules = manifest["modules"].as_array_mut().expect("modules");
        let function_index = modules[0]["functions"]
            .as_array()
            .expect("boundary functions")
            .iter()
            .position(|function| function["name"] == "owned_snapshot")
            .expect("owned snapshot function");
        let mut function = modules[0]["functions"]
            .as_array_mut()
            .expect("boundary functions")
            .remove(function_index);
        function["returns"] = Value::String("cpp_fixture.NativeBoundary.NativeSnapshot".into());
        modules[1]["functions"]
            .as_array_mut()
            .expect("gauge functions")
            .push(function);
    });
    let out_dir = temp_dir("cross_module_owned_record_out");
    generate_cpp_bindings(&manifest, &out_dir)
        .expect("qualified cross-module owned record must generate");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read generated helper");
    assert!(helper.contains("cpp_fixture.native_boundary.owned_snapshot"));
    assert!(helper.contains("value.projected_value()"));
    let gauge = fs::read_to_string(out_dir.join("src/cpp_fixture/NativeGauge.terl"))
        .expect("read generated operation owner");
    assert!(gauge.contains("pub owned_snapshot"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove generated output");
}

#[test]
fn copied_value_records_require_complete_reviewed_field_projections() {
    let wrong_ownership = write_fixture_variant("record_ownership", |manifest| {
        policy_mut(manifest, "record.native_snapshot")["ownership"] =
            Value::String("unique".into());
    });
    let error = generate_cpp_bindings(&wrong_ownership, &temp_dir("record_ownership_out"))
        .expect_err("non-copied record policy must fail");
    assert!(error.contains("requires copied ownership policy"));
    fs::remove_dir_all(wrong_ownership.parent().expect("variant root")).expect("remove variant");

    let missing_field = write_fixture_variant("record_field", |manifest| {
        symbol_mut(manifest, "record.native_snapshot")["fields"]
            .as_array_mut()
            .expect("record fields")
            .pop();
    });
    let error = generate_cpp_bindings(&missing_field, &temp_dir("record_field_out"))
        .expect_err("missing extracted field must fail");
    assert!(error.contains("unknown C++ field `doubled`"));
    fs::remove_dir_all(missing_field.parent().expect("variant root")).expect("remove variant");

    let duplicate_projection = write_fixture_variant("record_projection", |manifest| {
        let projections = manifest["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .iter_mut()
            .find(|function| function["role"] == "value_projection")
            .expect("projection function")["projections"]
            .as_array_mut()
            .expect("projections");
        projections[1]["field"] = Value::String("value".into());
    });
    let error = generate_cpp_bindings(&duplicate_projection, &temp_dir("record_projection_out"))
        .expect_err("duplicate field projection must fail");
    assert!(error.contains("duplicates field `value`"));
    fs::remove_dir_all(duplicate_projection.parent().expect("variant root"))
        .expect("remove variant");

    let wrong_receiver = write_fixture_variant("record_receiver", |manifest| {
        let projections = manifest["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .iter_mut()
            .find(|function| function["role"] == "value_projection")
            .expect("projection function")["projections"]
            .as_array_mut()
            .expect("projections");
        projections[1]["cpp_symbol"] = Value::String("method.native_gauge.reading".into());
    });
    let error = generate_cpp_bindings(&wrong_receiver, &temp_dir("record_receiver_out"))
        .expect_err("getter from another C++ receiver must fail");
    assert!(error.contains("requires a bindable zero-argument Int getter"));
    fs::remove_dir_all(wrong_receiver.parent().expect("variant root")).expect("remove variant");
}

#[test]
fn copied_record_inputs_require_complete_ordered_typed_parameter_mappings() {
    let cases: &[(&str, &str, fn(&mut Value))] = &[
        (
            "record_input_missing",
            "must map all 2 fields",
            |manifest| {
                function_mut(manifest, "sum_snapshot")["args"][0]["fields"]
                    .as_array_mut()
                    .expect("argument fields")
                    .pop();
            },
        ),
        (
            "record_input_duplicate",
            "maps field `value` more than once",
            |manifest| {
                function_mut(manifest, "sum_snapshot")["args"][0]["fields"][1]["field"] =
                    Value::String("value".into());
            },
        ),
        (
            "record_input_order",
            "must map the next extracted C++ parameter `value`, not `doubled`",
            |manifest| {
                function_mut(manifest, "sum_snapshot")["args"][0]["fields"][0]["cpp_parameter"] =
                    Value::String("doubled".into());
            },
        ),
        (
            "record_input_non_record",
            "is not a copied value record",
            |manifest| {
                function_mut(manifest, "sum_snapshot")["args"][0]["ty"] =
                    Value::String("Int".into());
            },
        ),
        (
            "record_input_type",
            "record_argument_mapping_mismatch",
            |manifest| {
                symbol_mut(manifest, "function.sum_snapshot_fields")["parameters"][0]["ty"]
                    ["spelling"] = Value::String("bool".into());
                symbol_mut(manifest, "function.sum_snapshot_fields")["parameters"][0]["ty"]
                    ["canonical"] = Value::String("bool".into());
            },
        ),
    ];
    for (name, expected, mutate) in cases {
        let manifest = write_fixture_variant(name, *mutate);
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("invalid copied record input must fail");
        assert!(error.contains(expected), "unexpected diagnostic: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}

#[test]
fn generated_helpers_lower_only_reviewed_enum_argument_atoms() {
    let manifest = write_fixture_variant("enum_argument", |manifest| {
        let add = manifest["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .iter_mut()
            .find(|function| function["name"] == "add")
            .expect("add function");
        add["args"][1]["name"] = Value::String("mode".into());
        add["args"][1]["ty"] = Value::String("BoundaryMode".into());
    });
    let out_dir = temp_dir("enum_argument_out");

    generate_cpp_bindings(&manifest, &out_dir).expect("generate enum argument helper");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read generated helper");

    assert!(helper.contains("Arg::Atom(arg_1)"));
    assert!(helper.contains("\"raw\" => 7_i64"));
    assert!(helper.contains("\"doubled\" => 41_i64"));
    assert!(helper.contains("\"offset\" => 99_i64"));
    assert!(helper.contains("invalid_enum_value"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn owned_double_vectors_map_to_copied_float_lists() {
    let cpp_type: CppTypeMetadata = serde_json::from_value(serde_json::json!({
        "spelling": "std::unique_ptr<std::vector<double>>",
        "canonical": "std::unique_ptr<std::vector<double>>",
        "is_const": false,
        "pointer_depth": 0,
        "reference": "none",
        "function_pointer": false,
        "template_dependent": false
    }))
    .expect("double-vector metadata");
    assert!(is_owned_f64_vector_type(&cpp_type));
    assert_eq!(
        rust_bridge_type(&cpp_type).expect("double-vector bridge"),
        "UniquePtr<CxxVector<f64>>"
    );
}

#[test]
fn borrowed_numeric_slices_map_to_copied_list_arguments() {
    for (spelling, canonical, expected) in [
        (
            "rust::Slice<const std::int64_t>",
            "rust::Slice<const std::int64_t>",
            "&[i64]",
        ),
        (
            "rust::Slice<const double>",
            "rust::Slice<const double>",
            "&[f64]",
        ),
    ] {
        let cpp_type: CppTypeMetadata = serde_json::from_value(serde_json::json!({
            "spelling": spelling,
            "canonical": canonical,
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        }))
        .expect("numeric-slice metadata");
        assert!(is_supported_cxx_type(&cpp_type));
        assert_eq!(
            rust_bridge_type(&cpp_type).expect("numeric-slice bridge"),
            expected
        );
    }
}

#[test]
fn generated_helpers_decode_numeric_lists_and_typed_empty_lists() {
    for (name, terlan_type, spelling, expected_bridge, expected_pattern, expected_call) in [
        (
            "integer_slice",
            "List[Int]",
            "rust::Slice<const std::int64_t>",
            "fn sum_snapshot_fields(values: &[i64]) -> i64;",
            "arg_0 @ (Arg::Ints(_) | Arg::EmptyList)",
            "ffi::sum_snapshot_fields(arg_ints(arg_0))",
        ),
        (
            "float_slice",
            "List[Float]",
            "rust::Slice<const double>",
            "fn sum_snapshot_fields(values: &[f64]) -> i64;",
            "arg_0 @ (Arg::Floats(_) | Arg::EmptyList)",
            "ffi::sum_snapshot_fields(arg_floats(arg_0))",
        ),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            let symbol = symbol_mut(manifest, "function.sum_snapshot_fields");
            symbol["parameters"] = serde_json::json!([{
                "name": "values",
                "ty": {
                    "spelling": spelling,
                    "canonical": spelling,
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }]);
            let function = function_mut(manifest, "sum_snapshot");
            function["args"] = serde_json::json!([{
                "name": "values",
                "ty": terlan_type
            }]);
        });
        let out_dir = temp_dir(&format!("{name}_out"));
        generate_cpp_bindings(&manifest, &out_dir).expect("generate numeric-list input");

        let bridge = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("bridge");
        let helper =
            fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
                .expect("helper");
        assert!(bridge.contains(expected_bridge), "missing bridge: {bridge}");
        assert!(helper.contains(expected_pattern));
        assert!(helper.contains(expected_call));
        assert!(helper.contains("value.strip_prefix(\"li:\")"));
        assert!(helper.contains("value.strip_prefix(\"lf:\")"));
        assert!(helper.contains("value == \"ls:\""));

        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
        fs::remove_dir_all(out_dir).expect("remove output");
    }
}

#[test]
fn copied_numeric_lists_reject_mismatched_cpp_slice_elements() {
    for (name, terlan_type, spelling) in [
        (
            "integer_as_float_slice",
            "List[Int]",
            "rust::Slice<const double>",
        ),
        (
            "float_as_integer_slice",
            "List[Float]",
            "rust::Slice<const std::int64_t>",
        ),
    ] {
        let manifest = write_fixture_variant(name, |manifest| {
            let symbol = symbol_mut(manifest, "function.sum_snapshot_fields");
            symbol["parameters"] = serde_json::json!([{
                "name": "values",
                "ty": {
                    "spelling": spelling,
                    "canonical": spelling,
                    "is_const": false,
                    "pointer_depth": 0,
                    "reference": "none",
                    "function_pointer": false,
                    "template_dependent": false
                },
                "direction": "input"
            }]);
            let function = function_mut(manifest, "sum_snapshot");
            function["args"] = serde_json::json!([{
                "name": "values",
                "ty": terlan_type
            }]);
        });
        let error = generate_cpp_bindings(&manifest, &temp_dir(&format!("{name}_out")))
            .expect_err("mismatched numeric-list element must fail");
        assert!(error.contains("error[cpp.type.argument_mapping_mismatch]"));
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}
