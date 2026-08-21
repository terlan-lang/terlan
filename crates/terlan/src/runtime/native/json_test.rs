use super::*;

/// Parses a JSON fixture for adapter tests.
///
/// Inputs:
/// - `text`: JSON source expected to parse.
///
/// Output:
/// - Parsed `Json` value, or JSON null after a failing assertion.
///
/// Transformation:
/// - Converts a `Result` into a convenient test value without unwrap/expect.
fn parsed_fixture(text: &str) -> Json {
    let result = parse(text);
    assert!(result.is_ok());
    result.unwrap_or_else(|_| Json::from_serde(Value::Null))
}

/// Validates JSON parsing and compact string rendering.
///
/// Inputs:
/// - Object JSON text with stable key ordering.
///
/// Output:
/// - Test passes when parsing succeeds and rendering returns compact JSON.
///
/// Transformation:
/// - Exercises the parse/stringify path over the `serde_json` backend.
#[test]
fn parse_and_stringify_round_trip_json_text() {
    let json = parsed_fixture(r#"{"name":"Ada","active":true}"#);
    assert_eq!(
        stringify(&json),
        Ok(String::from(r#"{"active":true,"name":"Ada"}"#))
    );
}

/// Validates JSON builder constructors.
///
/// Inputs:
/// - Primitive Rust values for each JSON scalar constructor.
/// - Empty array and object constructors.
///
/// Output:
/// - Test passes when each value serializes to the expected compact JSON.
///
/// Transformation:
/// - Exercises Rust-backed JSON construction without parsing source text.
#[test]
fn builder_constructors_render_json_values() {
    assert_eq!(stringify(&null()), Ok(String::from("null")));
    assert_eq!(stringify(&r#bool(true)), Ok(String::from("true")));
    assert_eq!(stringify(&int(3)), Ok(String::from("3")));
    let float_json = float(1.5).unwrap_or_else(|_| null());
    assert_eq!(stringify(&float_json), Ok(String::from("1.5")));
    assert_eq!(stringify(&string("Ada")), Ok(String::from(r#""Ada""#)));
    assert_eq!(stringify(&array()), Ok(String::from("[]")));
    assert_eq!(stringify(&object()), Ok(String::from("{}")));
}

/// Validates JSON array builder mutation.
///
/// Inputs:
/// - Empty JSON array and two JSON values.
///
/// Output:
/// - Test passes when values are appended in order.
///
/// Transformation:
/// - Mutates the adapter-owned JSON array and renders the result.
#[test]
fn mutable_array_builder_pushes_values() {
    let mut values = array();
    assert_eq!(push(&mut values, string("Ada")), Ok(()));
    assert_eq!(push(&mut values, int(3)), Ok(()));

    assert_eq!(stringify(&values), Ok(String::from(r#"["Ada",3]"#)));
}

/// Validates JSON object builder mutation.
///
/// Inputs:
/// - Empty JSON object and three keyed JSON values.
///
/// Output:
/// - Test passes when values are inserted under their keys.
///
/// Transformation:
/// - Mutates the adapter-owned JSON object and renders the result.
#[test]
fn mutable_object_builder_puts_values() {
    let mut value = object();
    assert_eq!(put(&mut value, "name", string("Ada")), Ok(()));
    assert_eq!(put(&mut value, "active", r#bool(true)), Ok(()));
    assert_eq!(put(&mut value, "count", int(3)), Ok(()));

    assert_eq!(
        stringify(&value),
        Ok(String::from(r#"{"active":true,"count":3,"name":"Ada"}"#))
    );
}

#[test]
fn pretty_stringify_uses_stable_two_space_indentation() {
    let value = parsed_fixture(r#"{"z":[1,2],"a":true}"#);

    assert_eq!(
        stringify_pretty(&value),
        Ok(String::from(
            "{\n  \"a\": true,\n  \"z\": [\n    1,\n    2\n  ]\n}"
        ))
    );
}

/// Validates object keys stay plain JSON strings even when they look like
/// runtime atom-construction names.
///
/// Inputs:
/// - JSON object keys named like unsafe Vm atom constructors.
///
/// Output:
/// - Test passes when lookup and rendering preserve the keys as ordinary text.
///
/// Transformation:
/// - Exercises the serde-backed object path without introducing atom
///   conversion or symbol interning semantics.
#[test]
fn object_keys_that_look_like_atom_builders_remain_strings() {
    let json = parsed_fixture(r#"{"binary_to_atom":"blocked","list_to_atom":"blocked"}"#);
    let binary_to_atom =
        get(&json, "binary_to_atom").unwrap_or_else(|_| Json::from_serde(Value::Null));
    let list_to_atom = get(&json, "list_to_atom").unwrap_or_else(|_| Json::from_serde(Value::Null));

    assert_eq!(as_string(&binary_to_atom), Ok(String::from("blocked")));
    assert_eq!(as_string(&list_to_atom), Ok(String::from("blocked")));
    assert_eq!(
        stringify(&json),
        Ok(String::from(
            r#"{"binary_to_atom":"blocked","list_to_atom":"blocked"}"#
        ))
    );
}

/// Validates wrong-kind mutation errors.
///
/// Inputs:
/// - Object used as an array and array used as an object.
///
/// Output:
/// - Test passes when each mutation returns its stable wrong-kind code.
///
/// Transformation:
/// - Converts backend JSON kind mismatches into portable JSON errors.
#[test]
fn mutable_builders_reject_wrong_receiver_kind() {
    let mut not_array = object();
    let array_error = push(&mut not_array, null())
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(array_error.code(), "json.not_array");

    let mut not_object = array();
    let object_error = put(&mut not_object, "name", string("Ada"))
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(object_error.code(), "json.not_object");
}

#[test]
fn array_set_replaces_one_existing_element_and_rejects_missing_indices() {
    let mut values = parsed_fixture(r#"["Ada","Grace"]"#);
    set(&mut values, 0, string("Margaret")).expect("replace array element");
    assert_eq!(
        stringify(&values).expect("serialize replaced array"),
        r#"["Margaret","Grace"]"#
    );

    let missing = set(&mut values, 2, null())
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(missing.code(), "json.index_out_of_bounds");
}

#[test]
fn object_remove_deletes_existing_members_and_rejects_invalid_requests() {
    let mut value = parsed_fixture(r#"{"name":"Ada","role":"engineer"}"#);
    remove(&mut value, "role").expect("remove object member");
    assert_eq!(stringify(&value), Ok(String::from(r#"{"name":"Ada"}"#)));

    let missing = remove(&mut value, "role")
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(missing.code(), "json.missing_key");

    let mut array = array();
    let wrong_kind = remove(&mut array, "role")
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(wrong_kind.code(), "json.not_object");
}

/// Validates stable parse error conversion.
///
/// Inputs:
/// - Invalid JSON text.
///
/// Output:
/// - Test passes when parsing returns the stable `json.parse` code.
///
/// Transformation:
/// - Converts a backend parser error into the portable JSON error shape.
#[test]
fn parse_error_uses_stable_error_code() {
    let result = parse("{");
    assert!(result.is_err());
    let error = result
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(error.code(), "json.parse");
    assert_eq!(error.offset(), 0);
}

/// Validates object lookup and typed accessors.
///
/// Inputs:
/// - Object JSON text with string, integer, float, boolean, and null fields.
///
/// Output:
/// - Test passes when each accessor returns the expected typed value.
///
/// Transformation:
/// - Exercises `get` plus all typed reader operations.
#[test]
fn object_lookup_supports_typed_accessors() {
    let json = parsed_fixture(r#"{"name":"Ada","count":3,"ratio":1.5,"active":true,"none":null}"#);
    let name = get(&json, "name").unwrap_or_else(|_| Json::from_serde(Value::Null));
    let count = get(&json, "count").unwrap_or_else(|_| Json::from_serde(Value::Null));
    let ratio = get(&json, "ratio").unwrap_or_else(|_| Json::from_serde(Value::Null));
    let active = get(&json, "active").unwrap_or_else(|_| Json::from_serde(Value::Null));
    let none = get(&json, "none").unwrap_or_else(|_| Json::from_serde(Value::Null));

    assert_eq!(as_string(&name), Ok(String::from("Ada")));
    assert_eq!(as_int(&count), Ok(3));
    assert_eq!(as_float(&ratio), Ok(1.5));
    assert_eq!(as_bool(&active), Ok(true));
    assert!(is_null(&none));
}

/// Validates object-key enumeration is deterministic and kind checked.
#[test]
fn object_keys_are_sorted_and_reject_non_objects() {
    let json = parsed_fixture(r#"{"z":1,"a":2}"#);
    assert_eq!(keys(&json), Ok(vec![String::from("a"), String::from("z")]));
    assert_eq!(object_length(&json), Ok(2));

    let error = keys(&parsed_fixture("[]"))
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(error.code(), "json.not_object");
    assert_eq!(
        object_length(&parsed_fixture("[]"))
            .expect_err("array is not an object")
            .code(),
        "json.not_object"
    );
}

/// Validates array length and indexed lookup.
///
/// Inputs:
/// - Array JSON text with string, integer, and boolean elements.
///
/// Output:
/// - Test passes when the array length is returned and indexed values can be
///   read through existing typed accessors.
///
/// Transformation:
/// - Exercises the read side of JSON arrays without exposing `serde_json`
///   values to Terlan-facing callers.
#[test]
fn array_lookup_supports_length_and_indexed_access() {
    let json = parsed_fixture(r#"["Ada",3,true]"#);
    let name = at(&json, 0).unwrap_or_else(|_| Json::from_serde(Value::Null));
    let count = at(&json, 1).unwrap_or_else(|_| Json::from_serde(Value::Null));
    let active = at(&json, 2).unwrap_or_else(|_| Json::from_serde(Value::Null));

    assert_eq!(length(&json), Ok(3));
    assert_eq!(as_string(&name), Ok(String::from("Ada")));
    assert_eq!(as_int(&count), Ok(3));
    assert_eq!(as_bool(&active), Ok(true));
}

/// Validates one-pass typed string projection across mixed JSON array rows.
#[test]
fn string_field_rows_preserve_order_and_malformed_rows() {
    let json = parsed_fixture(r#"[{"name":"Ada","count":3},null,{"name":"Grace"}]"#);
    let rows = string_field_rows(&json, &["name", "count"]).expect("array projection");

    assert_eq!(
        rows,
        vec![
            StringFieldRow {
                object: true,
                values: vec![Some("Ada".to_string()), None],
            },
            StringFieldRow {
                object: false,
                values: vec![None, None],
            },
            StringFieldRow {
                object: true,
                values: vec![Some("Grace".to_string()), None],
            },
        ]
    );
    let error =
        string_field_rows(&parsed_fixture("{}"), &["name"]).expect_err("object must be rejected");
    assert_eq!(error.code(), "json.not_array");
}

#[test]
fn nested_string_field_rows_project_parent_and_child_arrays_once() {
    let json = parse(
        r#"[
            {"module":"alpha","declarations":[{"kind":"function","name":"run"},null]},
            {"module":"empty","declarations":[]},
            {"module":"missing"},
            null
        ]"#,
    )
    .expect("parse nested projection fixture");
    let rows = nested_string_field_rows(&json, &["module"], "declarations", &["kind", "name"])
        .expect("nested projection");

    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].values, vec![Some("alpha".to_string())]);
    assert!(rows[0].child_array);
    assert_eq!(
        rows[0].children[0].values,
        vec![Some("function".to_string()), Some("run".to_string())]
    );
    assert!(!rows[0].children[1].object);
    assert!(rows[1].child_array);
    assert!(rows[1].children.is_empty());
    assert!(!rows[2].child_array);
    assert!(!rows[3].object);
}

#[test]
fn nested_string_field_rows_page_bounds_conversion_work() {
    let json = parse(
        r#"[
            {"module":"zero","declarations":[]},
            {"module":"one","declarations":[]},
            {"module":"two","declarations":[]}
        ]"#,
    )
    .expect("parse paged projection fixture");
    let rows =
        nested_string_field_rows_page(&json, 1, 1, &["module"], "declarations", &["kind", "name"])
            .expect("bounded nested projection");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values, vec![Some("one".to_string())]);
    assert!(nested_string_field_rows_page(
        &json,
        0,
        257,
        &["module"],
        "declarations",
        &["kind", "name"]
    )
    .is_err());
}

#[test]
fn array_extend_appends_exact_source_order_and_is_atomic_on_type_error() {
    let mut target = parsed_fixture("[1]");
    extend(&mut target, parsed_fixture("[2,3]")).expect("extend arrays");
    assert_eq!(target.as_serde(), &serde_json::json!([1, 2, 3]));
    let before = target.clone();
    assert_eq!(
        extend(&mut target, parsed_fixture("{}"))
            .expect_err("reject object extension")
            .code(),
        "json.not_array"
    );
    assert_eq!(target, before);
}

#[test]
fn string_object_rows_build_exact_objects_and_reject_bad_shapes() {
    let json = string_object_rows(
        &["kind", "name"],
        &[
            vec!["module".to_string(), "alpha".to_string()],
            vec!["function".to_string(), "run".to_string()],
        ],
    )
    .expect("string object rows");
    assert_eq!(
        json.as_serde(),
        &serde_json::json!([
            {"kind": "module", "name": "alpha"},
            {"kind": "function", "name": "run"}
        ])
    );
    assert_eq!(
        string_object_rows(&["name", "name"], &[])
            .expect_err("duplicate fields")
            .code(),
        "json.duplicate_field"
    );
    assert_eq!(
        string_object_rows(&["name"], &[vec![]])
            .expect_err("row width")
            .code(),
        "json.row_width_mismatch"
    );
}

/// Validates one-pass optional string projection from an object.
#[test]
fn string_fields_preserve_requested_order_and_missing_values() {
    let json = parsed_fixture(r#"{"name":"Ada","count":3}"#);
    assert_eq!(
        string_fields(&json, &["missing", "name", "count"]),
        Ok(vec![None, Some("Ada".to_string()), None])
    );
    assert_eq!(
        string_fields(&parsed_fixture("[]"), &["name"])
            .expect_err("array must be rejected")
            .code(),
        "json.not_object"
    );
}

/// Validates strict heterogeneous projection without returning JSON handles.
#[test]
fn required_fields_project_owned_values_and_reject_type_drift() {
    let json = parsed_fixture(r#"{"name":"Ada","count":3,"items":[1,2]}"#);
    assert_eq!(
        required_fields(&json, &["name"], &["count"], &["items"]),
        Ok(RequiredFieldProjection {
            strings: vec!["Ada".to_string()],
            ints: vec![3],
            array_lengths: vec![2],
        })
    );
    assert_eq!(
        required_fields(&json, &["count"], &[], &[])
            .expect_err("integer is not a required string")
            .code(),
        "json.not_string"
    );
}

/// Validates one-pass heterogeneous projection for large object arrays.
#[test]
fn required_field_rows_project_owned_scalars_and_report_the_bad_row() {
    let json = parsed_fixture(
        r#"[{"name":"Ada","count":3,"enabled":true},{"name":"Lin","count":4,"enabled":false}]"#,
    );
    assert_eq!(
        required_field_rows(&json, &["name"], &["count"], &["enabled"]),
        Ok(vec![
            RequiredFieldRow {
                strings: vec!["Ada".to_string()],
                ints: vec![3],
                bools: vec![true],
            },
            RequiredFieldRow {
                strings: vec!["Lin".to_string()],
                ints: vec![4],
                bools: vec![false],
            },
        ])
    );
    let error = required_field_rows(
        &parsed_fixture(r#"[{"name":"Ada"},{"name":2}]"#),
        &["name"],
        &[],
        &[],
    )
    .expect_err("second row must fail");
    assert_eq!(error.code(), "json.not_string");
    assert_eq!(error.offset(), 1);
    assert_eq!(
        required_field_rows_page(&json, 1, 1, &["name"], &["count"], &["enabled"]),
        Ok(vec![RequiredFieldRow {
            strings: vec!["Lin".to_string()],
            ints: vec![4],
            bools: vec![false],
        }])
    );
    assert_eq!(
        required_field_rows_page(&json, 0, 257, &[], &[], &[])
            .expect_err("oversized page")
            .code(),
        "json.invalid_page"
    );
}

/// Validates object lookup failure conversion.
///
/// Inputs:
/// - Object JSON text and an absent key.
///
/// Output:
/// - Test passes when lookup returns the stable missing-key code.
///
/// Transformation:
/// - Converts a missing object member into a portable JSON error.
#[test]
fn missing_key_uses_stable_error_code() {
    let json = parsed_fixture(r#"{"name":"Ada"}"#);
    let error = get(&json, "missing")
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(error.code(), "json.key_not_found");
}

/// Validates array lookup failure conversion.
///
/// Inputs:
/// - One array used with an out-of-bounds index.
/// - One object used as an array.
///
/// Output:
/// - Test passes when each failure returns a stable JSON error code.
///
/// Transformation:
/// - Converts backend array lookup failures into portable JSON errors.
#[test]
fn array_lookup_failures_use_stable_error_codes() {
    let json = parsed_fixture(r#"["Ada"]"#);
    let missing = at(&json, 3)
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(missing.code(), "json.index_out_of_bounds");

    let object = parsed_fixture(r#"{"name":"Ada"}"#);
    let not_array = length(&object)
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(not_array.code(), "json.not_array");
}

/// Validates typed accessor failure conversion.
///
/// Inputs:
/// - JSON string value read as an integer.
///
/// Output:
/// - Test passes when the accessor returns the stable wrong-kind code.
///
/// Transformation:
/// - Converts a JSON kind mismatch into a portable JSON error.
#[test]
fn wrong_kind_accessor_uses_stable_error_code() {
    let json = parsed_fixture(r#""Ada""#);
    let error = as_int(&json)
        .err()
        .unwrap_or_else(|| JsonError::new("missing", "", 0));
    assert_eq!(error.code(), "json.not_int");
}
