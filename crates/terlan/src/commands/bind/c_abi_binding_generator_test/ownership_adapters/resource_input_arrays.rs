use super::*;

#[test]
fn c_abi_wrapper_supports_borrowed_opaque_resource_input_arrays() {
    let manifest = write_fixture_variant("resource_input_arrays", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_combine",
                "c_name": "terlan_c_native_boundary_combine",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "boundaries", "c_type": "const uint64_t *", "direction": "input", "ownership": "borrowed_call", "input_array": {"length_parameter": "boundary_count", "element_type": "NativeBoundary"}},
                    {"name": "boundary_count", "c_type": "int64_t", "direction": "input", "ownership": "value"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "combine",
                "operation": "c_abi_fixture.native_boundary.combine",
                "c_symbol": "function.native_boundary_combine",
                "role": "free_function",
                "args": [{"name": "boundaries", "ty": "List[NativeBoundary]"}],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Combines borrowed opaque resources without transferring ownership.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("resource_input_arrays_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate resource input array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains(
        "pub fn combine(boundaries: &[&NativeBoundary]) -> Result<NativeBoundary, CAbiError>"
    ));
    assert!(adapter.contains(
        "let boundaries_handles = boundaries.iter().map(|value| value.raw.as_ptr() as usize as u64).collect::<Vec<_>>()"
    ));
    assert!(adapter.contains("let boundaries_length = i64::try_from(boundaries.len())"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "ffi::terlan_c_native_boundary_combine(boundaries_handles.as_ptr(), boundaries_length, &mut raw)"
    ));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Handles(_) | Arg::EmptyList"));
    assert!(helper.contains("for handle in arg_handles(boundaries)"));
    assert!(helper.contains("value_boundaries.as_slice()"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub combine(boundaries: List[NativeBoundary]): NativeBoundary"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_supports_resource_lists_on_mutable_methods_without_unsafe_code() {
    let manifest = write_fixture_variant("mutable_resource_input_arrays", |metadata| {
        symbol_mut(metadata, "function.native_boundary_add")["parameters"][1] = serde_json::json!({
            "name": "boundaries",
            "c_type": "const uint64_t *",
            "direction": "input",
            "ownership": "borrowed_call",
            "input_array": {
                "length_parameter": "boundary_count",
                "element_type": "NativeBoundary"
            }
        });
        symbol_mut(metadata, "function.native_boundary_add")["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(serde_json::json!({
                "name": "boundary_count",
                "c_type": "int64_t",
                "direction": "input",
                "ownership": "value"
            }));
        metadata["modules"][0]["functions"][2]["args"][1] =
            serde_json::json!({"name": "boundaries", "ty": "List[NativeBoundary]"});
        metadata["modules"][0]["functions"][2]["generated_smoke"] =
            serde_json::Value::String("package_owned".to_string());
    });
    let out_dir = temp_dir("mutable_resource_input_arrays_output");

    generate_c_abi_bindings(&manifest, &out_dir)
        .expect("generate mutable resource input array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains(
        "pub fn add(&mut self, boundaries: &[&NativeBoundary]) -> Result<(), CAbiError>"
    ));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("#![forbid(unsafe_code)]"));
    assert!(!helper.contains("unsafe {"));
    assert!(helper.contains("mutable resource calls cannot borrow the receiver"));
    assert!(helper.contains("self.handles.remove(&boundary.id)"));
    assert!(helper.contains("let call_result = value_boundary.add(value_boundaries.as_slice())"));
    assert!(helper.contains("self.handles.insert(boundary.id, entry_boundary)"));
    assert!(helper.contains("match call_result"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_resource_input_arrays_require_known_matching_resource_types() {
    let manifest = write_fixture_variant("unknown_resource_input_array", |metadata| {
        symbol_mut(metadata, "function.native_boundary_add")["parameters"][1] = serde_json::json!({
            "name": "delta",
            "c_type": "const uint64_t *",
            "direction": "input",
            "ownership": "borrowed_call",
            "input_array": {
                "length_parameter": "delta_length",
                "element_type": "MissingResource"
            }
        });
        symbol_mut(metadata, "function.native_boundary_add")["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(serde_json::json!({
                "name": "delta_length",
                "c_type": "int64_t",
                "direction": "input",
                "ownership": "value"
            }));
        metadata["modules"][0]["functions"][2]["args"][1]["ty"] =
            serde_json::Value::String("List[MissingResource]".to_string());
    });
    let error =
        generate_c_abi_bindings(&manifest, &temp_dir("unknown_resource_input_array_output"))
            .expect_err("resource input arrays require known opaque types");

    assert!(error.contains("error[native_bindgen.c_input_array_contract]"));
    assert!(error.contains("unknown opaque-resource element type `MissingResource`"));
    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
}
