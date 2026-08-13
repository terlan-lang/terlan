//! Tests for immutable secondary opaque-resource arguments.

use super::*;

#[test]
fn immutable_methods_borrow_secondary_opaque_resources() {
    let manifest = write_fixture_variant("borrowed_resource_argument", |manifest| {
        manifest["cpp_metadata"]["symbols"]
            .as_array_mut()
            .expect("symbols")
            .push(serde_json::json!({
                "id": "method.native_boundary.difference",
                "cpp_name": "difference",
                "source": {"path": "native_boundary.hpp", "line": 12, "column": 16},
                "kind": "method",
                "documentation": "Returns the difference from another boundary.",
                "annotations": ["TERLAN_BIND"],
                "overload_set": "terlan_fixture::NativeBoundary::difference",
                "receiver": "NativeBoundary",
                "receiver_mutable": false,
                "returns": {
                    "spelling": "std::int64_t", "canonical": "std::int64_t",
                    "is_const": false, "pointer_depth": 0, "reference": "none",
                    "function_pointer": false, "template_dependent": false
                },
                "parameters": [{
                    "name": "other",
                    "ty": {
                        "spelling": "const NativeBoundary &",
                        "canonical": "const terlan_fixture::NativeBoundary &",
                        "is_const": true, "pointer_depth": 0, "reference": "lvalue",
                        "function_pointer": false, "template_dependent": false
                    },
                    "direction": "input"
                }],
                "noexcept": true,
                "template_parameters": [],
                "overload_candidates": 1
            }));
        manifest["mapping"]["symbols"]
            .as_array_mut()
            .expect("mapping symbols")
            .push(serde_json::json!({
                "symbol": "method.native_boundary.difference",
                "disposition": "bind"
            }));
        manifest["modules"][0]["functions"]
            .as_array_mut()
            .expect("functions")
            .push(serde_json::json!({
                "name": "difference",
                "operation": "cpp_fixture.native_boundary.difference",
                "cpp_symbol": "method.native_boundary.difference",
                "role": "immutable_method",
                "args": [
                    {"name": "boundary", "ty": "NativeBoundary"},
                    {"name": "other", "ty": "NativeBoundary"}
                ],
                "returns": "Int",
                "blocking": "fast",
                "resource": "borrowed_handle",
                "documentation": "Compares two live boundaries."
            }));
    });
    let out_dir = temp_dir("borrowed_resource_argument_out");

    generate_cpp_bindings(&manifest, &out_dir).expect("generate borrowed resource argument");
    let bridge =
        fs::read_to_string(out_dir.join("native/rust/src/lib.rs")).expect("read generated bridge");
    let helper = fs::read_to_string(out_dir.join("native/rust/src/bin/native_boundary_helper.rs"))
        .expect("read generated helper");

    assert!(bridge.contains("fn difference(self: &NativeBoundary, other: &NativeBoundary) -> i64;"));
    assert!(helper.contains("Arg::Handle(arg_0), Arg::Handle(arg_1)"));
    assert!(helper.contains("let arg_1_entry = match self.live(arg_1"));
    assert!(helper.contains("let arg_1_ref = arg_1_value.as_ref()"));
    assert!(helper.contains(".difference(arg_1_ref)"));
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
    fs::remove_dir_all(out_dir).expect("remove output");
}

#[test]
fn mutable_methods_reject_secondary_opaque_resource_aliases() {
    let manifest = write_fixture_variant("mutable_resource_alias", |manifest| {
        let add = symbol_mut(manifest, "method.native_boundary.add");
        add["parameters"][0]["ty"] = serde_json::json!({
            "spelling": "const NativeBoundary &",
            "canonical": "const terlan_fixture::NativeBoundary &",
            "is_const": true,
            "pointer_depth": 0,
            "reference": "lvalue",
            "function_pointer": false,
            "template_dependent": false
        });
        let add = function_mut(manifest, "add");
        add["args"][1]["name"] = Value::String("other".into());
        add["args"][1]["ty"] = Value::String("NativeBoundary".into());
    });

    let error = generate_cpp_bindings(&manifest, &temp_dir("mutable_resource_alias_out"))
        .expect_err("mutable secondary resource alias must fail");
    assert!(
        error.contains("cpp.lifetime.mutable_alias"),
        "unexpected diagnostic: {error}"
    );
    fs::remove_dir_all(manifest.parent().expect("variant root")).expect("remove variant");
}
