use super::{convert_time_unit, VmTimeResolution};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::reference::VmReferenceAllocator;
use crate::runtime::vm::scheduler::{VmScheduler, VmSchedulerConfig};
use crate::runtime::vm::timer::{VmTimerEvent, VmTimerKind, VmTimerTable};

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.TimeSuiteParity", function, 0)
}

#[test]
fn time_suite_signed_unit_conversion_contract() {
    let second = VmTimeResolution::SECOND;
    let millisecond = VmTimeResolution::MILLISECOND;
    let microsecond = VmTimeResolution::MICROSECOND;
    let nanosecond = VmTimeResolution::NANOSECOND;

    let exact_cases = [
        (1, second, millisecond, 1_000),
        (-1, second, millisecond, -1_000),
        (1_999, millisecond, second, 1),
        (-1_999, millisecond, second, -2),
        (1_234_567, microsecond, millisecond, 1_234),
        (-1_234_567, microsecond, millisecond, -1_235),
        (1, nanosecond, microsecond, 0),
        (-1, nanosecond, microsecond, -1),
    ];
    for (value, from, to, expected) in exact_cases {
        assert_eq!(
            convert_time_unit(value, from, to).expect("convert exact time case"),
            expected
        );
    }

    let resolutions = [
        second,
        VmTimeResolution::new(10).expect("decimal resolution"),
        VmTimeResolution::new(17).expect("prime resolution"),
        VmTimeResolution::new(4_711).expect("custom resolution"),
        millisecond,
        microsecond,
        nanosecond,
    ];
    let values = [
        -1_000_000_i128,
        -65_537,
        -1_001,
        -1,
        0,
        1,
        1_001,
        65_537,
        1_000_000,
    ];
    for from in resolutions {
        for to in resolutions {
            let mut previous = None;
            for value in values {
                let converted = convert_time_unit(value, from, to).expect("bounded conversion");
                let expected = (value * i128::from(to.units_per_second()))
                    .div_euclid(i128::from(from.units_per_second()));
                assert_eq!(converted, expected);
                if let Some(previous) = previous {
                    assert!(previous <= converted);
                }
                previous = Some(converted);
            }
        }
    }

    assert_eq!(
        VmTimeResolution::new(0),
        Err("VM time resolution must be non-zero".to_string())
    );
    assert_eq!(
        convert_time_unit(i128::MAX, second, nanosecond),
        Err(format!(
            "VM time conversion overflow for value {} from resolution 1 to 1000000000",
            i128::MAX
        ))
    );
}

#[test]
fn time_suite_logical_clock_and_unique_sequence_contract() {
    let mut references =
        VmReferenceAllocator::new("time-suite@local", 1).expect("time-suite reference allocator");
    let mut previous = 0;
    for _ in 0..10_000 {
        let next = references
            .allocate_unique_integer()
            .expect("monotonic unique time-adjacent value");
        assert!(next > previous);
        previous = next;
    }

    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("logical-clock"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 100));
    let mut timers = VmTimerTable::default();
    let timer = timers
        .start_one_shot(&processes, owner, 20)
        .expect("pending logical timer");

    for tick in [0, 1, 1, 5, 10, 19] {
        assert!(timers
            .advance_clock(&mut processes, &mut scheduler, tick)
            .is_empty());
        assert_eq!(timers.current_tick(), tick);
    }
    assert!(timers
        .advance_clock(&mut processes, &mut scheduler, 18)
        .is_empty());
    assert_eq!(timers.current_tick(), 19);
    assert_eq!(timers.snapshots().len(), 1);
    assert_eq!(timers.metrics().clock_drift_rejections.len(), 1);
    assert_eq!(
        timers.advance_clock(&mut processes, &mut scheduler, 20),
        vec![VmTimerEvent::Fired {
            timer_id: timer,
            owner,
            kind: VmTimerKind::OneShot,
        }]
    );
    assert!(timers.snapshots().is_empty());
}
