//! Tests for bounded actor-owned integer text conversion.

use std::num::NonZeroUsize;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, managed_string_semantic_id, ActorHeap, ActorId, HeapLimits,
    ManagedAggregateDescriptor, ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry,
    ManagedString, SemanticTypeId, TvmRef,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;

use super::{
    encode_int_from_string_base_operation, encode_int_from_string_operation,
    encode_int_to_string_base_operation, encode_int_to_string_operation, execute_integer_operation,
    MAX_INTEGER_PARSE_BYTES,
};

const OPTION_INT: &str = "Apply(Option;Int)";
const OPTION_STRING: &str = "Apply(Option;String)";

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(118).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

fn registry() -> ManagedLayoutRegistry {
    let descriptors = [
        ManagedAggregateDescriptor::constructor(OPTION_INT, "None", 0, 2, Vec::new())
            .expect("None Int"),
        ManagedAggregateDescriptor::constructor(
            OPTION_INT,
            "Some",
            1,
            2,
            vec![(Some("value".to_string()), ManagedFieldType::Int)],
        )
        .expect("Some Int"),
        ManagedAggregateDescriptor::constructor(OPTION_STRING, "None", 0, 2, Vec::new())
            .expect("None String"),
        ManagedAggregateDescriptor::constructor(
            OPTION_STRING,
            "Some",
            1,
            2,
            vec![(
                Some("value".to_string()),
                ManagedFieldType::Reference(managed_string_semantic_id()),
            )],
        )
        .expect("Some String"),
    ];
    let layouts = descriptors
        .into_iter()
        .map(|descriptor| TvmManagedLayoutDescriptor {
            semantic_id: descriptor.managed().semantic_id().bytes(),
            encoded_layout: encode_aggregate_layout(&descriptor).expect("layout"),
        })
        .collect::<Vec<_>>();
    ManagedLayoutRegistry::from_image(&layouts, &[], &[]).expect("registry")
}

#[test]
fn integer_decimal_conversion_is_strict_bounded_and_overflow_safe() {
    let mut heap = heap();
    let layouts = registry();
    let parse = encode_int_from_string_operation(semantic(OPTION_INT));

    for (text, expected) in [
        ("12373", Some(12373)),
        ("-12373", Some(-12373)),
        ("+12373", Some(12373)),
        ("12373ABC", None),
        ("", None),
        ("9223372036854775808", None),
    ] {
        assert_option_int(&mut heap, &layouts, &parse, text, expected, "decimal parse");
    }
    let over_limit = "1".repeat(MAX_INTEGER_PARSE_BYTES + 1);
    assert_option_int(
        &mut heap,
        &layouts,
        &parse,
        &over_limit,
        None,
        "bounded parse",
    );

    let render = encode_int_to_string_operation();
    for value in [0, -1, i64::MIN, i64::MAX] {
        let output = execute_integer_operation(&mut heap, &layouts, &render, &[value])
            .map(reference)
            .expect("render");
        assert_eq!(
            heap.read_string(output.cast::<ManagedString>()),
            Ok(value.to_string().as_str())
        );
    }
}

#[test]
fn integer_base_conversion_accepts_two_through_thirty_six_atomically() {
    let mut heap = heap();
    let layouts = registry();
    let parse = encode_int_from_string_base_operation(semantic(OPTION_INT));
    let render = encode_int_to_string_base_operation(semantic(OPTION_STRING));

    for (text, base, expected) in [
        ("FF", 16, Some(255)),
        ("ff", 16, Some(255)),
        ("-10", 2, Some(-2)),
        ("Z", 36, Some(35)),
        ("2", 2, None),
        ("10tail", 10, None),
        ("10", 1, None),
        ("10", 37, None),
    ] {
        let input = heap.allocate_string(text).expect("input");
        let output = execute_integer_operation(&mut heap, &layouts, &parse, &[word(input), base])
            .map(reference)
            .expect("parse base");
        assert_option_int_reference(&heap, &layouts, output, expected);
    }

    for (value, base, expected) in [
        (255, 16, Some("FF")),
        (-2, 2, Some("-10")),
        (35, 36, Some("Z")),
        (10, 1, None),
        (10, 37, None),
    ] {
        let output = execute_integer_operation(&mut heap, &layouts, &render, &[value, base])
            .map(reference)
            .expect("render base");
        assert_option_string(&heap, &layouts, output, expected);
    }
}

fn assert_option_int(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    operation: &[u8],
    text: &str,
    expected: Option<i64>,
    context: &str,
) {
    let input = heap.allocate_string(text).expect("input");
    let output = execute_integer_operation(heap, layouts, operation, &[word(input)])
        .map(reference)
        .expect(context);
    assert_option_int_reference(heap, layouts, output, expected);
}

fn assert_option_int_reference(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    output: TvmRef<()>,
    expected: Option<i64>,
) {
    let layout = layouts
        .layout_for_reference(heap, semantic(OPTION_INT), output)
        .expect("Option[Int] layout");
    assert_eq!(
        layout.variant_name(),
        Some(if expected.is_some() { "Some" } else { "None" })
    );
    if let Some(expected) = expected {
        let view = heap
            .read_aggregate(output.cast(), &layout)
            .expect("Some Int");
        assert_eq!(view.field(0), Ok(ManagedFieldValue::Int(expected)));
    }
}

fn assert_option_string(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    output: TvmRef<()>,
    expected: Option<&str>,
) {
    let layout = layouts
        .layout_for_reference(heap, semantic(OPTION_STRING), output)
        .expect("Option[String] layout");
    assert_eq!(
        layout.variant_name(),
        Some(if expected.is_some() { "Some" } else { "None" })
    );
    if let Some(expected) = expected {
        let view = heap
            .read_aggregate(output.cast(), &layout)
            .expect("Some String");
        let ManagedFieldValue::Reference(value) = view.field(0).expect("field") else {
            panic!("expected managed string reference");
        };
        assert_eq!(heap.read_string(value.cast()), Ok(expected));
    }
}

fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic")
}

fn word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

fn reference(value: u64) -> TvmRef<()> {
    TvmRef::from_encoded(NonZeroUsize::new(value as usize).expect("reference"))
}
