use super::*;

#[test]
pub(super) fn direct_c_wrapper_maps_multiple_handle_inputs_and_scalar_conversion() {
    let manifest = write_fixture_variant("direct_multiple_handles", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_subtract",
                "c_name": "terlan_c_native_boundary_subtract",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "left", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "right", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "alpha", "c_type": "double", "direction": "input", "ownership": "value"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "subtract",
                "operation": "c_abi_fixture.native_boundary.subtract",
                "c_symbol": "function.native_boundary_subtract",
                "role": "immutable_method",
                "args": [
                    {"name": "left", "ty": "NativeBoundary"},
                    {"name": "right", "ty": "NativeBoundary"},
                    {"name": "alpha", "ty": "Int"}
                ],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Exercises two direct handle inputs and scalar conversion."
            }));
    });
    let out_dir = temp_dir("direct_multiple_handles_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate multi-handle C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter
        .contains("pub fn subtract(&self, right: &Self, alpha: i64) -> Result<Self, CAbiError>"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "ffi::terlan_c_native_boundary_subtract(self.raw.as_ptr(), right.raw.as_ptr(), alpha as f64, &mut raw)"
    ));
    let consumer = fs::read_to_string(out_dir.join("tests/c_abi_fixture/NativeBoundaryTest.terl"))
        .expect("generated consumer");
    assert!(!consumer.contains("returned_subtract"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn direct_c_constructor_names_each_borrowed_handle_argument() {
    let manifest = write_fixture_variant("direct_handle_constructor", |metadata| {
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "function.native_boundary_blend",
                "c_name": "terlan_c_native_boundary_blend",
                "kind": "function",
                "status": "bind",
                "returns": "int32_t",
                "error_model": "status_code",
                "success_code": 0,
                "parameters": [
                    {"name": "start", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "stop", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                    {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
                ]
            }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "blend",
                "operation": "c_abi_fixture.native_boundary.blend",
                "c_symbol": "function.native_boundary_blend",
                "role": "constructor",
                "args": [
                    {"name": "start", "ty": "NativeBoundary"},
                    {"name": "stop", "ty": "NativeBoundary"}
                ],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Exercises named resource inputs on an associated constructor."
            }));
    });
    let out_dir = temp_dir("direct_handle_constructor_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate resource constructor");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn blend(start: &Self, stop: &Self) -> Result<Self, CAbiError>"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "ffi::terlan_c_native_boundary_blend(start.raw.as_ptr(), stop.raw.as_ptr(), &mut raw)"
    ));
    assert!(!adapter.contains("terlan_c_native_boundary_blend(self.raw.as_ptr()"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn c_abi_wrapper_supports_float_constructors_arguments_and_results() {
    let manifest = write_fixture_variant("float_values", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_create_float",
            "c_name": "terlan_c_native_boundary_create_float",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "value", "c_type": "double", "direction": "input", "ownership": "value"},
                {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_ratio",
            "c_name": "terlan_c_native_boundary_ratio",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "out_ratio", "c_type": "double *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        let functions = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions");
        functions.push(serde_json::json!({
            "name": "new_float",
            "operation": "c_abi_fixture.native_boundary.new_float",
            "c_symbol": "function.native_boundary_create_float",
            "role": "constructor",
            "args": [{"name": "value", "ty": "Float"}],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "opaque_handle",
            "documentation": "Creates a boundary from a float."
        }));
        functions.push(serde_json::json!({
            "name": "ratio",
            "operation": "c_abi_fixture.native_boundary.ratio",
            "c_symbol": "function.native_boundary_ratio",
            "role": "immutable_method",
            "args": [{"name": "boundary", "ty": "NativeBoundary"}],
            "returns": "Float",
            "blocking": "fast",
            "resource": "borrowed_handle",
            "documentation": "Reads a floating-point ratio."
        }));
    });
    let out_dir = temp_dir("float_values_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate float C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn new_float(value: f64) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("pub fn ratio(&self) -> Result<f64, CAbiError>"));
    assert!(adapter.contains("terlan_c_native_boundary_create_float(value, &mut raw)"));
    assert!(adapter.contains("Ok(out_out_ratio)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Float(value)"));
    assert!(helper.contains("ok_float {value}"));
    assert!(helper.contains("strip_prefix(\"f:\")"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub new_float(value: Float): NativeBoundary"));
    assert!(source.contains("pub ratio(boundary: NativeBoundary): Float"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn c_abi_wrapper_supports_bool_constructors_arguments_and_results() {
    let manifest = write_fixture_variant("bool_values", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_create_bool",
            "c_name": "terlan_c_native_boundary_create_bool",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "value", "c_type": "bool", "direction": "input", "ownership": "value"},
                {"name": "out_boundary", "c_type": "TerlanCNativeBoundary **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.native_boundary_enabled",
            "c_name": "terlan_c_native_boundary_enabled",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "boundary", "c_type": "const TerlanCNativeBoundary *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "enabled", "c_type": "bool *", "direction": "output", "ownership": "borrowed_call"}
            ]
        }));
        let functions = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions");
        functions.push(serde_json::json!({
            "name": "new_bool",
            "operation": "c_abi_fixture.native_boundary.new_bool",
            "c_symbol": "function.native_boundary_create_bool",
            "role": "constructor",
            "args": [{"name": "value", "ty": "Bool"}],
            "returns": "NativeBoundary",
            "blocking": "fast",
            "resource": "opaque_handle",
            "documentation": "Creates a boundary from a boolean."
        }));
        functions.push(serde_json::json!({
            "name": "enabled",
            "operation": "c_abi_fixture.native_boundary.enabled",
            "c_symbol": "function.native_boundary_enabled",
            "role": "immutable_method",
            "args": [{"name": "boundary", "ty": "NativeBoundary"}],
            "returns": "Bool",
            "blocking": "fast",
            "resource": "borrowed_handle",
            "documentation": "Reads a boolean property."
        }));
    });
    let out_dir = temp_dir("bool_values_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate bool C wrapper");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn new_bool(value: bool) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("pub fn enabled(&self) -> Result<bool, CAbiError>"));
    assert!(adapter.contains("terlan_c_native_boundary_create_bool(value, &mut raw)"));
    assert!(adapter.contains("Ok(out_enabled)"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("Arg::Bool(value)"));
    assert!(helper.contains("ok_bool {value}"));
    assert!(helper.contains("strip_prefix(\"b:\")"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub new_bool(value: Bool): NativeBoundary"));
    assert!(source.contains("pub enabled(boundary: NativeBoundary): Bool"));

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
pub(super) fn c_abi_value_only_package_builds_without_an_opaque_resource() {
    let manifest = write_fixture_variant("value_only", |metadata| {
        metadata["validation"]["smoke"] = Value::String("package_owned_live".into());
        metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .retain(|symbol| symbol["id"] == "function.native_boundary_live_count");
        metadata["modules"][0]["types"] = serde_json::json!([]);
        let functions = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions");
        functions.retain(|function| function["name"] == "live_count");
        functions[0]["generated_smoke"] = Value::String("package_owned".into());
    });
    let out_dir = temp_dir("value_only_output");
    let target_dir = temp_dir("value_only_target");

    let summary =
        generate_c_abi_bindings(&manifest, &out_dir).expect("generate a value-only C ABI package");
    assert_eq!(summary.function_count, 1);
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs"))
        .expect("read value-only adapter");
    assert!(adapter.contains("pub fn live_count() -> i64"));
    assert!(!adapter.contains("pub struct NativeBoundary"));

    let output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--offline", "--quiet", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("build generated value-only package");
    assert!(
        output.status.success(),
        "value-only package build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(manifest.parent().expect("variant parent")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
    fs::remove_dir_all(target_dir).expect("remove target");
}
