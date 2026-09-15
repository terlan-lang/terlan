//! Suspension inside conditions, including evaluation order and authority.

use super::{success, transition, Fixture, ReplValue, TvmControlFrame, TvmTransitionOperation};

pub(super) fn assert_conditions(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int};
    use TvmTransitionOperation::Yield;
    for (name, args, captures, expected) in [
        ("yield_condition", vec![Bool(true)], vec![1], 31),
        ("yield_condition", vec![Bool(false)], vec![0], 32),
        (
            "yield_later_condition",
            vec![Bool(false), Bool(true)],
            vec![1],
            34,
        ),
        (
            "yield_later_condition",
            vec![Bool(false), Bool(false)],
            vec![0],
            35,
        ),
        (
            "yield_condition_capture",
            vec![Bool(true), Int(20), Int(22)],
            vec![1, 20, 22],
            42,
        ),
        (
            "yield_condition_capture",
            vec![Bool(false), Int(20), Int(22)],
            vec![0, 20, 22],
            -2,
        ),
        ("yield_condition_local", vec![Int(10)], vec![10, 11], 10),
        ("yield_condition_local", vec![Int(5)], vec![5, 6], 0),
        ("yield_condition_prefix_error", vec![Int(1)], vec![1], 1),
        (
            "nested_yield_condition",
            vec![Bool(true), Bool(true)],
            vec![1],
            40,
        ),
        (
            "nested_yield_condition",
            vec![Bool(true), Bool(false)],
            vec![0],
            41,
        ),
        (
            "yield_short_circuit_left",
            vec![Bool(false), Bool(true)],
            vec![0, 1],
            0,
        ),
        (
            "yield_short_circuit_left",
            vec![Bool(true), Bool(true)],
            vec![1, 1],
            1,
        ),
        (
            "yield_short_circuit_or_left",
            vec![Bool(false), Bool(true)],
            vec![0, 1],
            1,
        ),
        (
            "yield_short_circuit_or_left",
            vec![Bool(true), Bool(false)],
            vec![1, 0],
            1,
        ),
        (
            "yield_short_circuit_left_guard",
            vec![Bool(false)],
            vec![0],
            0,
        ),
        ("yield_short_circuit_or_guard", vec![Bool(true)], vec![1], 1),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &args);
        let (request, continuation) = transition(frame, Yield, &[], &captures);
        success(
            fixture
                .resume(1, request, continuation, captures)
                .expect(name),
            request,
            expected,
        );
        fixture
            .runtime
            .ensure_idle()
            .expect("condition resumed to completion");
    }
    for (name, args, expected) in [
        ("yield_later_condition", vec![Bool(true), Bool(false)], 33),
        ("nested_yield_condition", vec![Bool(false), Bool(true)], 42),
    ] {
        fixture.reset();
        success(fixture.call(name, &args), 1, expected);
        fixture
            .runtime
            .ensure_idle()
            .expect("unselected condition must not suspend");
    }
    for (name, flag, second_captures, expected) in [
        ("yield_condition_twice", true, vec![1], 36),
        ("yield_condition_twice", false, vec![0], 37),
        ("yield_condition_then_body", true, vec![], 38),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &[Bool(flag)]);
        let (request, first) = transition(frame, Yield, &[], &[i64::from(flag)]);
        let frame = fixture
            .resume(1, request, first, vec![i64::from(flag)])
            .expect(name);
        let (same_request, second) = transition(frame, Yield, &[], &second_captures);
        assert_eq!(same_request, request);
        assert_ne!(first, second, "distinct source suspension sites");
        success(
            fixture
                .resume(1, request, second, second_captures)
                .expect(name),
            request,
            expected,
        );
        fixture
            .runtime
            .ensure_idle()
            .expect("second condition stage completed");
    }
    fixture.reset();
    failure(
        fixture.call("yield_condition_prefix_error", &[Int(0)]),
        1,
        4,
    );
    fixture
        .runtime
        .ensure_idle()
        .expect("prefix failure precedes suspension");
    for (name, flag, status) in [
        ("yield_short_circuit_left_guard", true, 4),
        ("yield_short_circuit_or_guard", false, 4),
        ("yield_condition_only", false, 5),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &[Bool(flag)]);
        let (request, continuation) = transition(frame, Yield, &[], &[i64::from(flag)]);
        failure(
            fixture
                .resume(1, request, continuation, vec![i64::from(flag)])
                .expect(name),
            request,
            status,
        );
        fixture
            .runtime
            .ensure_idle()
            .expect("resumed failure released continuation");
    }
    assert_condition_authority(fixture);
}

fn failure(frame: TvmControlFrame, request: u64, expected: i32) {
    let TvmControlFrame::Failure {
        request_id,
        owner_id,
        status,
        ..
    } = frame
    else {
        panic!("expected native failure, found {frame:?}");
    };
    assert_eq!(request_id, request);
    assert_eq!(owner_id, 1);
    assert_eq!(status, expected);
}

fn assert_condition_authority(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int};
    fixture.reset();
    let frame = fixture.call("yield_condition_capture", &[Bool(true), Int(20), Int(22)]);
    let (request, continuation) =
        transition(frame, TvmTransitionOperation::Yield, &[], &[1, 20, 22]);
    assert!(fixture
        .resume(1, request + 1, continuation, vec![1, 20, 22])
        .is_err());
    for invalid in [vec![1, 20], vec![2, 20, 22]] {
        let descriptor = fixture
            .backend
            .continuation(continuation)
            .expect("condition descriptor");
        let error = super::super::super::validate_continuation_captures(descriptor, &invalid)
            .expect_err("reject malformed condition captures");
        assert!(error.contains("pure_native_continuation_type"), "{error}");
        assert_eq!(fixture.runtime.pending_continuation_count(), 1);
    }
    success(
        fixture
            .resume(1, request, continuation, vec![1, 20, 22])
            .expect("valid condition resume"),
        request,
        42,
    );
    assert!(fixture
        .resume(1, request, continuation, vec![1, 20, 22])
        .is_err());
    fixture
        .runtime
        .ensure_idle()
        .expect("condition authority released");
}
