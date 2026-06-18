use super::ts_type_mapping::*;

/// Verifies TypeScript nullable references map to `Option[T]`.
///
/// Inputs:
/// - A union of `Element`, `null`, and `undefined`.
///
/// Output:
/// - Test passes when the mapper returns `Option[Element]` without skips.
///
/// Transformation:
/// - Exercises the release rule that optional and nullable TypeScript shapes
///   become explicit Terlan `Option` types.
#[test]
fn maps_nullable_named_type_to_option() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::Named("Element".to_string()),
        TsTypeRef::Null,
        TsTypeRef::Undefined,
    ]));

    assert_eq!(mapped.terlan_type.as_deref(), Some("Option[Element]"));
    assert!(mapped.skipped.is_empty());
}

/// Verifies TypeScript optional primitive arrays map through `Option`.
///
/// Inputs:
/// - A union of `Array<string>` and `undefined`.
///
/// Output:
/// - Test passes when the mapper returns `Option[std.js.Array[...]]`.
///
/// Transformation:
/// - Confirms optional wrapping composes with simple array mapping, which is
///   needed for generated DOM collection-style APIs.
#[test]
fn maps_optional_array_to_option_list() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::Array(Box::new(TsTypeRef::Primitive(TsPrimitiveType::String))),
        TsTypeRef::Undefined,
    ]));

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("Option[std.js.Array[std.js.String.JsString]]")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies conservative literal unions remain Terlan unions.
///
/// Inputs:
/// - A TypeScript string-literal union.
///
/// Output:
/// - Test passes when the mapper emits a deterministic Terlan union type.
///
/// Transformation:
/// - Preserves source union shape without inventing enum wrappers during the
///   first generated-binding slice.
#[test]
fn maps_simple_literal_union_to_terlan_union() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::StringLiteral("loading".to_string()),
        TsTypeRef::StringLiteral("complete".to_string()),
    ]));

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("\"complete\" | \"loading\"")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies nullable simple unions are wrapped in `Option`.
///
/// Inputs:
/// - A TypeScript literal union plus `null`.
///
/// Output:
/// - Test passes when the mapper emits `Option[<union>]`.
///
/// Transformation:
/// - Applies optional/null handling after conservative union normalization.
#[test]
fn maps_nullable_simple_union_to_option_union() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::StringLiteral("open".to_string()),
        TsTypeRef::StringLiteral("closed".to_string()),
        TsTypeRef::Null,
    ]));

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("Option[\"closed\" | \"open\"]")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies complex unions fail with a stable manifest reason.
///
/// Inputs:
/// - A union containing a named type and an object type.
///
/// Output:
/// - Test passes when mapping is refused with `ts_bindgen.complex_union`.
///
/// Transformation:
/// - Prevents the generator from silently approximating TypeScript object
///   unions before the richer DOM mapping rules are implemented.
#[test]
fn skips_complex_union_with_stable_reason() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::Named("Element".to_string()),
        TsTypeRef::Object,
    ]));

    assert!(mapped.terlan_type.is_none());
    assert_eq!(mapped.skipped.len(), 1);
    assert_eq!(mapped.skipped[0].reason, "ts_bindgen.complex_union");
    assert!(mapped.skipped[0].source.contains("Object"));
}

/// Verifies broad TypeScript dynamic types are skipped.
///
/// Inputs:
/// - TypeScript `any`.
///
/// Output:
/// - Test passes when mapping is refused with `ts_bindgen.unsupported_any`.
///
/// Transformation:
/// - Keeps generated bindings from introducing untyped escape hatches into the
///   Terlan source surface.
#[test]
fn skips_any_with_stable_reason() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Any);

    assert!(mapped.terlan_type.is_none());
    assert_eq!(mapped.skipped.len(), 1);
    assert_eq!(mapped.skipped[0].reason, "ts_bindgen.unsupported_any");
}

/// Verifies TypeScript primitive mappings use JS-native wrapper types.
///
/// Inputs:
/// - TypeScript `string`, `number`, `boolean`, and `void` primitives.
///
/// Output:
/// - Test passes when JS-owned primitives map to `std.js.*` wrappers and
///   language-neutral primitives map to `std.core.*`.
///
/// Transformation:
/// - Pins the generator rule that DOM bindings do not pretend target-native
///   string/number values are portable core values.
#[test]
fn maps_primitives_to_js_binding_defaults() {
    assert_eq!(
        map_ts_type_to_terlan(&TsTypeRef::Primitive(TsPrimitiveType::String))
            .terlan_type
            .as_deref(),
        Some("std.js.String.JsString")
    );
    assert_eq!(
        map_ts_type_to_terlan(&TsTypeRef::Primitive(TsPrimitiveType::Number))
            .terlan_type
            .as_deref(),
        Some("std.js.Number.JsNumber")
    );
    assert_eq!(
        map_ts_type_to_terlan(&TsTypeRef::Primitive(TsPrimitiveType::Boolean))
            .terlan_type
            .as_deref(),
        Some("std.core.Bool")
    );
    assert_eq!(
        map_ts_type_to_terlan(&TsTypeRef::Primitive(TsPrimitiveType::Void))
            .terlan_type
            .as_deref(),
        Some("std.core.Unit")
    );
}

/// Verifies TypeScript `Promise<T>` maps to a JS-native promise wrapper.
///
/// Inputs:
/// - Generic `Promise<string>` reference.
///
/// Output:
/// - Test passes when the mapper returns `std.js.Promise[std.js.String.JsString]`.
///
/// Transformation:
/// - Pins the explicit JS promise bridge before portable `Task` conversion is
///   implemented.
#[test]
fn maps_promise_generic_to_js_promise() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Generic {
        name: "Promise".to_string(),
        args: vec![TsTypeRef::Primitive(TsPrimitiveType::String)],
    });

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("std.js.Promise[std.js.String.JsString]")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies TypeScript generic references preserve source constructors.
///
/// Inputs:
/// - Generic `ReadonlyMap<string, Element>` reference.
///
/// Output:
/// - Test passes when the mapper emits Terlan bracket generic syntax.
///
/// Transformation:
/// - Keeps non-special generic constructors source-shaped for later module
///   mapping instead of inventing generated aliases.
#[test]
fn maps_generic_reference_with_source_constructor() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Generic {
        name: "ReadonlyMap".to_string(),
        args: vec![
            TsTypeRef::Primitive(TsPrimitiveType::String),
            TsTypeRef::Named("Element".to_string()),
        ],
    });

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("ReadonlyMap[std.js.String.JsString, Element]")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies TypeScript record shapes map to Terlan named tuple types.
///
/// Inputs:
/// - A record with required and optional fields.
///
/// Output:
/// - Test passes when optional fields are wrapped in `Option`.
///
/// Transformation:
/// - Uses Terlan's existing named tuple syntax for anonymous object shapes.
#[test]
fn maps_record_shape_to_named_tuple_type() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Record(vec![
        TsRecordField {
            name: "id".to_string(),
            optional: false,
            ty: TsTypeRef::Primitive(TsPrimitiveType::String),
        },
        TsRecordField {
            name: "count".to_string(),
            optional: true,
            ty: TsTypeRef::Primitive(TsPrimitiveType::Number),
        },
    ]));

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("{id: std.js.String.JsString, count: Option[std.js.Number.JsNumber]}")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies TypeScript callback shapes map to Terlan arrow types.
///
/// Inputs:
/// - A callback accepting a string and returning void.
///
/// Output:
/// - Test passes when the mapper emits `(T) -> Unit` syntax.
///
/// Transformation:
/// - Pins callbacks to the EBNF-defined type-arrow form.
#[test]
fn maps_callback_shape_to_arrow_type() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Callback {
        params: vec![TsTypeRef::Primitive(TsPrimitiveType::String)],
        return_type: Box::new(TsTypeRef::Primitive(TsPrimitiveType::Void)),
    });

    assert_eq!(
        mapped.terlan_type.as_deref(),
        Some("(std.js.String.JsString) -> std.core.Unit")
    );
    assert!(mapped.skipped.is_empty());
}

/// Verifies standalone nullish types are skipped.
///
/// Inputs:
/// - A union containing only `null` and `undefined`.
///
/// Output:
/// - Test passes when the mapper reports `ts_bindgen.nullish_without_value_type`.
///
/// Transformation:
/// - Prevents standalone nullish TypeScript values from becoming an invented
///   Terlan `Option[Unit]` before that rule is explicitly designed.
#[test]
fn skips_standalone_nullish_type() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::Union(vec![
        TsTypeRef::Null,
        TsTypeRef::Undefined,
    ]));

    assert!(mapped.terlan_type.is_none());
    assert_eq!(mapped.skipped.len(), 1);
    assert_eq!(
        mapped.skipped[0].reason,
        "ts_bindgen.nullish_without_value_type"
    );
}

/// Verifies unresolved overload sets are skipped.
///
/// Inputs:
/// - A neutral overload-set marker.
///
/// Output:
/// - Test passes when the mapper reports `ts_bindgen.overload_requires_resolution`.
///
/// Transformation:
/// - Keeps overload resolution as a declaration-level generator step instead of
///   allowing overload sets to masquerade as one type.
#[test]
fn skips_unresolved_overload_set() {
    let mapped = map_ts_type_to_terlan(&TsTypeRef::OverloadSet(vec![
        TsTypeRef::Primitive(TsPrimitiveType::String),
        TsTypeRef::Primitive(TsPrimitiveType::Number),
    ]));

    assert!(mapped.terlan_type.is_none());
    assert_eq!(mapped.skipped.len(), 1);
    assert_eq!(
        mapped.skipped[0].reason,
        "ts_bindgen.overload_requires_resolution"
    );
}
