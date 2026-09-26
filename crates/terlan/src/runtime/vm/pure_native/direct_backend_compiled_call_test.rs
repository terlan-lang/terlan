//! Non-tail completion values remain on the actor-owned caller stack.

use super::condition_expr::{assert_observed_stages, assert_stages};
use super::{Fixture, ReplValue};

pub(super) type CallStage = (Vec<i64>, Vec<Vec<i64>>);

/// Returns callee and caller code identities observed during this sole execution.
pub(super) fn assert_call_stages(
    fixture: &mut Fixture,
    name: &str,
    args: &[ReplValue],
    stages: &[CallStage],
    outcome: Result<i64, i32>,
) -> Vec<(u64, Vec<u64>)> {
    let captures = stages
        .iter()
        .map(|(values, _)| values.clone())
        .collect::<Vec<_>>();
    let mut identities = Vec::new();
    assert_observed_stages(
        fixture,
        name,
        args,
        &captures,
        outcome,
        |fixture, index, callee| {
            let callers = fixture.runtime.pending_completion_frames_for_test(1);
            let values = callers
                .iter()
                .map(|frame| frame.scalar_captures.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                values, stages[index].1,
                "{name}: caller captures at stage {index}"
            );
            let ids = callers
                .iter()
                .map(|frame| frame.continuation_id)
                .collect::<Vec<_>>();
            assert!(
                ids.iter().all(|id| *id != callee),
                "{name}: caller and callee code are distinct"
            );
            identities.push((callee, ids));
        },
    );
    identities
}

pub(super) fn assert_non_tail_calls(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int};
    let callee = assert_call_stages(
        fixture,
        "branch_yield",
        &[Bool(true)],
        &[(vec![], vec![])],
        Ok(41),
    )[0]
    .0;
    let mut completion = None;
    for (name, args, stages, outcome) in [
        ("non_tail_yielding", vec![Bool(false)], vec![], Ok(8)),
        (
            "non_tail_yielding",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(42),
        ),
        (
            "non_tail_yielding_offset",
            vec![Bool(false), Int(9)],
            vec![],
            Ok(16),
        ),
        (
            "non_tail_yielding_offset",
            vec![Bool(true), Int(9)],
            vec![(vec![], vec![vec![9]])],
            Ok(50),
        ),
        ("non_tail_nested", vec![Bool(false)], vec![], Ok(16)),
        (
            "non_tail_nested",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(84),
        ),
        ("non_tail_negated", vec![Bool(false)], vec![], Ok(-7)),
        (
            "non_tail_negated",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(-41),
        ),
        ("non_tail_call_argument", vec![Bool(false)], vec![], Ok(8)),
        (
            "non_tail_call_argument",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(42),
        ),
        ("non_tail_let", vec![Bool(false)], vec![], Ok(9)),
        (
            "non_tail_let",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(43),
        ),
        (
            "non_tail_later_let",
            vec![Int(0), Bool(true)],
            vec![],
            Err(4),
        ),
        (
            "non_tail_later_let",
            vec![Int(1), Bool(false)],
            vec![],
            Ok(8),
        ),
        (
            "non_tail_later_let",
            vec![Int(2), Bool(true)],
            vec![(vec![], vec![vec![0]])],
            Ok(41),
        ),
        (
            "non_tail_second_call_argument",
            vec![Int(0), Bool(true)],
            vec![],
            Err(4),
        ),
        (
            "non_tail_second_call_argument",
            vec![Int(1), Bool(false)],
            vec![],
            Ok(8),
        ),
        (
            "non_tail_second_call_argument",
            vec![Int(2), Bool(true)],
            vec![(vec![], vec![vec![0]])],
            Ok(41),
        ),
        (
            "non_tail_local_capture",
            vec![Bool(false), Int(40), Int(2)],
            vec![],
            Ok(42),
        ),
        (
            "non_tail_local_capture",
            vec![Bool(true), Int(40), Int(2)],
            vec![(vec![41], vec![vec![2]])],
            Ok(44),
        ),
        (
            "non_tail_bool",
            vec![Bool(false)],
            vec![(vec![0], vec![vec![]])],
            Ok(1),
        ),
        (
            "non_tail_condition",
            vec![Bool(false)],
            vec![(vec![0], vec![vec![]])],
            Ok(71),
        ),
        (
            "non_tail_condition",
            vec![Bool(true)],
            vec![(vec![1], vec![vec![]])],
            Ok(70),
        ),
        ("non_tail_checked_argument", vec![Int(0)], vec![], Err(4)),
        (
            "non_tail_checked_argument",
            vec![Int(1)],
            vec![(vec![], vec![vec![]])],
            Ok(44),
        ),
        (
            "non_tail_eager_right",
            vec![Int(0), Bool(true)],
            vec![],
            Err(4),
        ),
        (
            "non_tail_eager_right",
            vec![Int(1), Bool(false)],
            vec![],
            Ok(8),
        ),
        (
            "non_tail_eager_right",
            vec![Int(1), Bool(true)],
            vec![(vec![], vec![vec![1]])],
            Ok(42),
        ),
        (
            "non_tail_eager_right",
            vec![Int(2), Bool(true)],
            vec![(vec![], vec![vec![0]])],
            Ok(41),
        ),
        (
            "non_tail_two_calls",
            vec![Bool(false), Bool(false)],
            vec![],
            Ok(14),
        ),
        (
            "non_tail_two_calls",
            vec![Bool(false), Bool(true)],
            vec![(vec![], vec![vec![7]])],
            Ok(48),
        ),
        (
            "non_tail_two_calls",
            vec![Bool(true), Bool(false)],
            vec![(vec![], vec![vec![0]])],
            Ok(48),
        ),
        (
            "non_tail_two_calls",
            vec![Bool(true), Bool(true)],
            vec![(vec![], vec![vec![1]]), (vec![], vec![vec![41]])],
            Ok(82),
        ),
        (
            "call_then_direct_yield",
            vec![Bool(false), Int(3)],
            vec![(vec![3, 7], vec![])],
            Ok(10),
        ),
        (
            "call_then_direct_yield",
            vec![Bool(true), Int(3)],
            vec![(vec![], vec![vec![3]]), (vec![3, 41], vec![])],
            Ok(44),
        ),
        (
            "direct_yield_then_call",
            vec![Int(3), Bool(false)],
            vec![(vec![3, 0], vec![])],
            Ok(10),
        ),
        (
            "direct_yield_then_call",
            vec![Int(3), Bool(true)],
            vec![(vec![3, 1], vec![]), (vec![], vec![vec![3]])],
            Ok(44),
        ),
        (
            "non_tail_branch",
            vec![Bool(false), Bool(true)],
            vec![],
            Ok(5),
        ),
        (
            "non_tail_branch",
            vec![Bool(true), Bool(false)],
            vec![],
            Ok(11),
        ),
        (
            "non_tail_branch",
            vec![Bool(true), Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(45),
        ),
        (
            "tail_composed_call",
            vec![Bool(true)],
            vec![(vec![], vec![vec![]])],
            Ok(42),
        ),
    ] {
        let ids = assert_call_stages(fixture, name, &args, &stages, outcome);
        if name == "non_tail_yielding" && !ids.is_empty() {
            assert_eq!(ids[0].0, callee, "callee owns the suspended continuation");
            completion = Some(ids[0].1[0]);
        }
        if name == "tail_composed_call" {
            assert_eq!(ids[0].0, callee);
            assert_eq!(
                Some(ids[0].1[0]),
                completion,
                "tail forwarding preserves its callee's completion"
            );
        }
    }
    assert_stages(
        fixture,
        "non_tail_eight_calls",
        &vec![Bool(false); 8],
        &[],
        Ok(56),
    );
    assert_stages(
        fixture,
        "non_tail_eight_calls",
        &vec![Bool(true); 8],
        &vec![vec![]; 8],
        Ok(328),
    );
    assert_boolean_resume_authority(fixture, "non_tail_bool", 1);
}

pub(super) fn assert_boolean_resume_authority(fixture: &mut Fixture, name: &str, count: usize) {
    let mut ids = Vec::new();
    assert_observed_stages(
        fixture,
        name,
        &[ReplValue::Bool(true)],
        &vec![vec![1]; count],
        Ok(0),
        |fixture, index, continuation| {
            assert!(fixture.resume(1, 2, continuation, vec![1]).is_err());
            if index > 0 {
                assert_ne!(ids[0], continuation);
                assert!(fixture.resume(1, 1, ids[0], vec![1]).is_err());
            }
            let descriptor = fixture
                .backend
                .continuation(continuation)
                .expect("Boolean callee descriptor");
            let error = super::super::super::validate_continuation_captures(descriptor, &[2])
                .expect_err("reject invalid Boolean capture");
            assert!(error.contains("pure_native_continuation_type"), "{error}");
            assert_eq!(fixture.runtime.pending_continuation_count(), 1);
            let callers = fixture.runtime.pending_completion_frames_for_test(1);
            assert_eq!(callers.len(), 1);
            assert!(callers[0].scalar_captures.is_empty());
            ids.push(continuation);
        },
    );
    assert!(fixture
        .resume(1, 1, *ids.last().expect("Boolean stages"), vec![1])
        .is_err());
    fixture
        .runtime
        .ensure_idle()
        .expect("Boolean caller completed once");
}
