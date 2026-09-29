use super::*;

#[test]
fn optional_enum_presence_group_is_not_mistaken_for_later_optional_bool() {
    let args: Vec<NativeBindingArg> = serde_json::from_value(serde_json::json!([
        {"name": "has_dtype", "ty": "Bool"},
        {"name": "dtype", "ty": "DType"}
    ]))
    .expect("presence group");
    let parameter: CppParameter = serde_json::from_value(serde_json::json!({
        "name": "dtype",
        "ty": {
            "spelling": "std::optional<ScalarType>",
            "canonical": "std::optional<c10::ScalarType>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        },
        "direction": "input"
    }))
    .expect("optional enum parameter");
    let later_parameter: CppParameter = serde_json::from_value(serde_json::json!({
        "name": "pin_memory",
        "ty": {
            "spelling": "std::optional<bool>",
            "canonical": "std::optional<bool>",
            "is_const": false,
            "pointer_depth": 0,
            "reference": "none",
            "function_pointer": false,
            "template_dependent": false
        },
        "direction": "input"
    }))
    .expect("later optional bool parameter");
    assert!(optional_presence_pair(&args, 0, &parameter));
    assert!(!should_omit_default_optional_parameter(
        &args,
        0,
        &[parameter, later_parameter],
        0
    ));
}
