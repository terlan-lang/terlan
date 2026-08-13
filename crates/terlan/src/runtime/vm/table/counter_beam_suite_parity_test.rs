use std::sync::Arc;

use super::{VmCounterArray, VmCounterError, VmCounterInfo, VmCounterMode};

#[test]
fn counters_suite_modes_basics_limits_validation_and_slot_isolation_contract() {
    for mode in [VmCounterMode::Atomic, VmCounterMode::WriteConcurrent] {
        let counters = VmCounterArray::new(10, mode).expect("counter array should be created");
        assert_eq!(
            counters.info(),
            VmCounterInfo {
                len: 10,
                logical_bytes: 80,
                mode,
            }
        );
        for index in 0..10 {
            assert_eq!(counters.get(index), Ok(0));
            assert_eq!(counters.add(index, 3), Ok(()));
            assert_eq!(counters.add(index, 14), Ok(()));
            assert_eq!(counters.add(index, -20), Ok(()));
            assert_eq!(counters.get(index), Ok(-3));
            assert_eq!(counters.add(index, 100), Ok(()));
            assert_eq!(counters.sub(index, 20), Ok(()));
            assert_eq!(counters.sub(index, -10), Ok(()));
            assert_eq!(counters.get(index), Ok(87));
            assert_eq!(counters.put(index, -321), Ok(()));
            assert_eq!(counters.get(index), Ok(-321));
        }
        assert!((0..10).all(|index| counters.get(index) == Ok(-321)));
    }

    let limits = VmCounterArray::new(1, VmCounterMode::Atomic).unwrap();
    limits.put(0, i64::MAX as i128).unwrap();
    limits.add(0, 1).unwrap();
    assert_eq!(limits.get(0), Ok(i64::MIN));
    limits.sub(0, 1).unwrap();
    assert_eq!(limits.get(0), Ok(i64::MAX));
    limits.put(0, 0).unwrap();
    limits.add(0, u64::MAX as i128).unwrap();
    assert_eq!(limits.get(0), Ok(-1));

    assert_eq!(
        VmCounterArray::new(0, VmCounterMode::Atomic).unwrap_err(),
        VmCounterError::EmptyArray
    );
    assert_eq!(
        VmCounterArray::new(usize::MAX, VmCounterMode::WriteConcurrent).unwrap_err(),
        VmCounterError::LogicalSizeOverflow
    );
    assert_eq!(
        limits.get(1),
        Err(VmCounterError::IndexOutOfBounds { index: 1, len: 1 })
    );
    assert_eq!(
        limits.put(0, i64::MAX as i128 + 1),
        Err(VmCounterError::ValueOutOfRange)
    );
    assert_eq!(
        limits.add(0, u64::MAX as i128 + 1),
        Err(VmCounterError::DeltaOutOfRange)
    );
    assert_eq!(
        limits.sub(0, i64::MIN as i128 - 1),
        Err(VmCounterError::DeltaOutOfRange)
    );
    assert_eq!(limits.get(0), Ok(-1));

    let independent = Arc::new(VmCounterArray::new(32, VmCounterMode::WriteConcurrent).unwrap());
    std::thread::scope(|scope| {
        for index in 0..32 {
            let independent = Arc::clone(&independent);
            scope.spawn(move || {
                let initial = index as i128 * 197;
                independent.put(index, initial).unwrap();
                for round in 1_i128..=100 {
                    independent.add(index, round * 17 + index as i128).unwrap();
                    independent.sub(index, round * 11).unwrap();
                }
            });
        }
    });
    for index in 0..32 {
        let expected = index as i64 * 197 + 6 * 5_050 + index as i64 * 100;
        assert_eq!(independent.get(index), Ok(expected));
    }
}

#[test]
fn counters_suite_write_concurrency_accumulates_wrapping_deltas_exactly() {
    const CELLS: usize = 100;
    const ROUNDS: usize = 1_000;
    let counters = Arc::new(
        VmCounterArray::new(CELLS, VmCounterMode::WriteConcurrent)
            .expect("write-concurrent counters"),
    );
    let deltas = [-9_999_i128, -511, -1, 1, 7, 513, 65_537, i64::MAX as i128];

    for index in 0..CELLS {
        counters
            .put(index, (index as i64).wrapping_mul(97) as i128)
            .unwrap();
    }
    std::thread::scope(|scope| {
        for delta in deltas {
            let counters = Arc::clone(&counters);
            scope.spawn(move || {
                for _ in 0..ROUNDS {
                    for index in 0..CELLS {
                        counters.add(index, delta).unwrap();
                    }
                }
            });
        }
    });

    let contribution = deltas.into_iter().fold(0_u64, |sum, delta| {
        sum.wrapping_add((delta as u64).wrapping_mul(ROUNDS as u64))
    });
    for index in 0..CELLS {
        let base = (index as i64).wrapping_mul(97) as u64;
        assert_eq!(
            counters.get(index),
            Ok(base.wrapping_add(contribution) as i64)
        );
    }
}
