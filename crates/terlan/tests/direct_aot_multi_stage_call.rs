#[path = "support/aot_program.rs"]
mod program;

#[test]
fn native_aot_wraps_bounded_linear_multi_stage_callees() {
    // The private compiled backend checks exact callee/caller capture shapes,
    // continuation identities, suspension counts, and invalid resume authority.
    program::assert_program(
        "direct_multi_stage_calls",
        include_str!("fixtures/direct_multi_stage_calls.terl"),
        &[],
        &[
            "yielded_add_twice(1) == 3",
            "non_tail_multi_stage() == 4",
            "non_tail_multi_stage_offset(10, 5) == 17",
            "non_tail_multi_stage_nested(2) == 10",
            "non_tail_multi_stage_bool(false)",
            "not non_tail_multi_stage_bool(true)",
            "non_tail_multi_stage_condition(false) == 73",
            "non_tail_multi_stage_condition(true) == 72",
            "non_tail_multi_stage_branch(false, 3) == 5",
            "non_tail_multi_stage_branch(true, 3) == 9",
            "non_tail_multi_stage_checked(1) == 4",
            "tail_multi_stage_composed(10, 5) == 17",
            "non_tail_eight(9) == 10",
        ],
        &[("non_tail_multi_stage_checked(0)", "error[division_by_zero]")],
    );
}
