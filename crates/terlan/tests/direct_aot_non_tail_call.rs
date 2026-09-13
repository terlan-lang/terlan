#[path = "support/aot_program.rs"]
mod program;

#[test]
fn native_aot_wraps_terminal_non_tail_calls_in_caller_continuations() {
    // The private compiled backend checks exact callee/caller capture shapes,
    // continuation identities, suspension counts, and invalid resume authority.
    program::assert_program(
        "direct_non_tail_calls",
        include_str!("fixtures/direct_non_tail_calls.terl"),
        &[(
            "direct_scalar_calls",
            include_str!("fixtures/direct_scalar_calls.terl"),
        )],
        &[
            "branch_yield(true) == 41",
            "non_tail_yielding(false) == 8",
            "non_tail_yielding(true) == 42",
            "non_tail_yielding_offset(false, 9) == 16",
            "non_tail_yielding_offset(true, 9) == 50",
            "non_tail_nested(false) == 16",
            "non_tail_nested(true) == 84",
            "non_tail_negated(false) == -7",
            "non_tail_negated(true) == -41",
            "non_tail_call_argument(false) == 8",
            "non_tail_call_argument(true) == 42",
            "non_tail_let(false) == 9",
            "non_tail_let(true) == 43",
            "non_tail_later_let(1, false) == 8",
            "non_tail_later_let(2, true) == 41",
            "non_tail_second_call_argument(1, false) == 8",
            "non_tail_second_call_argument(2, true) == 41",
            "non_tail_local_capture(false, 40, 2) == 42",
            "non_tail_local_capture(true, 40, 2) == 44",
            "non_tail_bool(false)",
            "not non_tail_bool(true)",
            "non_tail_condition(false) == 71",
            "non_tail_condition(true) == 70",
            "non_tail_checked_argument(1) == 44",
            "non_tail_eager_right(1, false) == 8",
            "non_tail_eager_right(1, true) == 42",
            "non_tail_eager_right(2, true) == 41",
            "non_tail_two_calls(false, false) == 14",
            "non_tail_two_calls(false, true) == 48",
            "non_tail_two_calls(true, false) == 48",
            "non_tail_two_calls(true, true) == 82",
            "non_tail_eight_calls(false, false, false, false, false, false, false, false) == 56",
            "non_tail_eight_calls(true, true, true, true, true, true, true, true) == 328",
            "call_then_direct_yield(false, 3) == 10",
            "call_then_direct_yield(true, 3) == 44",
            "direct_yield_then_call(3, false) == 10",
            "direct_yield_then_call(3, true) == 44",
            "non_tail_branch(false, true) == 5",
            "non_tail_branch(true, false) == 11",
            "non_tail_branch(true, true) == 45",
            "tail_composed_call(true) == 42",
        ],
        &[
            ("non_tail_later_let(0, true)", "error[division_by_zero]"),
            (
                "non_tail_second_call_argument(0, true)",
                "error[division_by_zero]",
            ),
            ("non_tail_checked_argument(0)", "error[division_by_zero]"),
            ("non_tail_eager_right(0, true)", "error[division_by_zero]"),
        ],
    );
}
