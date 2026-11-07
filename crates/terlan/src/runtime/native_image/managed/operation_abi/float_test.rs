//! Tests for actor-owned finite-binary64 managed operations.

use std::num::NonZeroUsize;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ActorHeap, ActorId, HeapLimits, ManagedAggregateDescriptor,
    ManagedFieldType, ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, ManagedString,
    SemanticTypeId, TvmRef,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;

use super::{
    encode_float_acos_operation, encode_float_acosh_operation, encode_float_asin_operation,
    encode_float_asinh_operation, encode_float_atan2_operation, encode_float_atan_operation,
    encode_float_atanh_operation, encode_float_cbrt_operation, encode_float_cos_operation,
    encode_float_cosh_operation, encode_float_exp2_operation, encode_float_exp_operation,
    encode_float_expm1_operation, encode_float_from_string_operation, encode_float_hypot_operation,
    encode_float_log10_operation, encode_float_log1p_operation, encode_float_log2_operation,
    encode_float_log_operation, encode_float_pow_operation, encode_float_round_operation,
    encode_float_sin_operation, encode_float_sinh_operation, encode_float_sqrt_operation,
    encode_float_tan_operation, encode_float_tanh_operation, encode_float_to_int_exact_operation,
    encode_float_to_string_operation, encode_float_trunc_operation, execute_float_operation,
};

const OPTION_FLOAT: &str = "Apply(Option;Float)";
const OPTION_INT: &str = "Apply(Option;Int)";

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
        ManagedAggregateDescriptor::constructor(OPTION_INT, "None", 0, 2, Vec::new())
            .expect("None"),
        ManagedAggregateDescriptor::constructor(
            OPTION_INT,
            "Some",
            1,
            2,
            vec![(Some("value".to_string()), ManagedFieldType::Int)],
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
fn float_atan_and_sqrt_use_finite_scalar_words() {
    let mut heap = heap();
    let layouts = registry();
    let atan = encode_float_atan_operation();
    assert_eq!(
        execute_float_operation(&mut heap, &layouts, &atan, &[float_word(1.0)]),
        Ok(1.0_f64.atan().to_bits())
    );
    let sqrt = encode_float_sqrt_operation();
    assert_eq!(
        execute_float_operation(&mut heap, &layouts, &sqrt, &[float_word(9.0)]),
        Ok(3.0_f64.to_bits())
    );
    assert_eq!(
        execute_float_operation(&mut heap, &layouts, &sqrt, &[float_word(-1.0)]),
        Err(ManagedMemoryError::InvalidManagedScalar)
    );
    for operation in [&atan, &sqrt] {
        assert_eq!(
            execute_float_operation(&mut heap, &layouts, operation, &[float_word(f64::INFINITY)],),
            Err(ManagedMemoryError::InvalidManagedScalar)
        );
    }
}

#[test]
fn graphing_calculator_operations_use_finite_scalar_words() {
    let mut heap = heap();
    let layouts = registry();
    for (operation, input, expected) in [
        (encode_float_round_operation(), 2.5, 3.0),
        (encode_float_trunc_operation(), -2.9, -2.0),
        (encode_float_exp_operation(), 0.0, 1.0),
        (encode_float_log2_operation(), 8.0, 3.0),
        (encode_float_log10_operation(), 100.0, 2.0),
        (encode_float_cbrt_operation(), 27.0, 3.0),
        (encode_float_sin_operation(), 0.0, 0.0),
        (encode_float_cos_operation(), 0.0, 1.0),
        (encode_float_tan_operation(), 0.0, 0.0),
        (encode_float_asin_operation(), 0.0, 0.0),
        (encode_float_acos_operation(), 1.0, 0.0),
        (encode_float_sinh_operation(), 0.0, 0.0),
        (encode_float_cosh_operation(), 0.0, 1.0),
        (encode_float_tanh_operation(), 0.0, 0.0),
        (encode_float_asinh_operation(), 0.0, 0.0),
        (encode_float_acosh_operation(), 1.0, 0.0),
        (encode_float_atanh_operation(), 0.0, 0.0),
        (encode_float_exp2_operation(), 3.0, 8.0),
        (
            encode_float_expm1_operation(),
            0.0000000000000001,
            0.0000000000000001_f64.exp_m1(),
        ),
        (
            encode_float_log1p_operation(),
            0.0000000000000001,
            0.0000000000000001_f64.ln_1p(),
        ),
    ] {
        assert_eq!(
            execute_float_operation(&mut heap, &layouts, &operation, &[float_word(input)]),
            Ok(f64::to_bits(expected))
        );
    }
    for (operation, left, right, expected) in [
        (encode_float_pow_operation(), 2.0, 10.0, 1024.0),
        (encode_float_atan2_operation(), 0.0, 1.0, 0.0),
        (encode_float_hypot_operation(), 3.0, 4.0, 5.0),
    ] {
        assert_eq!(
            execute_float_operation(
                &mut heap,
                &layouts,
                &operation,
                &[float_word(left), float_word(right)],
            ),
            Ok(f64::to_bits(expected))
        );
    }
}

#[test]
fn graphing_calculator_operations_reject_invalid_domains() {
    let mut heap = heap();
    let layouts = registry();
    for (operation, input) in [
        (encode_float_log_operation(), 0.0),
        (encode_float_log2_operation(), -1.0),
        (encode_float_log10_operation(), 0.0),
        (encode_float_asin_operation(), 1.1),
        (encode_float_acos_operation(), -1.1),
        (encode_float_acosh_operation(), 0.9),
        (encode_float_atanh_operation(), 1.0),
        (encode_float_log1p_operation(), -1.0),
        (encode_float_exp_operation(), 1000.0),
        (encode_float_exp2_operation(), 1024.0),
    ] {
        assert_eq!(
            execute_float_operation(&mut heap, &layouts, &operation, &[float_word(input)]),
            Err(ManagedMemoryError::InvalidManagedScalar)
        );
    }
    assert_eq!(
        execute_float_operation(
            &mut heap,
            &layouts,
            &encode_float_pow_operation(),
            &[float_word(-1.0), float_word(0.5)],
        ),
        Err(ManagedMemoryError::InvalidManagedScalar)
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

#[test]
fn exact_float_to_int_conversion_checks_integrality_and_i64_range() {
    let mut heap = heap();
    let layouts = registry();
    let convert = encode_float_to_int_exact_operation(semantic(OPTION_INT));

    for (value, expected) in [
        (0.0, Some(0)),
        (-0.0, Some(0)),
        (42.0, Some(42)),
        (i64::MIN as f64, Some(i64::MIN)),
        (1.5, None),
        (9_223_372_036_854_775_808.0, None),
        (f64::INFINITY, None),
        (f64::NAN, None),
    ] {
        let output = execute_float_operation(&mut heap, &layouts, &convert, &[float_word(value)])
            .map(reference)
            .expect("checked conversion");
        let layout = layouts
            .layout_for_reference(&heap, semantic(OPTION_INT), output)
            .expect("option layout");
        assert_eq!(
            layout.variant_name(),
            Some(if expected.is_some() { "Some" } else { "None" })
        );
        if let Some(expected) = expected {
            let view = heap
                .read_aggregate(output.cast(), &layout)
                .expect("Some value");
            assert_eq!(view.field(0), Ok(ManagedFieldValue::Int(expected)));
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
