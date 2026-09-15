//! Branch and failure metadata checked against the shared compiled image.

use super::{success, transition, Fixture, ReplValue, TvmControlFrame, TvmTransitionOperation};

pub(super) fn assert_branches(fixture: &mut Fixture) {
    use ReplValue::{Bool, Float, Int};
    use TvmTransitionOperation::Yield;
    for (name, arguments, expected) in [
        ("typed_empty_collections", vec![], 1),
        ("branch_yield", vec![Bool(false)], 7),
        ("branch_yield_local", vec![Bool(false), Int(40)], 40),
        ("nested_branch_yield", vec![Bool(true), Bool(false)], 12),
        ("nested_branch_yield", vec![Bool(false), Bool(true)], 13),
        ("short_circuit_yield", vec![Bool(false), Bool(true)], 0),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &arguments);
        success(frame, 1, expected);
        fixture
            .runtime
            .ensure_idle()
            .expect("pure branch must not suspend");
    }
    let mut branch_points = Vec::new();
    for (name, arguments, captures, expected) in [
        ("branch_yield", vec![Bool(true)], vec![], 41),
        (
            "branch_yield_local",
            vec![Bool(true), Int(40)],
            vec![41],
            42,
        ),
        ("branch_yield_both", vec![Bool(true)], vec![], 1),
        ("branch_yield_both", vec![Bool(false)], vec![], 2),
        (
            "nested_branch_yield",
            vec![Bool(true), Bool(true)],
            vec![],
            11,
        ),
        (
            "short_circuit_yield",
            vec![Bool(true), Bool(true)],
            vec![1],
            1,
        ),
        (
            "branch_capture_pair",
            vec![Bool(true), Int(20), Int(22)],
            vec![20, 22],
            42,
        ),
        ("branch_yield_only", vec![Bool(true)], vec![], 1),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &arguments);
        let (request, continuation) = transition(frame, Yield, &[], &captures);
        if name == "branch_yield_both" {
            branch_points.push(continuation);
        }
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
            .expect("branch continuation released");
    }
    assert_eq!(branch_points.len(), 2);
    assert_ne!(branch_points[0], branch_points[1]);
    for flag in [false, true] {
        fixture.reset();
        let frame = fixture.call("yield_then_branch", &[Bool(flag)]);
        let (request, first) = transition(frame, Yield, &[], &[i64::from(flag)]);
        let frame = fixture
            .resume(1, request, first, vec![i64::from(flag)])
            .expect("outer branch resume");
        if flag {
            let (same_request, second) = transition(frame, Yield, &[], &[]);
            assert_eq!(same_request, request);
            assert_ne!(first, second);
            success(
                fixture
                    .resume(1, request, second, vec![])
                    .expect("inner branch resume"),
                request,
                21,
            );
        } else {
            success(frame, request, 22);
        }
        fixture
            .runtime
            .ensure_idle()
            .expect("nested branch released");
    }
    fixture.reset();
    let frame = fixture.call("bool_capture_twice", &[Bool(true)]);
    let (request, first) = transition(frame, Yield, &[], &[1]);
    let frame = fixture
        .resume(1, request, first, vec![1])
        .expect("first Boolean resume");
    let (same_request, second) = transition(frame, Yield, &[], &[1]);
    assert_eq!(same_request, request);
    assert_ne!(first, second);
    success(
        fixture
            .resume(1, request, second, vec![1])
            .expect("second Boolean resume"),
        request,
        1,
    );
    fixture
        .runtime
        .ensure_idle()
        .expect("Boolean continuation released");

    for (name, arguments, expected_status) in [
        ("add", vec![Int(i64::MAX), Int(1)], 3),
        ("divide", vec![Int(1), Int(0)], 4),
        ("only_true", vec![Bool(false)], 5),
        ("branch_yield_only", vec![Bool(false)], 5),
        (
            "float_divide",
            vec![Float("1.0".into()), Float("0.0".into())],
            19,
        ),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &arguments);
        let TvmControlFrame::Failure {
            request_id,
            owner_id,
            status,
            ..
        } = frame
        else {
            panic!("{name}: expected native failure, found {frame:?}");
        };
        assert_eq!(request_id, 1);
        assert_eq!(owner_id, 1);
        assert_eq!(status, expected_status, "{name}");
        fixture
            .runtime
            .ensure_idle()
            .expect("failed entry must not remain parked");
    }
}
