use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "terlan_c_abi_bind_{name}_{}_{}",
        std::process::id(),
        now
    ))
}

fn contains_ignoring_whitespace(source: &str, expected: &str) -> bool {
    let compact = |value: &str| {
        value
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>()
            .replace(",)", ")")
            .replace(",]", "]")
    };
    compact(source).contains(&compact(expected))
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/c_abi_native_boundary")
}

fn fixture_manifest() -> PathBuf {
    fixture_dir().join("native-binding.json")
}

fn generated_files(root: &Path) -> Vec<PathBuf> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
        let mut entries = fs::read_dir(directory)
            .expect("read generated directory")
            .map(|entry| entry.expect("read generated entry"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                visit(root, &path, files);
            } else if path.file_name().is_none_or(|name| name != "Cargo.lock") {
                files.push(
                    path.strip_prefix(root)
                        .expect("generated file is below root")
                        .to_path_buf(),
                );
            }
        }
    }

    let mut files = Vec::new();
    visit(root, root, &mut files);
    files
}

fn write_fixture_variant(name: &str, mutate: impl FnOnce(&mut Value)) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("create variant dir");
    fs::copy(
        fixture_dir().join("native_boundary.h"),
        root.join("native_boundary.h"),
    )
    .expect("copy header");
    fs::copy(
        fixture_dir().join("native_boundary.c"),
        root.join("native_boundary.c"),
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
    metadata["c_metadata"]["symbols"]
        .as_array_mut()
        .expect("symbols")
        .iter_mut()
        .find(|symbol| symbol["id"] == id)
        .expect("fixture symbol")
}

#[test]
fn structured_c_metadata_generates_real_ffi_package() {
    let out_dir = temp_dir("outputs");

    let summary =
        generate_c_abi_bindings(&fixture_manifest(), &out_dir).expect("generate C ABI package");

    assert_eq!(
        summary,
        CAbiBindingGenerationSummary {
            module_count: 1,
            function_count: 10,
            skipped_symbol_count: 9,
        }
    );
    for path in [
        "terlan.toml",
        "src/c_abi_fixture/NativeBoundary.terl",
        "tests/c_abi_fixture/NativeBoundaryTest.terl",
        "native/terlan-native.toml",
        "native/rust/Cargo.toml",
        "native/rust/build.rs",
        "native/rust/src/lib.rs",
        "native/rust/src/bin/native_boundary_helper.rs",
        "native/rust/include/native_boundary.h",
        "native/rust/c/native_boundary.c",
        "bindings/native-binding-manifest.json",
        "bindings/skipped-symbols.json",
    ] {
        assert!(out_dir.join(path).is_file(), "missing generated {path}");
    }

    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("unsafe extern \"C\""));
    assert!(adapter.contains("pub struct TerlanCNativeBoundary"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "pub fn terlan_c_native_boundary_create(value: i64, out_boundary: *mut *mut TerlanCNativeBoundary) -> i32;"
    ));
    assert!(adapter.contains("pub struct NativeBoundary"));
    assert!(adapter.contains("impl Drop for NativeBoundary"));
    assert!(adapter.contains("pub fn samples(&self) -> Result<Vec<i64>, CAbiError>"));
    assert!(adapter.contains("pub fn clone(&self) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("pub fn unsqueeze(&self, dimension: i64) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("pub fn matmul(&self, right: &Self) -> Result<Self, CAbiError>"));
    assert!(adapter.contains("struct DispatcherInputGuard"));
    assert!(adapter.contains("ffi::terlan_c_call_dispatcher"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "let mut stack = [dispatcher_input_boundary.into_stable_ivalue(), 0u64]"
    ));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "let mut stack = [dispatcher_input_boundary.into_stable_ivalue(), dimension as u64]"
    ));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "let mut stack = [dispatcher_input_left.into_stable_ivalue(), dispatcher_input_right.into_stable_ivalue()]"
    ));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "std::slice::from_raw_parts(pointer.as_ptr(), length).to_vec()"
    ));
    assert!(!adapter.contains("cxx::bridge"));
    assert!(!adapter.contains("torch"));
    assert!(!adapter.contains("pjrt"));

    let cargo = fs::read_to_string(out_dir.join("native/rust/Cargo.toml")).expect("Cargo.toml");
    assert!(cargo.contains("cc = \"=1.2.67\""));
    assert!(cargo.contains("[workspace]"));
    assert!(!cargo.contains("cxx"));
    let package = fs::read_to_string(out_dir.join("terlan.toml")).expect("terlan.toml");
    assert!(package.contains("namespace = \"c_abi_fixture\""));
    assert!(package.contains("artifact = \"library\""));
    assert!(package.contains("[native.rust]"));
    assert!(package.contains("crate = \"terlan-c-abi-boundary-fixture\""));
    assert!(package.contains("path = \"native/rust\""));
    assert!(package.contains("helper = \"native-boundary-helper\""));
    assert!(package.contains("helper_env = \"TERLAN_NATIVE_BOUNDARY_HELPER_PATH\""));
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build.rs");
    assert!(build.contains("cc::Build::new()"));
    assert!(build.contains("c/native_boundary.c"));
    let boundary_metadata =
        fs::read_to_string(out_dir.join("native/terlan-native.toml")).expect("metadata");
    for field in [
        "[public_adapter]",
        "adapter_abi_version = 1",
        "execution_context = \"explicit\"",
        "ownership = \"opaque_handles\"",
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
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("const MAX_ADAPTER_FRAME_BYTES: usize = 1048576"));
    assert!(helper.contains("take((MAX_ADAPTER_FRAME_BYTES + 1) as u64)"));
    assert!(helper.contains("last_request_id"));
    assert!(helper.contains("request_not_monotonic"));

    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan module");
    assert!(source.contains("pub opaque type NativeBoundary."));
    assert!(source.contains("@compiler.native {c_abi_fixture.native_boundary.new}"));
    assert!(!source.contains("std.native"));
    let consumer = fs::read_to_string(out_dir.join("tests/c_abi_fixture/NativeBoundaryTest.terl"))
        .expect("generated consumer");
    assert!(consumer.contains("observed_clone_value == observed_returned_source_value"));
    assert!(!consumer.contains("observed_unsqueeze_value == observed_returned_source_value"));

    fs::remove_dir_all(out_dir).expect("remove generated outputs");
}

#[test]
fn supplemental_private_c_headers_are_copied_and_compiled() {
    let manifest = write_fixture_variant("supplemental_private_header", |metadata| {
        metadata["c_metadata"]["headers"] = serde_json::json!(["native_boundary_internal.h"]);
    });
    let root = manifest.parent().expect("variant root");
    fs::write(
        root.join("native_boundary_internal.h"),
        "#ifndef NATIVE_BOUNDARY_INTERNAL_H\n#define NATIVE_BOUNDARY_INTERNAL_H\n#define NATIVE_BOUNDARY_LAYOUT_VERSION 1\n#endif\n",
    )
    .expect("write private header");
    let source_path = root.join("native_boundary.c");
    let source = fs::read_to_string(&source_path).expect("read fixture source");
    fs::write(
        &source_path,
        format!(
            "#include \"native_boundary_internal.h\"\n_Static_assert(NATIVE_BOUNDARY_LAYOUT_VERSION == 1, \"private header mismatch\");\n{source}"
        ),
    )
    .expect("include private header");
    let out_dir = temp_dir("supplemental_private_header_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate package with private header");
    assert_eq!(
        fs::read_to_string(out_dir.join("native/rust/c/native_boundary_internal.h"))
            .expect("copied private header"),
        fs::read_to_string(root.join("native_boundary_internal.h")).expect("source private header")
    );
    let build = fs::read_to_string(out_dir.join("native/rust/build.rs")).expect("build script");
    assert!(!build.contains("c_build.file(\"c/native_boundary_internal.h\")"));

    let target_dir = temp_dir("supplemental_private_header_target");
    let output =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
            .args(["build", "--offline", "--quiet", "--manifest-path"])
            .arg(out_dir.join("native/rust/Cargo.toml"))
            .env("CARGO_TARGET_DIR", &target_dir)
            .output()
            .expect("build generated package");
    assert!(
        output.status.success(),
        "private-header package build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(root).expect("remove variant root");
    fs::remove_dir_all(out_dir).expect("remove generated output");
    fs::remove_dir_all(target_dir).expect("remove target");
}

#[test]
fn generated_package_identity_is_independent_from_native_crate_identity() {
    let manifest = write_fixture_variant("package_identity", |metadata| {
        metadata["package"]["name"] = Value::String("terlan-c-fixture".into());
        metadata["package"]["version"] = Value::String("0.0.7".into());
    });
    let out_dir = temp_dir("package_identity_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate named C ABI package");

    let package = fs::read_to_string(out_dir.join("terlan.toml")).expect("terlan.toml");
    assert!(package.contains("name = \"terlan-c-fixture\""));
    assert!(package.contains("version = \"0.0.7\""));
    assert!(package.contains("crate = \"terlan-c-abi-boundary-fixture\""));
    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove manifest");
    fs::remove_dir_all(out_dir).expect("remove generated outputs");
}

#[test]
fn generated_package_identity_rejects_non_exact_versions() {
    let manifest = write_fixture_variant("package_version", |metadata| {
        metadata["package"]["version"] = Value::String("0.0.7-dev".into());
    });
    let out_dir = temp_dir("package_version_output");

    let error = generate_c_abi_bindings(&manifest, &out_dir)
        .expect_err("non-exact package versions must fail");

    assert!(error.contains("exact x.y.z"), "{error}");
    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove manifest");
}

#[test]
fn c_abi_generation_rejects_disabled_validation_obligations() {
    let manifest = write_fixture_variant("disabled_validation", |metadata| {
        metadata["validation"]["ownership_lifecycle"] = Value::Bool(false);
    });
    let error = generate_c_abi_bindings(&manifest, &temp_dir("disabled_validation_output"))
        .expect_err("disabled ownership validation must fail");
    assert!(error.contains("C ABI validation obligation `ownership_lifecycle` must be enabled"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
}

#[test]
fn generated_scalar_smoke_can_defer_package_specific_operation_domains() {
    let manifest = write_fixture_variant("package_owned_smoke", |metadata| {
        let function = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .iter_mut()
            .find(|function| function["name"] == "unsqueeze")
            .expect("unsqueeze binding");
        function["generated_smoke"] = Value::String("package_owned".to_string());
    });
    let out_dir = temp_dir("package_owned_smoke_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate deferred smoke package");
    let consumer = fs::read_to_string(out_dir.join("tests/c_abi_fixture/NativeBoundaryTest.terl"))
        .expect("generated consumer");
    assert!(!consumer.contains("returned_unsqueeze"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("generated module");
    assert!(source.contains("pub unsqueeze"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn dispatcher_metadata_generates_owned_optional_and_integer_list_stack_values() {
    let manifest = write_fixture_variant("dispatcher_optional_bool", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.new_stable_ivalue",
            "c_name": "terlan_c_new_stable_ivalue",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "value", "c_type": "uint64_t **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.delete_stable_ivalue",
            "c_name": "terlan_c_delete_stable_ivalue",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "value", "c_type": "uint64_t *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.new_list",
            "c_name": "terlan_c_new_list",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "capacity", "c_type": "size_t", "direction": "input", "ownership": "value"},
                {"name": "list", "c_type": "void **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.list_push",
            "c_name": "terlan_c_list_push",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "list", "c_type": "void *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "element", "c_type": "uint64_t", "direction": "input", "ownership": "value"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.delete_list",
            "c_name": "terlan_c_delete_list",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "list", "c_type": "void *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "reduce_index",
                "operation": "c_abi_fixture.native_boundary.reduce_index",
                "c_symbol": "function.call_dispatcher",
                "role": "immutable_method",
                "args": [
                    {"name": "boundary", "ty": "NativeBoundary"},
                    {"name": "weight", "ty": "NativeBoundary"},
                    {"name": "dimensions", "ty": "List[Int]"},
                    {"name": "dimension", "ty": "Int"},
                    {"name": "keep_dimension", "ty": "Bool"},
                    {"name": "smoothing", "ty": "Float"}
                ],
                "returns": "NativeBoundary",
                "blocking": "fast",
                "resource": "opaque_handle",
                "documentation": "Exercises owned optional handles and integers with booleans.",
                "generated_smoke": "package_owned",
                "dispatcher": {
                    "duplicate_handle_symbol": "function.native_boundary_duplicate_handle",
                    "optional_value_allocator_symbol": "function.new_stable_ivalue",
                    "optional_value_destructor_symbol": "function.delete_stable_ivalue",
                    "list_allocator_symbol": "function.new_list",
                    "list_push_symbol": "function.list_push",
                    "list_destructor_symbol": "function.delete_list",
                    "operator_name": "fixture::reduce_index",
                    "overload_name": "",
                    "extension_abi_version": "0x0001000000000000",
                    "stack": [
                        {"kind": "owned_handle_copy", "argument": "boundary"},
                        {"kind": "owned_optional_handle_copy", "argument": "weight"},
                        {"kind": "owned_int_list_argument", "argument": "dimensions"},
                        {"kind": "owned_optional_int_argument", "argument": "dimension"},
                        {"kind": "bool_argument", "argument": "keep_dimension"},
                        {"kind": "float_argument", "argument": "smoothing"}
                    ],
                    "output": {"kind": "owned_handle", "index": 0}
                }
            }));
    });
    let out_dir = temp_dir("dispatcher_optional_bool_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate optional dispatcher package");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("struct DispatcherOptionalValueGuard"));
    assert!(adapter.contains("ffi::terlan_c_new_stable_ivalue"));
    assert!(adapter.contains("ffi::terlan_c_delete_stable_ivalue"));
    assert!(adapter.contains("struct DispatcherListGuard"));
    assert!(adapter.contains("ffi::terlan_c_new_list(dimensions.len()"));
    assert!(adapter.contains("ffi::terlan_c_list_push(dispatcher_list_dimensions.as_ptr()"));
    assert!(adapter.contains("ffi::terlan_c_delete_list"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "dispatcher_optional_weight.write_stable_ivalue(dispatcher_input_weight.into_stable_ivalue())"
    ));
    assert!(adapter.contains("dispatcher_optional_dimension.write_i64(dimension)"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "dispatcher_optional_weight.into_stable_ivalue(), dispatcher_list_dimensions.into_stable_ivalue(), dispatcher_optional_dimension.into_stable_ivalue(), u64::from(keep_dimension), smoothing.to_bits()"
    ));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(contains_ignoring_whitespace(
        &source,
        "pub reduce_index(boundary: NativeBoundary, weight: NativeBoundary, dimensions: List[Int], dimension: Int, keep_dimension: Bool, smoothing: Float): NativeBoundary"
    ));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn dispatcher_metadata_generates_owned_fixed_string_stack_values() {
    let manifest = write_fixture_variant("dispatcher_owned_string", |metadata| {
        let symbols = metadata["c_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols");
        symbols.push(serde_json::json!({
            "id": "function.new_string",
            "c_name": "terlan_c_new_string",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "data", "c_type": "const char *", "direction": "input", "ownership": "borrowed_call"},
                {"name": "length", "c_type": "size_t", "direction": "input", "ownership": "value"},
                {"name": "string", "c_type": "void **", "direction": "output", "ownership": "transfer_full"}
            ]
        }));
        symbols.push(serde_json::json!({
            "id": "function.delete_string",
            "c_name": "terlan_c_delete_string",
            "kind": "function",
            "status": "bind",
            "returns": "int32_t",
            "error_model": "status_code",
            "success_code": 0,
            "parameters": [
                {"name": "string", "c_type": "void *", "direction": "input", "ownership": "transfer_full"}
            ]
        }));
        let function = metadata["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .iter_mut()
            .find(|function| function["name"] == "clone")
            .expect("clone binding");
        function["name"] = Value::String("gelu".to_string());
        function["operation"] = Value::String("c_abi_fixture.native_boundary.gelu".to_string());
        function["documentation"] = Value::String("Applies exact GELU.".to_string());
        function["dispatcher"]["operator_name"] = Value::String("aten::gelu".to_string());
        function["dispatcher"]["string_allocator_symbol"] =
            Value::String("function.new_string".to_string());
        function["dispatcher"]["string_destructor_symbol"] =
            Value::String("function.delete_string".to_string());
        function["dispatcher"]["stack"][1] = serde_json::json!({
            "kind": "owned_string_literal",
            "value": "none"
        });
    });
    let out_dir = temp_dir("dispatcher_owned_string_output");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate string dispatcher package");
    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("struct DispatcherStringGuard"));
    assert!(adapter.contains("let dispatcher_string_1_bytes: &[u8] = \"none\".as_bytes()"));
    assert!(adapter.contains("ffi::terlan_c_new_string("));
    assert!(adapter.contains("ffi::terlan_c_delete_string"));
    assert!(contains_ignoring_whitespace(
        &adapter,
        "dispatcher_input_boundary.into_stable_ivalue(), dispatcher_string_1.into_stable_ivalue()"
    ));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub gelu(boundary: NativeBoundary): NativeBoundary"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn direct_c_wrapper_maps_multiple_handle_inputs_and_scalar_conversion() {
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
fn c_abi_wrapper_supports_float_constructors_arguments_and_results() {
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
    assert!(adapter.contains("value as f64"));
    assert!(adapter.contains("Ok(out_out_ratio as f64)"));
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
fn c_abi_wrapper_supports_bool_constructors_arguments_and_results() {
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
    assert!(adapter.contains("value as bool"));
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
