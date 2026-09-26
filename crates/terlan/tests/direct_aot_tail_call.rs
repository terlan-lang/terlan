#[path = "support/aot_program.rs"]
mod program;

#[test]
fn native_aot_forwards_suspending_tail_calls_without_a_caller_stack() {
    // The private compiled boundary also checks callee continuation identity,
    // capture layouts, stale requests, malformed Booleans, and duplicate resumes.
    program::assert_program(
        "direct_tail_calls",
        include_str!("fixtures/direct_tail_calls.terl"),
        &[(
            "direct_scalar_calls",
            include_str!("fixtures/direct_scalar_calls.terl"),
        )],
        &[
            "branch_yield(true) == 41",
            "call_yielding(false) == 7",
            "call_yielding(true) == 41",
            "tail_yielding_local(true, 40) == 42",
            "tail_yielding_local(false, 40) == 40",
            "tail_yielding_branch(true, true) == 41",
            "tail_yielding_branch(false, false) == 42",
            "tail_yielding_chain(true) == 41",
            "tail_yielding_bool(true)",
            "tail_yielding_prefix(true, 40) == 42",
            "tail_yielding_prefix(false, 40) == 40",
            "tail_yielding_checked(true, 1) == 2",
            "tail_yielding_checked(false, 0) == 7",
        ],
        &[("tail_yielding_checked(true, 0)", "error[division_by_zero]")],
    );
}
