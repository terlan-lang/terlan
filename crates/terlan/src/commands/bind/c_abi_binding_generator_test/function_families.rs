use super::*;

/// Replaces one fixture dispatcher function with a two-member family.
fn replace_dispatcher_function_with_family(metadata: &mut Value) {
    let function = metadata["modules"][0]["functions"]
        .as_array_mut()
        .expect("fixture functions")
        .remove(5);
    let dispatcher = &function["dispatcher"];
    metadata["modules"][0]["function_families"] = serde_json::json!([{
        "operation_prefix": "fixture.family",
        "c_symbol": function["c_symbol"],
        "role": function["role"],
        "args": function["args"],
        "returns": function["returns"],
        "blocking": function["blocking"],
        "resource": function["resource"],
        "dispatcher": {
            "duplicate_handle_symbol": dispatcher["duplicate_handle_symbol"],
            "optional_value_allocator_symbol": dispatcher["optional_value_allocator_symbol"],
            "optional_value_destructor_symbol": dispatcher["optional_value_destructor_symbol"],
            "list_allocator_symbol": dispatcher["list_allocator_symbol"],
            "list_push_symbol": dispatcher["list_push_symbol"],
            "list_destructor_symbol": dispatcher["list_destructor_symbol"],
            "string_allocator_symbol": dispatcher["string_allocator_symbol"],
            "string_destructor_symbol": dispatcher["string_destructor_symbol"],
            "extension_abi_version": dispatcher["extension_abi_version"],
            "stack": dispatcher["stack"],
            "output": dispatcher["output"]
        },
        "members": [
            {
                "name": "family_first",
                "adapter_name": "family_first_adapter",
                "operator_name": "fixture::first",
                "overload_name": "out",
                "documentation": "Executes the first generated family member."
            },
            {
                "name": "family_second",
                "operation_suffix": "second_operation",
                "operator_name": "fixture::second",
                "documentation": "Executes the second generated family member."
            }
        ]
    }]);
}

/// Proves families flatten into deterministic ordinary function metadata.
#[test]
fn dispatcher_function_families_expand_and_flatten_deterministically() {
    let manifest = write_fixture_variant("function_family", |metadata| {
        replace_dispatcher_function_with_family(metadata);
    });
    let out_dir = temp_dir("function_family_output");
    let summary = generate_c_abi_bindings(&manifest, &out_dir).expect("generate family");
    let generated = fs::read_to_string(out_dir.join("bindings/native-binding-manifest.json"))
        .expect("read normalized manifest");
    let generated: Value = serde_json::from_str(&generated).expect("parse normalized manifest");
    let functions = generated["modules"][0]["functions"]
        .as_array()
        .expect("normalized functions");

    assert_eq!(summary.function_count, 11);
    assert!(generated["modules"][0]["function_families"].is_null());
    assert!(functions.iter().any(|function| {
        function["name"] == "family_first"
            && function["adapter_name"] == "family_first_adapter"
            && function["operation"] == "fixture.family.family_first"
            && function["dispatcher"]["operator_name"] == "fixture::first"
            && function["dispatcher"]["overload_name"] == "out"
    }));
    assert!(functions.iter().any(|function| {
        function["name"] == "family_second"
            && function["operation"] == "fixture.family.second_operation"
            && function["dispatcher"]["operator_name"] == "fixture::second"
            && function["dispatcher"]["overload_name"] == ""
    }));

    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove fixture");
    fs::remove_dir_all(out_dir).expect("remove output");
}

/// Verifies an invalid family operation prefix fails before output creation.
fn assert_invalid_operation_prefix(prefix: &str, fixture_name: &str) {
    let manifest = write_fixture_variant(fixture_name, |metadata| {
        replace_dispatcher_function_with_family(metadata);
        metadata["modules"][0]["function_families"][0]["operation_prefix"] =
            Value::String(prefix.to_string());
    });
    let out_dir = temp_dir(&format!("{fixture_name}_output"));
    let error = generate_c_abi_bindings(&manifest, &out_dir)
        .expect_err("invalid operation prefix unexpectedly generated");

    assert!(error.contains("invalid operation_prefix"), "{error}");
    assert!(!out_dir.exists());
    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove fixture");
}

/// Rejects an empty family operation namespace.
#[test]
fn dispatcher_function_families_reject_empty_operation_prefixes() {
    assert_invalid_operation_prefix("", "empty_function_family_prefix");
}

/// Rejects a namespace that would produce a doubled operation separator.
#[test]
fn dispatcher_function_families_reject_trailing_operation_separators() {
    assert_invalid_operation_prefix("fixture.family.", "trailing_function_family_prefix");
}

/// Rejects a family that cannot expand any public functions.
#[test]
fn dispatcher_function_families_reject_empty_member_sets() {
    let manifest = write_fixture_variant("empty_function_family", |metadata| {
        replace_dispatcher_function_with_family(metadata);
        metadata["modules"][0]["function_families"][0]["members"] = serde_json::json!([]);
    });
    let out_dir = temp_dir("empty_function_family_output");
    let error = generate_c_abi_bindings(&manifest, &out_dir)
        .expect_err("empty family unexpectedly generated");

    assert!(error.contains("function family without members"), "{error}");
    assert!(!out_dir.exists());
    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove fixture");
}

/// Rejects duplicate public names within one family.
#[test]
fn dispatcher_function_families_reject_duplicate_members() {
    let manifest = write_fixture_variant("duplicate_function_family", |metadata| {
        replace_dispatcher_function_with_family(metadata);
        let first = metadata["modules"][0]["function_families"][0]["members"][0].clone();
        metadata["modules"][0]["function_families"][0]["members"]
            .as_array_mut()
            .expect("family members")
            .push(first);
    });
    let out_dir = temp_dir("duplicate_function_family_output");
    let error = generate_c_abi_bindings(&manifest, &out_dir)
        .expect_err("duplicate family unexpectedly generated");

    assert!(error.contains("repeats family member"), "{error}");
    assert!(!out_dir.exists());
    fs::remove_dir_all(manifest.parent().expect("manifest parent")).expect("remove fixture");
}
