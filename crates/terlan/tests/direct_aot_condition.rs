#[path = "support/aot_program.rs"]
mod program;

#[test]
fn native_aot_composes_suspending_conditions_with_enclosing_control_flow() {
    // Capture shape, resume authority, and exact suspension ordering are checked
    // by the private compiled backend against this same focused source fixture.
    program::assert_program(
        "direct_conditions",
        include_str!("fixtures/direct_conditions.terl"),
        &[],
        &[
            "yield_condition(true) == 31",
            "yield_condition(false) == 32",
            "yield_later_condition(true, false) == 33",
            "yield_later_condition(false, true) == 34",
            "yield_later_condition(false, false) == 35",
            "yield_condition_capture(true, 20, 22) == 42",
            "yield_condition_capture(false, 20, 22) == -2",
            "yield_condition_local(10) == 10",
            "yield_condition_local(5) == 0",
            "yield_condition_prefix_error(1) == 1",
            "yield_condition_twice(true) == 36",
            "yield_condition_twice(false) == 37",
            "yield_condition_then_body(true) == 38",
            "nested_yield_condition(false, true) == 42",
            "nested_yield_condition(true, true) == 40",
            "nested_yield_condition(true, false) == 41",
            "not yield_short_circuit_left(false, true)",
            "yield_short_circuit_left(true, true)",
            "yield_short_circuit_or_left(false, true)",
            "yield_short_circuit_or_left(true, false)",
            "not yield_short_circuit_left_guard(false)",
            "yield_short_circuit_or_guard(true)",
        ],
        &[
            ("yield_condition_prefix_error(0)", "error[division_by_zero]"),
            (
                "yield_short_circuit_left_guard(true)",
                "error[division_by_zero]",
            ),
            (
                "yield_short_circuit_or_guard(false)",
                "error[division_by_zero]",
            ),
            ("yield_condition_only(false)", "error[if_clause]"),
        ],
    );
}
