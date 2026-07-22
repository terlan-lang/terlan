
#[test]
fn c_abi_wrapper_copies_owned_utf8_outputs_and_calls_the_named_destructor() {
    let manifest = write_fixture_variant("owned_string_output", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.delete_owned_string",
            "c_name": "terlan_c_delete_owned_string",
            "kind": "function",
            "status": "bind",
            "returns": "void",
            "error_model": "infallible",
            "parameters": [
                {"name": "value", "c_type": "char *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_label",
            "c_name": "terlan_c_native_boundary_label",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {
                    "name": "out_label",
                    "c_type": "char **",
                    "direction": "output",
                    "ownership": "transfer_full",
                    "owned_string": {
                        "length_parameter": "out_length",
                        "destructor_symbol": "function.delete_owned_string",
                        "copy": "immediate_utf8"
                    }
                },
                {"name": "out_length", "c_type": "size_t *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "label",
                "operation": "c_abi_fixture.native_boundary.label",
                "c_symbol": "function.native_boundary_label",
                "role": "immutable_method",
                "args": [{"name": "boundary", "ty": "NativeBoundary"}],
                "returns": "String",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Copies an owned native UTF-8 label.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("owned_string_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate owned-string wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn label(&self) -> Result<String, CAbiError>"));
    assert!(adapter.contains("let mut out_out_label: *mut std::ffi::c_char"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "std::slice::from_raw_parts(pointer.as_ptr().cast::<u8>()"
    ));
    assert!(adapter.contains("ffi::terlan_c_delete_owned_string(out_out_label)"));
    assert!(adapter.contains("String::from_utf8(bytes)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("ok_string {}"));
    assert!(helper.contains("STANDARD.encode(value.as_bytes())"));

    let mut malformed: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("read owned-string manifest"))
            .expect("parse owned-string manifest");
    symbol_mut(&mut malformed, "function.native_boundary_label")["parameters"][2]["c_type"] =
        Value::String("int64_t *".to_string());
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&malformed).expect("render malformed owned string"),
    )
    .expect("write malformed owned string");
    let error = generate_c_abi_bindings(
        &manifest,
        &temp_dir("malformed_owned_string_output_generated"),
    )
    .expect_err("owned strings require a size_t byte length");
    assert!(error.contains("error[native_bindgen.c_owned_string_contract]"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_copies_owned_int_arrays_and_calls_the_named_destructor() {
    let manifest = write_fixture_variant("owned_int_array_output", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.delete_owned_int_array",
            "c_name": "terlan_c_delete_owned_int_array",
            "kind": "function",
            "status": "bind",
            "returns": "void",
            "error_model": "infallible",
            "parameters": [
                {"name": "values", "c_type": "int64_t *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_indices",
            "c_name": "terlan_c_native_boundary_indices",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {
                    "name": "out_indices",
                    "c_type": "int64_t **",
                    "direction": "output",
                    "ownership": "transfer_full",
                    "owned_array": {
                        "length_parameter": "out_length",
                        "destructor_symbol": "function.delete_owned_int_array",
                        "copy": "immediate"
                    }
                },
                {"name": "out_length", "c_type": "size_t *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "indices",
                "operation": "c_abi_fixture.native_boundary.indices",
                "c_symbol": "function.native_boundary_indices",
                "role": "immutable_method",
                "args": [{"name": "boundary", "ty": "NativeBoundary"}],
                "returns": "List[Int]",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Copies an owned native integer array.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("owned_int_array_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate owned-array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn indices(&self) -> Result<Vec<i64>, CAbiError>"));
    assert!(adapter.contains("let mut out_out_indices: *mut i64"));
    assert!(
        adapter.contains("std::slice::from_raw_parts(pointer.as_ptr(), out_out_indices_length)")
    );
    assert!(adapter.contains("ffi::terlan_c_delete_owned_int_array(out_out_indices)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("ok_ints {}"));

    let mut malformed: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("read owned-array manifest"))
            .expect("parse owned-array manifest");
    symbol_mut(&mut malformed, "function.native_boundary_indices")["parameters"][2]["c_type"] =
        Value::String("int64_t *".to_string());
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&malformed).expect("render malformed owned array"),
    )
    .expect("write malformed owned array");
    let error = generate_c_abi_bindings(
        &manifest,
        &temp_dir("malformed_owned_int_array_output_generated"),
    )
    .expect_err("owned arrays require a size_t element count");
    assert!(error.contains("error[native_bindgen.c_owned_array_contract]"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_copies_owned_float_arrays_and_calls_the_named_destructor() {
    let manifest = write_fixture_variant("owned_float_array_output", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.delete_owned_float_array",
            "c_name": "terlan_c_delete_owned_float_array",
            "kind": "function",
            "status": "bind",
            "returns": "void",
            "error_model": "infallible",
            "parameters": [
                {"name": "values", "c_type": "double *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_scores",
            "c_name": "terlan_c_native_boundary_scores",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {
                    "name": "out_scores",
                    "c_type": "double **",
                    "direction": "output",
                    "ownership": "transfer_full",
                    "owned_array": {
                        "length_parameter": "out_length",
                        "destructor_symbol": "function.delete_owned_float_array",
                        "copy": "immediate",
                        "element": "float64"
                    }
                },
                {"name": "out_length", "c_type": "size_t *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "scores",
                "operation": "c_abi_fixture.native_boundary.scores",
                "c_symbol": "function.native_boundary_scores",
                "role": "immutable_method",
                "args": [{"name": "boundary", "ty": "NativeBoundary"}],
                "returns": "List[Float]",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Copies an owned native float array.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("owned_float_array_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate owned float-array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn scores(&self) -> Result<Vec<f64>, CAbiError>"));
    assert!(adapter.contains("let mut out_out_scores: *mut f64"));
    assert!(adapter.contains("std::slice::from_raw_parts(pointer.as_ptr(), out_out_scores_length)"));
    assert!(adapter.contains("ffi::terlan_c_delete_owned_float_array(out_out_scores)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("ok_floats {}"));

    let mut malformed: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("read owned-array manifest"))
            .expect("parse owned-array manifest");
    symbol_mut(&mut malformed, "function.delete_owned_float_array")["parameters"][0]["c_type"] =
        Value::String("int64_t *".to_string());
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&malformed).expect("render malformed owned float array"),
    )
    .expect("write malformed owned float array");
    let error = generate_c_abi_bindings(
        &manifest,
        &temp_dir("malformed_owned_float_array_output_generated"),
    )
    .expect_err("owned float arrays require a matching consuming destructor");
    assert!(error.contains("error[native_bindgen.c_owned_array_contract]"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_validates_and_copies_owned_bool_arrays() {
    let manifest = write_fixture_variant("owned_bool_array_output", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.delete_owned_bool_array",
            "c_name": "terlan_c_delete_owned_bool_array",
            "kind": "function",
            "status": "bind",
            "returns": "void",
            "error_model": "infallible",
            "parameters": [
                {"name": "values", "c_type": "uint8_t *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_flags",
            "c_name": "terlan_c_native_boundary_flags",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {
                    "name": "out_flags",
                    "c_type": "uint8_t **",
                    "direction": "output",
                    "ownership": "transfer_full",
                    "owned_array": {
                        "length_parameter": "out_length",
                        "destructor_symbol": "function.delete_owned_bool_array",
                        "copy": "immediate",
                        "element": "bool8"
                    }
                },
                {"name": "out_length", "c_type": "size_t *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "flags",
                "operation": "c_abi_fixture.native_boundary.flags",
                "c_symbol": "function.native_boundary_flags",
                "role": "immutable_method",
                "args": [{"name": "boundary", "ty": "NativeBoundary"}],
                "returns": "List[Bool]",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Copies a validated owned native boolean array.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("owned_bool_array_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate owned bool-array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn flags(&self) -> Result<Vec<bool>, CAbiError>"));
    assert!(adapter.contains("let mut out_out_flags: *mut u8"));
    assert!(adapter.contains("0 => Ok(false)"));
    assert!(adapter.contains("1 => Ok(true)"));
    assert!(adapter.contains("status: -4"));
    assert!(adapter.contains("ffi::terlan_c_delete_owned_bool_array(out_out_flags)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("ok_bools {}"));

    let mut malformed: Value =
        serde_json::from_str(&fs::read_to_string(&manifest).expect("read owned-array manifest"))
            .expect("parse owned-array manifest");
    symbol_mut(&mut malformed, "function.native_boundary_flags")["parameters"][1]["c_type"] =
        Value::String("bool **".to_string());
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&malformed).expect("render malformed owned bool array"),
    )
    .expect("write malformed owned bool array");
    let error = generate_c_abi_bindings(
        &manifest,
        &temp_dir("malformed_owned_bool_array_output_generated"),
    )
    .expect_err("owned boolean arrays require the portable uint8 encoding");
    assert!(error.contains("error[native_bindgen.c_owned_array_contract]"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_copies_owned_utf8_string_arrays_and_calls_the_named_destructor() {
    let manifest = write_fixture_variant("owned_string_array_output", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.delete_owned_string_array",
            "c_name": "terlan_c_delete_owned_string_array",
            "kind": "function",
            "status": "bind",
            "returns": "void",
            "error_model": "infallible",
            "parameters": [
                {"name": "values", "c_type": "char **", "direction": "input", "ownership": "transfer_full"},
                {"name": "lengths", "c_type": "size_t *", "direction": "input", "ownership": "transfer_full"},
                {"name": "count", "c_type": "size_t", "direction": "input", "ownership": "value"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_labels",
            "c_name": "terlan_c_native_boundary_labels",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {
                    "name": "out_labels",
                    "c_type": "char ***",
                    "direction": "output",
                    "ownership": "transfer_full",
                    "owned_string_array": {
                        "lengths_parameter": "out_lengths",
                        "count_parameter": "out_count",
                        "destructor_symbol": "function.delete_owned_string_array",
                        "copy": "immediate_utf8"
                    }
                },
                {"name": "out_lengths", "c_type": "size_t **", "direction": "output", "ownership": "transfer_full"},
                {"name": "out_count", "c_type": "size_t *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "labels",
                "operation": "c_abi_fixture.native_boundary.labels",
                "c_symbol": "function.native_boundary_labels",
                "role": "immutable_method",
                "args": [{"name": "boundary", "ty": "NativeBoundary"}],
                "returns": "List[String]",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Copies an owned array of length-delimited native UTF-8 strings.",
                "generated_smoke": "package_owned"
            }));
    });
    let out_dir = temp_dir("owned_string_array_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate owned string-array wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn labels(&self) -> Result<Vec<String>, CAbiError>"));
    assert!(adapter.contains("let mut out_out_labels: *mut *mut std::ffi::c_char"));
    assert!(adapter.contains("let mut out_out_labels_lengths: *mut usize"));
    assert!(adapter.contains("String::from_utf8(value)"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "ffi::terlan_c_delete_owned_string_array( out_out_labels, out_out_labels_lengths, out_out_labels_count, )"
    ));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("ok_strings {}"));
    assert!(helper.contains("STANDARD.encode(value.as_bytes())"));

    let mut malformed: Value = serde_json::from_str(
        &fs::read_to_string(&manifest).expect("read owned string-array manifest"),
    )
    .expect("parse owned string-array manifest");
    symbol_mut(&mut malformed, "function.native_boundary_labels")["parameters"][2]["c_type"] =
        Value::String("int64_t **".to_string());
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&malformed).expect("render malformed owned string array"),
    )
    .expect("write malformed owned string array");
    let error = generate_c_abi_bindings(
        &manifest,
        &temp_dir("malformed_owned_string_array_output_generated"),
    )
    .expect_err("owned string arrays require size_t byte lengths");
    assert!(error.contains("error[native_bindgen.c_owned_string_array_contract]"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_supports_borrowed_int_input_arrays_with_linked_lengths() {
    let manifest = write_fixture_variant("int_input_arrays", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_reduce",
                "c_name": "terlan_c_native_boundary_reduce",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "dimensions", "c_type": "const int64_t *", "direction": "input", "ownership": "borrowed_call", "input_array": {"length_parameter": "dimensions_length"}},
                    {"name": "dimensions_length", "c_type": "int64_t", "direction": "input", "ownership": "value"},
                    {"name": "keep_dimension", "c_type": "int32_t", "direction": "input", "ownership": "value"},
                    {"name": "dtype", "c_type": "int32_t *", "direction": "input", "ownership": "borrowed_call", "fixed": {"kind": "int32", "value": 7}},
                    {"name": "layout", "c_type": "int32_t *", "direction": "input", "ownership": "borrowed_call", "fixed": {"kind": "null"}},
                    {"name": "device_index", "c_type": "int32_t", "direction": "input", "ownership": "value", "fixed": {"kind": "int32", "value": -1}},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "reduce",
                "operation": "c_abi_fixture.native_boundary.reduce",
                "c_symbol": "function.native_boundary_reduce",
                "role": "immutable_method",
                "args": [
                    {"name": "boundary", "ty": "NativeBoundary"},
                    {"name": "dimensions", "ty": "List[Int]"},
                    {"name": "keep_dimension", "ty": "Bool"}
                ],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Reduces over a copied integer dimension list."
            }));
    });
    let out_dir = temp_dir("int_input_arrays_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate input array C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains(
        "pub fn reduce(&self, dimensions: &[i64], keep_dimension: bool) -> Result<Self, CAbiError>"
    ));
    assert!(adapter.contains("let dimensions_length = i64::try_from(dimensions.len())"));
    assert!(adapter.contains("let mut fixed_dtype: i32 = 7"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "ffi::terlan_c_native_boundary_reduce(self.raw.as_ptr(), dimensions.as_ptr(), dimensions_length, keep_dimension as i32, &mut fixed_dtype, std::ptr::null_mut(), -1i32, &mut raw)"
    ));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Ints(_) | Arg::EmptyList"));
    assert!(helper.contains("arg_ints(dimensions)"));
    assert!(helper.contains("strip_prefix(\"li:\")"));
    assert!(helper.contains("value == \"ls:\""));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains(
        "pub reduce(boundary: NativeBoundary, dimensions: List[Int], keep_dimension: Bool): NativeBoundary"
    ));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_supports_borrowed_float_input_arrays_with_linked_lengths() {
    let manifest = write_fixture_variant("float_input_arrays", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_from_floats",
                "c_name": "terlan_c_native_boundary_from_floats",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "values", "c_type": "const double *", "direction": "input", "ownership": "borrowed_call", "input_array": {"length_parameter": "values_length"}},
                    {"name": "values_length", "c_type": "int64_t", "direction": "input", "ownership": "value"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "from_floats",
                "operation": "c_abi_fixture.native_boundary.from_floats",
                "c_symbol": "function.native_boundary_from_floats",
                "role": "constructor",
                "args": [{"name": "values", "ty": "List[Float]"}],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Copies floating-point values into a boundary."
            }));
    });
    let out_dir = temp_dir("float_input_arrays_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate float input array C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn from_floats(values: &[f64]) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("let values_length = i64::try_from(values.len())"));
    assert!(adapter.contains("values.as_ptr(), values_length, &mut raw"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Floats(_) | Arg::EmptyList"));
    assert!(helper.contains("arg_floats(values)"));
    assert!(helper.contains("strip_prefix(\"lf:\")"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub from_floats(values: List[Float]): NativeBoundary"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_wrapper_copies_bool_input_arrays_into_explicit_bytes() {
    let manifest = write_fixture_variant("bool_input_arrays", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_from_bools",
                "c_name": "terlan_c_native_boundary_from_bools",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "values", "c_type": "const uint8_t *", "direction": "input", "ownership": "borrowed_call", "input_array": {"length_parameter": "values_length"}},
                    {"name": "values_length", "c_type": "int64_t", "direction": "input", "ownership": "value"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "from_bools",
                "operation": "c_abi_fixture.native_boundary.from_bools",
                "c_symbol": "function.native_boundary_from_bools",
                "role": "constructor",
                "args": [{"name": "values", "ty": "List[Bool]"}],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Copies booleans into an explicit one-byte ABI representation."
            }));
    });
    let out_dir = temp_dir("bool_input_arrays_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate bool input array C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn from_bools(values: &[bool]) -> Result<Self, CAbiError>"));
    assert!(adapter
        .contains("let values_bytes = values.iter().copied().map(u8::from).collect::<Vec<_>>()"));
    assert!(adapter.contains("values_bytes.as_ptr(), values_length, &mut raw"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Bools(_) | Arg::EmptyList"));
    assert!(helper.contains("arg_bools(values)"));
    assert!(helper.contains("strip_prefix(\"lb:\")"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub from_bools(values: List[Bool]): NativeBoundary"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn c_abi_bool_input_arrays_require_bool_terlan_arguments() {
    let manifest = write_fixture_variant("bool_input_type_mismatch", |metadata| {
        symbol_mut(metadata, "function.native_boundary_add")["parameters"][1] = serde_json::json!({
            "name": "delta",
            "c_type": "const uint8_t *",
            "direction": "input",
            "ownership": "borrowed_call",
            "input_array": {"length_parameter": "delta_length"}
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
            serde_json::Value::String("List[Int]".to_string());
    });
    let error = generate_c_abi_bindings(&manifest, &temp_dir("bool_input_type_mismatch_output"))
        .expect_err("uint8_t input arrays require List[Bool]");

    assert!(error.contains("error[native_bindgen.c_input_array_contract]"));
    assert!(error.contains("requires `List[Bool]`"));
    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
}

/// Compiles one generated C adapter and exercises its bounded public protocol.
fn compile_and_exercise_generated_c_adapter(out_dir: &Path, target_dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = out_dir.join("native/rust/Cargo.toml");
    let test_output = std::process::Command::new(&cargo)
        .args(["test", "--manifest-path"])
        .arg(&manifest)
        .args(["--offline", "--quiet"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .expect("run generated C ABI tests");
    assert!(
        test_output.status.success(),
        "generated C ABI tests failed\nstdout:\n{}\nstderr:\n{}",
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
        "generated C ABI helper failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build_output.stdout),
        String::from_utf8_lossy(&build_output.stderr)
    );

    let helper = target_dir.join("debug/native-boundary-helper");
    assert!(
        helper.is_file(),
        "missing generated helper {}",
        helper.display()
    );
    let operation = |value: &str| STANDARD.encode(value);
    let handle_type = STANDARD.encode("c_abi_fixture.NativeBoundary.NativeBoundary");
    let wrong_handle_type = STANDARD.encode("other.Type");
    let mut requests = format!(
        "call 1 {} i:40\ncall 2 {} i:5\ncall 3 {} h:2:1:{handle_type}\ncall 4 {} h:1:1:{handle_type} h:2:1:{handle_type}\ncall 5 {} h:1:1:{handle_type} h:1:1:{wrong_handle_type}\ncall 6 {} i:9\ncall 7 {} h:2:1:{handle_type}\ncall 8 {} h:2:2:{handle_type}\ncall 9 {} h:2:2:{handle_type}\ncall 9 {} h:2:2:{handle_type}\n",
        operation("c_abi_fixture.native_boundary.new"),
        operation("c_abi_fixture.native_boundary.new"),
        operation("c_abi_fixture.native_boundary.dispose"),
        operation("c_abi_fixture.native_boundary.matmul"),
        operation("c_abi_fixture.native_boundary.matmul"),
        operation("c_abi_fixture.native_boundary.new"),
        operation("c_abi_fixture.native_boundary.dispose"),
        operation("c_abi_fixture.native_boundary.dispose"),
        operation("c_abi_fixture.native_boundary.dispose"),
        operation("c_abi_fixture.native_boundary.dispose"),
    );
    requests.push_str(&"x".repeat(
        crate::runtime::native_boundary::adapter_abi::PUBLIC_ADAPTER_MAX_FRAME_BYTES + 1,
    ));
    requests.push('\n');
    let mut helper_process = std::process::Command::new(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn generated helper for stale secondary handle");
    helper_process
        .stdin
        .take()
        .expect("helper stdin")
        .write_all(requests.as_bytes())
        .expect("write helper requests");
    let helper_output = helper_process
        .wait_with_output()
        .expect("wait for generated helper");
    assert!(helper_output.status.success());
    let helper_stdout = String::from_utf8_lossy(&helper_output.stdout);
    let stale_code = STANDARD.encode("stale_handle");
    assert!(
        helper_stdout.lines().any(|line| line.contains(&stale_code)),
        "secondary stale handle was not rejected:\n{helper_stdout}"
    );
    let type_code = STANDARD.encode("handle_type_mismatch");
    assert!(
        helper_stdout.lines().any(|line| line.contains(&type_code)),
        "secondary handle type was not rejected:\n{helper_stdout}"
    );
    assert!(
        helper_stdout
            .lines()
            .any(|line| line == format!("reply 6 1 ok_handle 2 2 {handle_type}")),
        "disposed slot was not reused with a new generation:\n{helper_stdout}"
    );
    let storage_code = STANDARD.encode("handle_storage_mismatch");
    assert!(
        helper_stdout
            .lines()
            .any(|line| line.starts_with("reply 7 1 ") && line.contains(&storage_code)),
        "old generation was not rejected after slot reuse:\n{helper_stdout}"
    );
    assert!(
        helper_stdout
            .lines()
            .any(|line| line.starts_with("reply 9 1 ") && line.contains(&stale_code)),
        "double disposal was not rejected:\n{helper_stdout}"
    );
    for code in ["request_not_monotonic", "frame_too_large"] {
        let encoded = STANDARD.encode(code);
        assert!(
            helper_stdout.lines().any(|line| line.contains(&encoded)),
            "adapter did not reject {code}:\n{helper_stdout}"
        );
    }
    helper
}

#[test]
fn generated_c_adapter_compiles_and_enforces_public_protocol() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("adapter_protocol");
    let target_dir = temp_dir("adapter_protocol_target");
    generate_c_abi_bindings(&fixture_manifest(), &out_dir).expect("generate C ABI package");

    compile_and_exercise_generated_c_adapter(&out_dir, &target_dir);

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
    fs::remove_dir_all(target_dir).expect("remove generated target");
}

#[test]
fn generated_c_ffi_compiles_links_owns_and_executes_from_terlan() {
    let _guard = crate::commands::bind::native_helper_env_lock()
        .lock()
        .expect("native helper env lock");
    let out_dir = temp_dir("end_to_end");
    let target_dir = temp_dir("end_to_end_target");
    generate_c_abi_bindings(&fixture_manifest(), &out_dir).expect("generate C ABI package");
    let helper = compile_and_exercise_generated_c_adapter(&out_dir, &target_dir);

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
fn skipped_c_symbols_are_sorted_and_cover_required_rejection_families() {
    let first = temp_dir("stable_first");
    let second = temp_dir("stable_second");
    generate_c_abi_bindings(&fixture_manifest(), &first).expect("first generation");
    generate_c_abi_bindings(&fixture_manifest(), &second).expect("second generation");

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
            "native_bindgen.c_abi_version_missing",
            "native_bindgen.c_borrowed_lifetime",
            "native_bindgen.c_missing_destructor",
            "native_bindgen.c_pointer_ownership_unknown",
            "native_bindgen.c_thread_local_error",
            "native_bindgen.c_unsupported_bitfield",
            "native_bindgen.c_unsupported_callback",
            "native_bindgen.c_unsupported_union",
            "native_bindgen.c_unsupported_variadic_function",
        ])
    );

    fs::remove_dir_all(first).expect("remove first output");
    fs::remove_dir_all(second).expect("remove second output");
}
