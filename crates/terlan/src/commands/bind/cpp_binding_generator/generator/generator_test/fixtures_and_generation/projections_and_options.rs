use super::*;

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
