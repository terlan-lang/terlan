//! Tests for actor-owned request JSON decoding.

use std::num::NonZeroUsize;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ActorHeap, ActorId, HeapLimits, ManagedAggregate,
    ManagedAggregateDescriptor, ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry,
    ManagedMemoryError, ManagedString, SemanticTypeId, TvmRef,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;

use super::{
    encode_json_parse_result_operation, encode_result_is_ok_operation, execute_json_operation,
};

const JSON: &str = "Named(Json)";
const ERROR: &str = "Named(Error)";
const RESULT: &str = "Apply(Result;Named(Json),Named(Error))";

/// Builds one actor heap for bounded JSON operation tests.
fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(111).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

/// Builds the exact JSON, error, and result layouts required by body decoding.
fn registry() -> ManagedLayoutRegistry {
    let string = ManagedFieldType::Reference(semantic("std.core.String"));
    let descriptors = [
        ManagedAggregateDescriptor::tuple(JSON, vec![string]).expect("json"),
        ManagedAggregateDescriptor::record(
            ERROR,
            vec![
                ("code".to_string(), ManagedFieldType::Atom),
                ("message".to_string(), string),
            ],
        )
        .expect("error"),
        ManagedAggregateDescriptor::constructor(
            RESULT,
            "Ok",
            0,
            2,
            vec![(
                Some("value".to_string()),
                ManagedFieldType::Reference(semantic(JSON)),
            )],
        )
        .expect("Ok"),
        ManagedAggregateDescriptor::constructor(
            RESULT,
            "Err",
            1,
            2,
            vec![(
                Some("reason".to_string()),
                ManagedFieldType::Reference(semantic(ERROR)),
            )],
        )
        .expect("Err"),
    ];
    let layouts = descriptors
        .into_iter()
        .map(|descriptor| TvmManagedLayoutDescriptor {
            semantic_id: descriptor.managed().semantic_id().bytes(),
            encoded_layout: encode_aggregate_layout(&descriptor).expect("layout"),
        })
        .collect::<Vec<_>>();
    ManagedLayoutRegistry::from_image(&layouts, &[], &["json.parse".to_string()]).expect("registry")
}

/// Valid JSON becomes an `Ok` value containing canonical actor-owned text.
#[test]
fn parse_result_allocates_canonical_json_and_reports_ok_variant() {
    let mut heap = heap();
    let layouts = registry();
    let input = heap
        .allocate_string("{\"enabled\":true,\"count\":2}")
        .expect("input");
    let parse =
        encode_json_parse_result_operation(semantic(JSON), semantic(RESULT), semantic(ERROR));
    let result = execute_json_operation(&mut heap, &layouts, &parse, &[word(input)])
        .map(reference)
        .expect("parse");
    let is_ok = encode_result_is_ok_operation(semantic(RESULT));
    assert_eq!(
        execute_json_operation(&mut heap, &layouts, &is_ok, &[word(result)]),
        Ok(1)
    );

    let result_layout = layouts
        .layout_for_reference(&heap, semantic(RESULT), result)
        .expect("result layout");
    let ManagedFieldValue::Reference(json) = heap
        .read_aggregate(result.cast(), &result_layout)
        .expect("result")
        .field(0)
        .expect("payload")
    else {
        panic!("JSON payload must be a reference");
    };
    let json_layout = layouts.layouts(semantic(JSON))[0].clone();
    let ManagedFieldValue::Reference(text) = heap
        .read_aggregate(json.cast(), &json_layout)
        .expect("json")
        .field(0)
        .expect("text")
    else {
        panic!("JSON text must be a reference");
    };
    assert_eq!(
        heap.read_string(text.cast::<ManagedString>()),
        Ok("{\"count\":2,\"enabled\":true}")
    );
}

/// Invalid JSON becomes `Err(Error)` and malformed calls fail closed.
#[test]
fn parse_result_allocates_portable_error_and_rejects_invalid_operations() {
    let mut heap = heap();
    let layouts = registry();
    let input = heap.allocate_string("{").expect("input");
    let parse =
        encode_json_parse_result_operation(semantic(JSON), semantic(RESULT), semantic(ERROR));
    let result = execute_json_operation(&mut heap, &layouts, &parse, &[word(input)])
        .map(reference)
        .expect("parse error result");
    let is_ok = encode_result_is_ok_operation(semantic(RESULT));
    assert_eq!(
        execute_json_operation(&mut heap, &layouts, &is_ok, &[word(result)]),
        Ok(0)
    );

    let result_layout = layouts
        .layout_for_reference(&heap, semantic(RESULT), result)
        .expect("result layout");
    let ManagedFieldValue::Reference(error) = heap
        .read_aggregate(result.cast(), &result_layout)
        .expect("result")
        .field(0)
        .expect("payload")
    else {
        panic!("error payload must be a reference");
    };
    let error_layout = layouts.layouts(semantic(ERROR))[0].clone();
    let error = heap
        .read_aggregate(error.cast::<ManagedAggregate>(), &error_layout)
        .expect("error");
    let ManagedFieldValue::Atom(code) = error.field(0).expect("code") else {
        panic!("error code must be an atom");
    };
    assert_eq!(layouts.atom_identity(code), Ok("json.parse"));

    assert_eq!(
        execute_json_operation(&mut heap, &layouts, &parse[..7], &[word(input)]),
        Err(ManagedMemoryError::InvalidManagedOperation)
    );
    assert_eq!(
        execute_json_operation(&mut heap, &layouts, &parse, &[]),
        Err(ManagedMemoryError::InvalidManagedOperation)
    );
    assert_eq!(
        execute_json_operation(&mut heap, &layouts, &is_ok, &[0]),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
}

/// Resolves one stable semantic identity used by the fixture.
fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic")
}

/// Converts one managed reference into its signed callback word.
fn word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

/// Converts one callback result into a nonzero managed reference.
fn reference(value: u64) -> TvmRef<()> {
    TvmRef::from_encoded(NonZeroUsize::new(value as usize).expect("reference"))
}
