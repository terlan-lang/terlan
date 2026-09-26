#[path = "support/aot_program.rs"]
mod program;

#[test]
fn native_aot_composes_non_linear_scalar_conditions_in_evaluation_order() {
    // The same source is admitted by the private compiled boundary, which
    // asserts ordered captures, exact suspension counts, and resume authority.
    program::assert_program(
        "direct_condition_expr",
        include_str!("fixtures/direct_condition_expr.terl"),
        &[(
            "direct_scalar_calls",
            include_str!("fixtures/direct_scalar_calls.terl"),
        )],
        &[
            "yield_comparison_condition(11) == 51",
            "yield_comparison_condition(10) == 52",
            "yield_not_condition(false) == 53",
            "yield_not_condition(true) == 54",
            "yield_and_rhs(false, true) == 56",
            "yield_and_rhs(true, true) == 55",
            "yield_and_rhs(true, false) == 56",
            "yield_or_rhs(true, false) == 57",
            "yield_or_rhs(false, true) == 57",
            "yield_or_rhs(false, false) == 58",
            "yield_and_rhs_then_body(true, true) == 60",
            "yield_or_rhs_then_body(true, false) == 74",
            "yield_or_rhs_then_body(false, false) == 75",
            "yield_or_rhs_then_body(false, true) == 74",
            "yield_nested_and_rhs(false, true, true) == 63",
            "yield_nested_and_rhs(true, false, true) == 63",
            "yield_nested_and_rhs(true, true, true) == 62",
            "yield_nested_and_rhs(true, true, false) == 63",
            "checked_before_yield_rhs(1, true) == 64",
            "yield_eager_right_condition(1, 0) == 68",
            "yield_eager_right_condition(2, 3) == 69",
            "yield_eager_value(2, 5) == 5",
            "yield_unary_value(3) == -3",
            "yield_second_call_argument(2, 5) == 5",
            "yield_only() == Unit",
            "unit_identity(Unit) == Unit",
            "two_unit_effects(43) == 43",
            "eight_unit_effects(44) == 44",
            "yield_unit_then_int(41) == 42",
            "tail_yield_only() == Unit",
            "yield_unit_capture(Unit) == Unit",
        ],
        &[
            ("yield_and_rhs_only(false, true)", "error[if_clause]"),
            ("yield_and_rhs_only(true, false)", "error[if_clause]"),
            (
                "checked_before_yield_rhs(0, true)",
                "error[division_by_zero]",
            ),
            (
                "yield_eager_right_condition(0, 1)",
                "error[division_by_zero]",
            ),
            ("yield_eager_value(0, 5)", "error[division_by_zero]"),
            (
                "yield_second_call_argument(0, 5)",
                "error[division_by_zero]",
            ),
            ("yield_first_call_argument(5, 0)", "error[division_by_zero]"),
        ],
    );
}
