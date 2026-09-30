use super::*;

fn read(binding: &NativeBinding) -> i64 {
    let NativeValue::Int(value) = (binding.invoke)(&[]).unwrap() else {
        panic!("clock must return an Int")
    };
    value
}

#[test]
fn timestamp_boundaries_are_checked_without_truncation_or_saturation() {
    assert_eq!(unix_time_ns_at(UNIX_EPOCH).unwrap(), 0);
    for nanos in [1, 999_999_999, 1_000_000_000, i64::MAX as u64] {
        let elapsed = Duration::from_nanos(nanos);
        assert_eq!(nanoseconds(elapsed, "test").unwrap(), nanos as i64);
        assert_eq!(unix_time_ns_at(UNIX_EPOCH + elapsed).unwrap(), nanos as i64);
    }
    for elapsed in [Duration::from_nanos(i64::MAX as u64 + 1), Duration::MAX] {
        assert_eq!(
            nanoseconds(elapsed, "test").unwrap_err().code(),
            "dispatch.clock_overflow"
        );
    }
    assert_eq!(
        unix_time_ns_at(UNIX_EPOCH - Duration::from_nanos(1))
            .unwrap_err()
            .code(),
        "dispatch.clock_before_unix_epoch"
    );
    assert_eq!(
        unix_time_ns_at(UNIX_EPOCH + Duration::from_nanos(i64::MAX as u64 + 1))
            .unwrap_err()
            .code(),
        "dispatch.clock_overflow"
    );
}

#[test]
fn bindings_reject_arguments_before_reading_the_clock() {
    for binding in [&UNIX_TIME_NS, &MONOTONIC_TIME_NS] {
        assert_eq!(binding.arity, 0);
        for count in [1, 2, 16] {
            assert_eq!(
                (binding.invoke)(&vec![NativeValue::Unit; count])
                    .unwrap_err()
                    .code(),
                "native_package.arguments"
            );
        }
    }
}

#[test]
fn wall_clock_reads_host_time_in_nanoseconds() {
    let before = unix_time_ns().unwrap();
    let observed = read(&UNIX_TIME_NS);
    let after = unix_time_ns().unwrap();
    // A wall clock can move backwards; bound the sample in either direction.
    assert!((before.min(after)..=before.max(after)).contains(&observed));
}

#[test]
fn monotonic_origin_is_shared_across_threads_and_observations_are_not_cached() {
    let before = monotonic_time_ns().unwrap();
    std::thread::sleep(Duration::from_millis(2));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let mut previous = read(&MONOTONIC_TIME_NS);
                for _ in 0..128 {
                    let current = read(&MONOTONIC_TIME_NS);
                    assert!(current >= previous);
                    previous = current;
                }
                previous
            })
        })
        .collect();
    let samples: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    let after = monotonic_time_ns().unwrap();
    assert!(after > before);
    for sample in samples {
        assert!(sample > before && sample <= after);
    }
}
