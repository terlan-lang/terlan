use std::sync::Arc;
use std::thread;

use super::{VmAtomicArray, VmAtomicError, VmAtomicKind, VmAtomicValue, VmCompareExchange};

fn signed(value: i64) -> VmAtomicValue {
    VmAtomicValue::Signed(value)
}

fn unsigned(value: u64) -> VmAtomicValue {
    VmAtomicValue::Unsigned(value)
}

#[test]
fn atomics_suite_signed_unsigned_limits_validation_and_concurrency_contract() {
    let signed_cells = VmAtomicArray::new(10, VmAtomicKind::Signed)
        .expect("signed atomic array should be created");
    assert_eq!(signed_cells.len(), 10);
    assert_eq!(signed_cells.kind(), VmAtomicKind::Signed);
    assert_eq!(signed_cells.limits(), (signed(i64::MIN), signed(i64::MAX)));

    for index in 0..signed_cells.len() {
        assert_eq!(signed_cells.get(index), Ok(signed(0)));
        assert_eq!(signed_cells.put(index, signed(3)), Ok(()));
        assert_eq!(signed_cells.add(index, 14), Ok(()));
        assert_eq!(signed_cells.get(index), Ok(signed(17)));
        assert_eq!(signed_cells.add_get(index, 3), Ok(signed(20)));
        assert_eq!(signed_cells.add_get(index, -23), Ok(signed(-3)));
        assert_eq!(signed_cells.add_get(index, 20), Ok(signed(17)));
        assert_eq!(signed_cells.sub(index, 4), Ok(()));
        assert_eq!(signed_cells.get(index), Ok(signed(13)));
        assert_eq!(signed_cells.sub_get(index, 20), Ok(signed(-7)));
        assert_eq!(signed_cells.sub_get(index, -10), Ok(signed(3)));
        assert_eq!(signed_cells.exchange(index, signed(666)), Ok(signed(3)));
        assert_eq!(
            signed_cells.compare_exchange(index, signed(666), signed(777)),
            Ok(VmCompareExchange::Exchanged)
        );
        assert_eq!(
            signed_cells.compare_exchange(index, signed(666), signed(-666)),
            Ok(VmCompareExchange::Mismatch(signed(777)))
        );
    }

    let unsigned_cells = VmAtomicArray::new(10, VmAtomicKind::Unsigned)
        .expect("unsigned atomic array should be created");
    assert_eq!(unsigned_cells.kind(), VmAtomicKind::Unsigned);
    assert_eq!(unsigned_cells.limits(), (unsigned(0), unsigned(u64::MAX)));
    for index in 0..unsigned_cells.len() {
        assert_eq!(unsigned_cells.get(index), Ok(unsigned(0)));
        assert_eq!(unsigned_cells.put(index, unsigned(3)), Ok(()));
        assert_eq!(unsigned_cells.add(index, 14), Ok(()));
        assert_eq!(unsigned_cells.add_get(index, 3), Ok(unsigned(20)));
        assert_eq!(unsigned_cells.sub(index, 7), Ok(()));
        assert_eq!(unsigned_cells.sub_get(index, 10), Ok(unsigned(3)));
        assert_eq!(
            unsigned_cells.exchange(index, unsigned(666)),
            Ok(unsigned(3))
        );
        assert_eq!(
            unsigned_cells.compare_exchange(index, unsigned(666), unsigned(777)),
            Ok(VmCompareExchange::Exchanged)
        );
        assert_eq!(
            unsigned_cells.compare_exchange(index, unsigned(666), unsigned(888)),
            Ok(VmCompareExchange::Mismatch(unsigned(777)))
        );
    }

    let signed_limit = VmAtomicArray::new(1, VmAtomicKind::Signed).unwrap();
    assert_eq!(signed_limit.add(0, i64::MAX as i128), Ok(()));
    assert_eq!(signed_limit.add_get(0, 1), Ok(signed(i64::MIN)));
    assert_eq!(signed_limit.sub_get(0, 1), Ok(signed(i64::MAX)));
    signed_limit.put(0, signed(0)).unwrap();
    assert_eq!(signed_limit.add(0, u64::MAX as i128), Ok(()));
    assert_eq!(signed_limit.get(0), Ok(signed(-1)));

    let unsigned_limit = VmAtomicArray::new(1, VmAtomicKind::Unsigned).unwrap();
    assert_eq!(unsigned_limit.add(0, u64::MAX as i128), Ok(()));
    assert_eq!(unsigned_limit.add_get(0, 1), Ok(unsigned(0)));
    assert_eq!(unsigned_limit.sub_get(0, 1), Ok(unsigned(u64::MAX)));
    unsigned_limit
        .put(0, unsigned(1_u64 << 63))
        .expect("high unsigned value should be accepted");
    assert_eq!(unsigned_limit.add(0, i64::MIN as i128), Ok(()));
    assert_eq!(unsigned_limit.get(0), Ok(unsigned(0)));

    assert_eq!(
        VmAtomicArray::new(0, VmAtomicKind::Signed).unwrap_err(),
        VmAtomicError::EmptyArray
    );
    assert_eq!(
        signed_limit.get(1),
        Err(VmAtomicError::IndexOutOfBounds { index: 1, len: 1 })
    );
    assert_eq!(
        signed_limit.put(0, unsigned(1)),
        Err(VmAtomicError::ValueKindMismatch)
    );
    assert_eq!(
        signed_limit.add(0, u64::MAX as i128 + 1),
        Err(VmAtomicError::DeltaOutOfRange)
    );
    assert_eq!(
        signed_limit.sub(0, i64::MIN as i128 - 1),
        Err(VmAtomicError::DeltaOutOfRange)
    );
    assert_eq!(signed_limit.get(0), Ok(signed(-1)));

    let concurrent = Arc::new(VmAtomicArray::new(2, VmAtomicKind::Unsigned).unwrap());
    let mut workers = Vec::new();
    for worker in 0..8 {
        let concurrent = Arc::clone(&concurrent);
        workers.push(thread::spawn(move || {
            for _ in 0..5_000 {
                if worker % 2 == 0 {
                    concurrent.add(0, 1).unwrap();
                } else {
                    loop {
                        let current = concurrent.get(1).unwrap();
                        let VmAtomicValue::Unsigned(current) = current else {
                            unreachable!("unsigned array returned a signed value");
                        };
                        if concurrent
                            .compare_exchange(1, unsigned(current), unsigned(current + 1))
                            .unwrap()
                            == VmCompareExchange::Exchanged
                        {
                            break;
                        }
                    }
                }
            }
        }));
    }
    for worker in workers {
        worker.join().expect("atomic worker should not panic");
    }
    assert_eq!(concurrent.get(0), Ok(unsigned(20_000)));
    assert_eq!(concurrent.get(1), Ok(unsigned(20_000)));
}
