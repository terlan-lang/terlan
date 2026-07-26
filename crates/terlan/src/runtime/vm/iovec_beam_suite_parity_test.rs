use std::sync::Arc;

use super::{VmIoVector, VmIoVectorError, VmIoVectorLimits};
use crate::runtime::vm::bitstring::VmBitString;
use crate::runtime::vm::driver::{VmDriverDescriptor, VmDriverRuntime};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::ReplValue;

fn limits() -> VmIoVectorLimits {
    VmIoVectorLimits::new(1 << 20, 1 << 12, 1 << 16)
}

fn normalize(value: &ReplValue) -> VmIoVector {
    VmIoVector::from_value(value, limits()).expect("valid VM iodata")
}

fn byte_values(range: std::ops::RangeInclusive<i64>) -> Vec<ReplValue> {
    range.map(ReplValue::Int).collect()
}

#[test]
fn iovec_suite_nested_empty_mixed_and_shared_segments_are_exact() {
    let expected = (1u8..=255).collect::<Vec<_>>();
    let flat = ReplValue::List(byte_values(1..=255));
    let nested = ReplValue::List(vec![
        ReplValue::List(byte_values(1..=63)),
        ReplValue::List(vec![
            ReplValue::List(byte_values(64..=127)),
            ReplValue::List(byte_values(128..=191)),
        ]),
        ReplValue::List(byte_values(192..=255)),
    ]);
    assert_eq!(normalize(&flat).flatten(), expected);
    assert_eq!(normalize(&nested).flatten(), expected);
    assert_eq!(normalize(&ReplValue::List(Vec::new())).flatten(), b"");

    let empty_segments = ReplValue::List(
        (0..8_192)
            .map(|_| ReplValue::Bytes(Arc::from([])))
            .collect(),
    );
    let empty = normalize(&empty_segments);
    assert_eq!(empty.total_len(), 0);
    assert!(empty.segments().is_empty());

    let shared: Arc<[u8]> = Arc::from(&b"shared"[..]);
    let aligned = VmBitString::from_bytes(b"-bits", 40).expect("byte-aligned immutable bitstring");
    let mixed = ReplValue::List(vec![
        ReplValue::Bytes(Arc::from([])),
        ReplValue::List(vec![
            ReplValue::Int(b'h'.into()),
            ReplValue::Int(b'i'.into()),
        ]),
        ReplValue::Bytes(Arc::clone(&shared)),
        ReplValue::BitString(aligned.clone()),
        ReplValue::Bytes(Arc::from([])),
    ]);
    let vector = normalize(&mixed);
    assert_eq!(vector.flatten(), b"hishared-bits");
    assert_eq!(vector.total_len(), 13);
    assert_eq!(vector.segments().len(), 3);
    assert!(Arc::ptr_eq(&shared, &vector.segments()[1]));
    assert_eq!(
        vector
            .as_io_slices()
            .iter()
            .flat_map(|slice| slice.iter().copied())
            .collect::<Vec<_>>(),
        b"hishared-bits"
    );

    let direct = normalize(&ReplValue::Bytes(Arc::clone(&shared)));
    assert_eq!(direct.segments().len(), 1);
    assert!(Arc::ptr_eq(&shared, &direct.segments()[0]));

    let aligned_direct = ReplValue::BitString(aligned);
    let aligned_first = normalize(&aligned_direct);
    let aligned_second = normalize(&aligned_direct);
    assert!(Arc::ptr_eq(
        &aligned_first.segments()[0],
        &aligned_second.segments()[0],
    ));

    let child: Arc<[u8]> = Arc::from(&b"parent-prefix-child-suffix"[14..19]);
    assert_eq!(
        normalize(&ReplValue::Bytes(child)).flatten(),
        b"child",
        "a byte subrange preserves only its logical bytes"
    );

    let normalized_again = ReplValue::List(
        vector
            .segments()
            .iter()
            .map(|segment| ReplValue::Bytes(Arc::clone(segment)))
            .collect(),
    );
    assert_eq!(normalize(&normalized_again), vector);
}

#[test]
fn iovec_suite_rejects_non_bytes_and_bounds_adversarial_shapes_iteratively() {
    for (value, expected) in [
        (ReplValue::Int(-1), VmIoVectorError::InvalidByte(-1)),
        (ReplValue::Int(256), VmIoVectorError::InvalidByte(256)),
        (
            ReplValue::BitString(VmBitString::from_bytes([0x80], 1).expect("one-bit value")),
            VmIoVectorError::NonByteAlignedBitString { bit_len: 1 },
        ),
        (
            ReplValue::Atom("bad".into()),
            VmIoVectorError::UnsupportedValue,
        ),
        (
            ReplValue::Tuple(vec![ReplValue::Int(1)]),
            VmIoVectorError::UnsupportedValue,
        ),
    ] {
        assert_eq!(VmIoVector::from_value(&value, limits()), Err(expected));
    }

    let oversized = ReplValue::List(byte_values(0..=8));
    assert_eq!(
        VmIoVector::from_value(&oversized, VmIoVectorLimits::new(8, 16, 32)),
        Err(VmIoVectorError::ByteLimitExceeded { bytes: 9, limit: 8 })
    );

    let segmented = ReplValue::List(vec![
        ReplValue::Bytes(Arc::from(&b"a"[..])),
        ReplValue::Bytes(Arc::from(&b"b"[..])),
        ReplValue::Bytes(Arc::from(&b"c"[..])),
    ]);
    assert_eq!(
        VmIoVector::from_value(&segmented, VmIoVectorLimits::new(8, 2, 16)),
        Err(VmIoVectorError::SegmentLimitExceeded {
            segments: 3,
            limit: 2,
        })
    );

    let mut deep = ReplValue::Bytes(Arc::from(&b"end"[..]));
    for _ in 0..1_024 {
        deep = ReplValue::List(vec![ReplValue::List(Vec::new()), deep]);
    }
    assert_eq!(normalize(&deep).flatten(), b"end");
    assert!(matches!(
        VmIoVector::from_value(&deep, VmIoVectorLimits::new(8, 8, 1_000)),
        Err(VmIoVectorError::NodeLimitExceeded {
            nodes: 1_001,
            limit: 1_000
        })
    ));
}

#[test]
fn iovec_suite_driver_boundary_is_bounded_and_transactional() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.IovecParity", "owner", 0));
    let mut drivers = VmDriverRuntime::default();
    let driver = drivers
        .open(
            &processes,
            owner,
            VmDriverDescriptor::new("iovec", 64, 8).with_max_command_bytes(8),
        )
        .expect("driver opens");

    let command = ReplValue::List(vec![
        ReplValue::List(vec![ReplValue::Int(1), ReplValue::Int(2)]),
        ReplValue::Bytes(Arc::from(&[3, 4][..])),
        ReplValue::BitString(VmBitString::from_bytes([5, 6], 16).expect("aligned command segment")),
    ]);
    assert_eq!(
        drivers.command_value(driver, owner, &command),
        Ok(vec![1, 2, 3, 4, 5, 6])
    );

    let before = drivers.snapshot(driver).expect("driver snapshot");
    assert!(drivers
        .command_value(
            driver,
            owner,
            &ReplValue::List(vec![
                ReplValue::Bytes(Arc::from(&b"12345678"[..])),
                ReplValue::Int(9),
            ]),
        )
        .is_err());
    assert_eq!(drivers.snapshot(driver), Ok(before));
}
