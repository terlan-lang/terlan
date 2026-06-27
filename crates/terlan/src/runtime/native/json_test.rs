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
