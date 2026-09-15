//! Multi-stage callees preserve one stable actor-owned caller completion.

use super::calls::{assert_boolean_resume_authority, assert_call_stages};
use super::{Fixture, ReplValue};

pub(super) fn assert_multi_stage_calls(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int};
    let direct = assert_call_stages(
        fixture,
        "yielded_add_twice",
        &[Int(1)],
        &[(vec![1], vec![]), (vec![2], vec![])],
        Ok(3),
    );
    assert_ne!(direct[0].0, direct[1].0);
    let mut offset_completion = None;
    for (name, args, stages, outcome) in [
        (
            "non_tail_multi_stage",
            vec![],
            vec![(vec![1], vec![vec![]]), (vec![2], vec![vec![]])],
            Ok(4),
        ),
        (
            "non_tail_multi_stage_offset",
            vec![Int(10), Int(5)],
            vec![(vec![10], vec![vec![5]]), (vec![11], vec![vec![5]])],
            Ok(17),
        ),
        (
            "non_tail_multi_stage_nested",
            vec![Int(2)],
            vec![(vec![2], vec![vec![]]), (vec![3], vec![vec![]])],
            Ok(10),
        ),
        (
            "non_tail_multi_stage_bool",
            vec![Bool(false)],
            vec![(vec![0], vec![vec![]]), (vec![0], vec![vec![]])],
            Ok(1),
        ),
        (
            "non_tail_multi_stage_condition",
            vec![Bool(false)],
            vec![(vec![0], vec![vec![]]), (vec![0], vec![vec![]])],
            Ok(73),
        ),
        (
            "non_tail_multi_stage_condition",
            vec![Bool(true)],
            vec![(vec![1], vec![vec![]]), (vec![1], vec![vec![]])],
            Ok(72),
        ),
        (
            "non_tail_multi_stage_branch",
            vec![Bool(false), Int(3)],
            vec![],
            Ok(5),
        ),
        (
            "non_tail_multi_stage_branch",
            vec![Bool(true), Int(3)],
            vec![(vec![3], vec![vec![]]), (vec![4], vec![vec![]])],
            Ok(9),
        ),
        ("non_tail_multi_stage_checked", vec![Int(0)], vec![], Err(4)),
        (
            "non_tail_multi_stage_checked",
            vec![Int(1)],
            vec![(vec![1], vec![vec![]]), (vec![2], vec![vec![]])],
            Ok(4),
        ),
        (
            "tail_multi_stage_composed",
            vec![Int(10), Int(5)],
            vec![(vec![10], vec![vec![5]]), (vec![11], vec![vec![5]])],
            Ok(17),
        ),
    ] {
        let ids = assert_call_stages(fixture, name, &args, &stages, outcome);
        if ids.is_empty() {
            continue;
        }
        assert_ne!(ids[0].0, ids[1].0, "{name}: distinct source yield sites");
        assert_eq!(
            ids[0].1, ids[1].1,
            "{name}: one stable caller frame across both stages"
        );
        if name == "non_tail_multi_stage" {
            assert_eq!([ids[0].0, ids[1].0], [direct[0].0, direct[1].0]);
        }
        if name == "non_tail_multi_stage_offset" {
            offset_completion = Some(ids[0].1[0]);
        }
        if name == "tail_multi_stage_composed" {
            assert_eq!(Some(ids[0].1[0]), offset_completion);
        }
    }
    let ids = assert_call_stages(
        fixture,
        "non_tail_eight",
        &[Int(9)],
        &vec![(vec![9], vec![vec![]]); 8],
        Ok(10),
    );
    let unique = ids
        .iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), 8, "eight distinct source suspension sites");
    assert!(
        ids.iter().all(|(_, frames)| frames == &ids[0].1),
        "one stable eight-stage caller frame"
    );
    assert_boolean_resume_authority(fixture, "non_tail_multi_stage_bool", 2);
}
