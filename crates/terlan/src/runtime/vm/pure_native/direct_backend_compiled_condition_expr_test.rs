//! Evaluation order, short-circuiting, and Unit values across suspension.

use super::{success, transition, Fixture, ReplValue, TvmControlFrame, TvmTransitionOperation};

pub(super) fn assert_condition_expressions(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int, Unit};
    for (name, args, stages, outcome) in [
        (
            "yield_comparison_condition",
            vec![Int(11)],
            vec![vec![11]],
            Ok(51),
        ),
        (
            "yield_comparison_condition",
            vec![Int(10)],
            vec![vec![10]],
            Ok(52),
        ),
        (
            "yield_not_condition",
            vec![Bool(false)],
            vec![vec![0]],
            Ok(53),
        ),
        (
            "yield_not_condition",
            vec![Bool(true)],
            vec![vec![1]],
            Ok(54),
        ),
        (
            "yield_and_rhs",
            vec![Bool(false), Bool(true)],
            vec![],
            Ok(56),
        ),
        (
            "yield_and_rhs",
            vec![Bool(true), Bool(true)],
            vec![vec![1]],
            Ok(55),
        ),
        (
            "yield_and_rhs",
            vec![Bool(true), Bool(false)],
            vec![vec![0]],
            Ok(56),
        ),
        (
            "yield_or_rhs",
            vec![Bool(true), Bool(false)],
            vec![],
            Ok(57),
        ),
        (
            "yield_or_rhs",
            vec![Bool(false), Bool(true)],
            vec![vec![1]],
            Ok(57),
        ),
        (
            "yield_or_rhs",
            vec![Bool(false), Bool(false)],
            vec![vec![0]],
            Ok(58),
        ),
        (
            "yield_and_rhs_only",
            vec![Bool(false), Bool(true)],
            vec![],
            Err(5),
        ),
        (
            "yield_and_rhs_only",
            vec![Bool(true), Bool(false)],
            vec![vec![0]],
            Err(5),
        ),
        (
            "yield_and_rhs_then_body",
            vec![Bool(true), Bool(true)],
            vec![vec![1], vec![]],
            Ok(60),
        ),
        (
            "yield_or_rhs_then_body",
            vec![Bool(true), Bool(false)],
            vec![vec![]],
            Ok(74),
        ),
        (
            "yield_or_rhs_then_body",
            vec![Bool(false), Bool(false)],
            vec![vec![0]],
            Ok(75),
        ),
        (
            "yield_or_rhs_then_body",
            vec![Bool(false), Bool(true)],
            vec![vec![1], vec![]],
            Ok(74),
        ),
        (
            "yield_nested_and_rhs",
            vec![Bool(false), Bool(true), Bool(true)],
            vec![],
            Ok(63),
        ),
        (
            "yield_nested_and_rhs",
            vec![Bool(true), Bool(false), Bool(true)],
            vec![],
            Ok(63),
        ),
        (
            "yield_nested_and_rhs",
            vec![Bool(true), Bool(true), Bool(true)],
            vec![vec![1]],
            Ok(62),
        ),
        (
            "yield_nested_and_rhs",
            vec![Bool(true), Bool(true), Bool(false)],
            vec![vec![0]],
            Ok(63),
        ),
        (
            "checked_before_yield_rhs",
            vec![Int(0), Bool(true)],
            vec![],
            Err(4),
        ),
        (
            "checked_before_yield_rhs",
            vec![Int(1), Bool(true)],
            vec![vec![1]],
            Ok(64),
        ),
        (
            "yield_eager_right_condition",
            vec![Int(0), Int(1)],
            vec![],
            Err(4),
        ),
        (
            "yield_eager_right_condition",
            vec![Int(1), Int(0)],
            vec![vec![0, 1]],
            Ok(68),
        ),
        (
            "yield_eager_right_condition",
            vec![Int(2), Int(3)],
            vec![vec![3, 0]],
            Ok(69),
        ),
        ("yield_eager_value", vec![Int(0), Int(5)], vec![], Err(4)),
        (
            "yield_eager_value",
            vec![Int(2), Int(5)],
            vec![vec![5, 0]],
            Ok(5),
        ),
        ("yield_unary_value", vec![Int(3)], vec![vec![3]], Ok(-3)),
        (
            "yield_second_call_argument",
            vec![Int(0), Int(5)],
            vec![],
            Err(4),
        ),
        (
            "yield_second_call_argument",
            vec![Int(2), Int(5)],
            vec![vec![5, 0]],
            Ok(5),
        ),
        (
            "yield_first_call_argument",
            vec![Int(5), Int(0)],
            vec![vec![5, 0]],
            Err(4),
        ),
        ("yield_only", vec![], vec![vec![]], Ok(0)),
        ("unit_identity", vec![Unit], vec![], Ok(0)),
        (
            "two_unit_effects",
            vec![Int(43)],
            vec![vec![], vec![]],
            Ok(43),
        ),
        ("eight_unit_effects", vec![Int(44)], vec![vec![]; 8], Ok(44)),
        ("yield_unit_then_int", vec![Int(41)], vec![vec![]], Ok(42)),
        ("tail_yield_only", vec![], vec![vec![]], Ok(0)),
        ("yield_unit_capture", vec![Unit], vec![vec![0]], Ok(0)),
    ] {
        assert_stages(fixture, name, &args, &stages, outcome);
    }
    assert_capture_admission(fixture);
}

pub(super) fn assert_stages(
    fixture: &mut Fixture,
    name: &str,
    args: &[ReplValue],
    stages: &[Vec<i64>],
    outcome: Result<i64, i32>,
) {
    assert_observed_stages(fixture, name, args, stages, outcome, |_, _, _| {});
}

pub(super) fn assert_observed_stages(
    fixture: &mut Fixture,
    name: &str,
    args: &[ReplValue],
    stages: &[Vec<i64>],
    outcome: Result<i64, i32>,
    mut observe: impl FnMut(&mut Fixture, usize, u64),
) {
    fixture.reset();
    let mut frame = fixture.call(name, args);
    for (index, captures) in stages.iter().enumerate() {
        let (request, continuation) =
            transition(frame, TvmTransitionOperation::Yield, &[], captures);
        if matches!(
            name,
            "two_unit_effects" | "eight_unit_effects" | "yield_unit_then_int"
        ) {
            // A delegated Unit callee has no scalar captures. The enclosing
            // Int survives in the actor-owned caller frame, not the callee.
            let completions = fixture.runtime.pending_completion_frames_for_test(1);
            assert_eq!(completions.len(), 1, "{name}: stage {index}");
            let ReplValue::Int(value) = args[0] else {
                panic!("Int caller fixture")
            };
            assert_eq!(completions[0].scalar_captures, vec![value]);
            assert_ne!(completions[0].continuation_id, continuation);
        }
        assert_eq!(
            request, 1,
            "{name}: stage {index} belongs to original request"
        );
        observe(fixture, index, continuation);
        frame = fixture
            .resume(1, request, continuation, captures.clone())
            .expect(name);
    }
    match outcome {
        Ok(expected) => success(frame, 1, expected),
        Err(expected) => {
            let TvmControlFrame::Failure {
                request_id,
                owner_id,
                status,
                ..
            } = frame
            else {
                panic!("{name}: expected failure {expected}, found {frame:?}");
            };
            assert_eq!(request_id, 1);
            assert_eq!(owner_id, 1);
            assert_eq!(status, expected, "{name}");
        }
    }
    fixture
        .runtime
        .ensure_idle()
        .expect("all expected stages complete");
}

fn assert_capture_admission(fixture: &mut Fixture) {
    for (name, args, captures, invalid, expected) in [
        (
            "yield_and_rhs",
            vec![ReplValue::Bool(true), ReplValue::Bool(true)],
            vec![1],
            vec![2],
            55,
        ),
        (
            "yield_unit_capture",
            vec![ReplValue::Unit],
            vec![0],
            vec![1],
            0,
        ),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &args);
        let (request, continuation) =
            transition(frame, TvmTransitionOperation::Yield, &[], &captures);
        for (owner, request) in [(1, request + 1), (0, request)] {
            assert!(fixture
                .resume(owner, request, continuation, captures.clone())
                .is_err());
        }
        let descriptor = fixture
            .backend
            .continuation(continuation)
            .expect("capture descriptor");
        let error = super::super::super::validate_continuation_captures(descriptor, &invalid)
            .expect_err("malformed Boolean/Unit capture");
        assert!(error.contains("pure_native_continuation_type"), "{error}");
        assert_eq!(fixture.runtime.pending_continuation_count(), 1);
        success(
            fixture
                .resume(1, request, continuation, captures.clone())
                .expect(name),
            request,
            expected,
        );
        assert!(fixture.resume(1, request, continuation, captures).is_err());
        fixture
            .runtime
            .ensure_idle()
            .expect("valid owner completed");
    }
}
