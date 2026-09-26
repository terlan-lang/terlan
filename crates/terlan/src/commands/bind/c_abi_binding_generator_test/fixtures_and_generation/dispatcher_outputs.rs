use super::*;

use std::fs;

use serde_json::Value;

fn mutable_clone(metadata: &mut Value, output: Value) {
    let function = metadata["modules"][0]["functions"]
        .as_array_mut()
        .expect("functions")
        .iter_mut()
        .find(|function| function["name"] == "clone")
        .expect("clone binding");
    function["name"] = Value::String("write_clone".to_string());
    function["operation"] = Value::String("c_abi_fixture.native_boundary.write_clone".to_string());
    function["role"] = Value::String("mutable_method".to_string());
    function["returns"] = Value::String("Unit".to_string());
    function["dispatcher"]["output"] = output;
}

fn mutable_clone_with_output_and_source(metadata: &mut Value) {
    mutable_clone(
        metadata,
        serde_json::json!({
            "kind": "discard_owned_handle_tuple",
            "indices": [0, 1]
        }),
    );
    let function = metadata["modules"][0]["functions"]
        .as_array_mut()
        .expect("functions")
        .iter_mut()
        .find(|function| function["name"] == "write_clone")
        .expect("mutable clone binding");
    function["args"] = serde_json::json!([
        {"name": "output", "ty": "NativeBoundary"},
        {"name": "other_output", "ty": "NativeBoundary", "mutable": true},
        {"name": "source", "ty": "NativeBoundary"},
        {"name": "source_alias", "ty": "NativeBoundary"}
    ]);
    function["dispatcher"]["stack"] = serde_json::json!([
        {"kind": "owned_handle_copy", "argument": "output"},
        {"kind": "owned_handle_copy", "argument": "other_output"},
        {"kind": "owned_handle_copy", "argument": "source"},
        {"kind": "owned_handle_copy", "argument": "source_alias"}
    ]);
    function["generated_smoke"] = Value::String("package_owned".to_string());
}

#[test]
fn mutable_dispatcher_can_discard_one_owned_alias_result() {
    let manifest = write_fixture_variant("dispatcher_discard_output", |metadata| {
        mutable_clone(
            metadata,
            serde_json::json!({"kind": "discard_owned_handle", "index": 0}),
        );
    });
    let out_dir = temp_dir("dispatcher_discard_output_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate mutable dispatcher package");

    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("pub fn write_clone(&mut self) -> Result<(), CAbiError>"));
    assert!(adapter.contains("let _output = DispatcherOutputGuard::new(raw)"));
    assert!(adapter.contains("Ok(())"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper.contains("value_boundary.write_clone()"));
    assert!(helper.contains("Ok(()) => \"ok_unit\".to_string()"));
    let source = fs::read_to_string(out_dir.join("src/c_abi_fixture/NativeBoundary.terl"))
        .expect("Terlan source");
    assert!(source.contains("pub write_clone(boundary: NativeBoundary): Unit"));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn mutable_dispatcher_can_discard_multiple_owned_alias_results() {
    let manifest = write_fixture_variant("dispatcher_discard_tuple", |metadata| {
        mutable_clone_with_output_and_source(metadata);
    });
    let out_dir = temp_dir("dispatcher_discard_tuple_generated");

    generate_c_abi_bindings(&manifest, &out_dir).expect("generate tuple-discard package");

    let adapter = fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("adapter");
    assert!(adapter.contains("let _output_0 = DispatcherOutputGuard::new(raw_0)"));
    assert!(adapter.contains("let _output_1 = DispatcherOutputGuard::new(raw_1)"));
    assert!(
        adapter.contains("pub fn write_clone(&mut self, other_output: &mut Self, source: &Self, source_alias: &Self)")
    );
    assert!(adapter.contains("let mut stack = ["));
    assert!(adapter.contains("0u64"));
    assert!(adapter.contains("Ok(())"));
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("helper");
    assert!(helper
        .contains("HandleValue::NativeBoundary(value_other_output) = &mut entry_other_output"));
    assert!(helper
        .contains("HandleValue::NativeBoundary(value_source) = &self.handles.get(&source.id)"));
    assert!(!helper.contains("if source.id == source_alias.id"));
    assert!(helper.contains(
        "value_output.write_clone(value_other_output, value_source, value_source_alias)"
    ));

    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn mutable_dispatcher_rejects_invalid_mutable_argument_shapes() {
    for (name, mutate, expected) in [
        (
            "immutable_method",
            0usize,
            "may be mutable only when it is a non-receiver opaque-resource argument",
        ),
        (
            "receiver",
            1usize,
            "may be mutable only when it is a non-receiver opaque-resource argument",
        ),
        (
            "scalar",
            2usize,
            "may be mutable only when it is a non-receiver opaque-resource argument",
        ),
    ] {
        let manifest = write_fixture_variant(name, |metadata| {
            let function = metadata["modules"][0]["functions"]
                .as_array_mut()
                .expect("functions")
                .iter_mut()
                .find(|function| function["name"] == "clone")
                .expect("clone binding");
            match mutate {
                0 => {
                    function["args"] = serde_json::json!([
                        {"name": "boundary", "ty": "NativeBoundary"},
                        {"name": "output", "ty": "NativeBoundary", "mutable": true}
                    ]);
                }
                1 => function["args"][0]["mutable"] = Value::Bool(true),
                2 => {
                    function["role"] = Value::String("mutable_method".to_string());
                    function["returns"] = Value::String("Unit".to_string());
                    function["args"] = serde_json::json!([
                        {"name": "boundary", "ty": "NativeBoundary"},
                        {"name": "count", "ty": "Int", "mutable": true}
                    ]);
                }
                _ => unreachable!(),
            }
        });
        let out_dir = temp_dir(name);
        let error = generate_c_abi_bindings(&manifest, &out_dir).expect_err("reject mutability");
        assert!(error.contains(expected), "unexpected error: {error}");
        fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    }
}
