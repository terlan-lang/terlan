//! Tail forwarding uses the callee continuation, not a caller-side wrapper.

use super::{success, transition, Fixture, ReplValue, TvmTransitionOperation};

pub(super) fn assert_tail_calls(fixture: &mut Fixture) {
    use ReplValue::{Bool, Int};
    use TvmTransitionOperation::Yield;
    fixture.reset();
    let frame = fixture.call("branch_yield", &[Bool(true)]);
    let (request, callee) = transition(frame, Yield, &[], &[]);
    success(
        fixture
            .resume(1, request, callee, vec![])
            .expect("direct callee"),
        request,
        41,
    );
    fixture.runtime.ensure_idle().expect("callee completed");
    for (name, args, captures, expected, forwards_callee) in [
        (
            "tail_yielding_prefix",
            vec![Bool(true), Int(40)],
            vec![41],
            42,
            false,
        ),
        (
            "tail_yielding_checked",
            vec![Bool(true), Int(1)],
            vec![1],
            2,
            false,
        ),
        ("call_yielding", vec![Bool(true)], vec![], 41, true),
        (
            "tail_yielding_local",
            vec![Bool(true), Int(40)],
            vec![41],
            42,
            false,
        ),
        (
            "tail_yielding_branch",
            vec![Bool(true), Bool(true)],
            vec![],
            41,
            true,
        ),
        (
            "tail_yielding_branch",
            vec![Bool(false), Bool(false)],
            vec![41],
            42,
            false,
        ),
        ("tail_yielding_chain", vec![Bool(true)], vec![], 41, true),
        ("tail_yielding_bool", vec![Bool(true)], vec![1], 1, false),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &args);
        let (request, continuation) = transition(frame, Yield, &[], &captures);
        assert!(
            fixture
                .runtime
                .pending_completion_frames_for_test(1)
                .is_empty(),
            "{name}: a tail call must not retain a caller completion frame"
        );
        if forwards_callee {
            assert_eq!(continuation, callee, "{name}: forwarded code identity");
        }
        if name == "tail_yielding_bool" {
            let descriptor = fixture
                .backend
                .continuation(continuation)
                .expect("Boolean descriptor");
            let error = super::super::super::validate_continuation_captures(descriptor, &[2])
                .expect_err("tail call retains Boolean capture validation");
            assert!(error.contains("pure_native_continuation_type"), "{error}");
        }
        if name == "tail_yielding_local" {
            assert!(fixture
                .resume(1, request + 1, continuation, captures.clone())
                .is_err());
        }
        assert_eq!(fixture.runtime.pending_continuation_count(), 1);
        success(
            fixture
                .resume(1, request, continuation, captures.clone())
                .expect(name),
            request,
            expected,
        );
        if name == "call_yielding" {
            assert!(fixture.resume(1, request, continuation, captures).is_err());
        }
        fixture
            .runtime
            .ensure_idle()
            .expect("tail continuation completed");
    }
    for (name, args, expected) in [
        ("tail_yielding_prefix", vec![Bool(false), Int(40)], 40),
        ("tail_yielding_checked", vec![Bool(false), Int(0)], 7),
        ("call_yielding", vec![Bool(false)], 7),
        ("tail_yielding_local", vec![Bool(false), Int(40)], 40),
    ] {
        fixture.reset();
        success(fixture.call(name, &args), 1, expected);
        fixture
            .runtime
            .ensure_idle()
            .expect("pure tail branch must not suspend");
    }
    super::condition_expr::assert_stages(
        fixture,
        "tail_yielding_checked",
        &[Bool(true), Int(0)],
        &[],
        Err(4),
    );
}
