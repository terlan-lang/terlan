//! Tests for actor-owned finite-binary64 managed operations.

use std::num::NonZeroUsize;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ActorHeap, ActorId, HeapLimits, ManagedAggregateDescriptor,
    ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry, ManagedString, SemanticTypeId,
    TvmRef,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;

use super::{
    encode_float_from_string_operation, encode_float_log_operation,
    encode_float_to_string_operation, execute_float_operation,
};

const OPTION_FLOAT: &str = "Apply(Option;Float)";

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(117).expect("actor"),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

fn registry() -> ManagedLayoutRegistry {
    let descriptors = [
        ManagedAggregateDescriptor::constructor(OPTION_FLOAT, "None", 0, 2, Vec::new())
            .expect("None"),
        ManagedAggregateDescriptor::constructor(
            OPTION_FLOAT,
            "Some",
            1,
            2,
            vec![(Some("value".to_string()), ManagedFieldType::Float)],
        )
        .expect("Some"),
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
fn float_format_and_log_use_exact_scalar_words() {
    let mut heap = heap();
    let layouts = registry();
    let format = encode_float_to_string_operation();
    let formatted = execute_float_operation(&mut heap, &layouts, &format, &[float_word(-2.25)])
        .map(reference)
        .expect("format");
    assert_eq!(
        heap.read_string(formatted.cast::<ManagedString>()),
        Ok("-2.25")
    );

    let log = encode_float_log_operation();
    assert_eq!(
        execute_float_operation(&mut heap, &layouts, &log, &[float_word(1.0)]),
        Ok(0.0_f64.to_bits())
    );
}

#[test]
fn float_parse_allocates_some_none_and_accepts_decimal_underflow() {
    let mut heap = heap();
    let layouts = registry();
    let parse = encode_float_from_string_operation(semantic(OPTION_FLOAT));

    for (text, expected) in [
        ("1.5", Some(1.5)),
        ("1.0e-325", Some(0.0)),
        ("nope", None),
        ("1.0e83291083210", None),
    ] {
        let input = heap.allocate_string(text).expect("input");
        let output = execute_float_operation(&mut heap, &layouts, &parse, &[word(input)])
            .map(reference)
            .expect("parse");
        let layout = layouts
            .layout_for_reference(&heap, semantic(OPTION_FLOAT), output)
            .expect("option layout");
        assert_eq!(
            layout.variant_name(),
            Some(if expected.is_some() { "Some" } else { "None" })
        );
        if let Some(expected) = expected {
            let view = heap
                .read_aggregate(output.cast(), &layout)
                .expect("Some value");
            assert_eq!(view.field(0), Ok(ManagedFieldValue::Float(expected)));
        }
    }
}

fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic")
}

fn float_word(value: f64) -> i64 {
    i64::from_ne_bytes(value.to_bits().to_ne_bytes())
}

fn word<T>(reference: TvmRef<T>) -> i64 {
    i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes())
}

fn reference(value: u64) -> TvmRef<()> {
    TvmRef::from_encoded(NonZeroUsize::new(value as usize).expect("reference"))
}
