//! Real compiled-image continuation checks at the direct execution boundary.

use std::fs;
use std::process::ExitCode;

use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::PureNativeExecutionRuntime;
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

use super::*;

#[path = "direct_backend_compiled_branch_test.rs"]
mod branches;
#[path = "direct_backend_compiled_call_test.rs"]
mod calls;
#[path = "direct_backend_compiled_condition_expr_test.rs"]
mod condition_expr;
#[path = "direct_backend_compiled_condition_test.rs"]
mod conditions;
#[path = "direct_backend_compiled_multi_stage_test.rs"]
mod multi_stage;
#[path = "direct_backend_compiled_tail_call_test.rs"]
mod tail_calls;

/// Each case shares immutable admitted code and starts with empty actor state.
struct Fixture {
    backend: DirectNativeBackend,
    runtime: PureNativeExecutionRuntime,
    directory: TestDirectory,
}

impl Fixture {
    fn build() -> Self {
        let directory = TestDirectory::new("direct-boundary", "compiled");
        let source = directory.join("direct_boundary.terl");
        let mut program =
            include_str!("../../../../tests/fixtures/direct_boundary.terl").to_owned();
        for (module, fixture) in [
            (
                "direct_scalar_calls",
                include_str!("../../../../tests/fixtures/direct_scalar_calls.terl"),
            ),
            (
                "direct_non_tail_calls",
                include_str!("../../../../tests/fixtures/direct_non_tail_calls.terl"),
            ),
            (
                "direct_multi_stage_calls",
                include_str!("../../../../tests/fixtures/direct_multi_stage_calls.terl"),
            ),
            (
                "direct_condition_expr",
                include_str!("../../../../tests/fixtures/direct_condition_expr.terl"),
            ),
            (
                "direct_conditions",
                include_str!("../../../../tests/fixtures/direct_conditions.terl"),
            ),
            (
                "direct_tail_calls",
                include_str!("../../../../tests/fixtures/direct_tail_calls.terl"),
            ),
        ] {
            let header = format!("module {module}.\n\nimport std.vm.Process.\n");
            program.push_str(
                fixture
                    .strip_prefix(&header)
                    .expect("fixture module header"),
            );
        }
        fs::write(&source, program).expect("write compiled boundary source");
        let output = directory.join("build");
        assert_eq!(
            crate::commands::build::run(
                CliCommand {
                    verb: Some("build".into()),
                    args: vec![
                        source.display().to_string(),
                        "--target".into(),
                        "terlan-vm".into()
                    ],
                },
                CliState {
                    out_dir: output.clone(),
                    ..CliState::default()
                },
            ),
            ExitCode::SUCCESS,
        );
        let (backend, managed) = DirectNativeBackend::load(&output.join("vm/direct_boundary.tvm"))
            .expect("admit compiled boundary image");
        Self {
            backend,
            runtime: PureNativeExecutionRuntime::from_managed(managed),
            directory,
        }
    }

    fn reset(&mut self) {
        self.runtime = self.runtime.fork_empty();
    }

    fn call(&mut self, name: &str, args: &[ReplValue]) -> TvmControlFrame {
        let export = self
            .backend
            .image
            .exports
            .iter()
            .find(|entry| entry.name == format!("direct_boundary.{name}/{}", args.len()))
            .expect("fixture export")
            .id;
        let request = self
            .runtime
            .allocate_request_id()
            .expect("request identity");
        self.backend
            .call_frame(
                &mut PureNativeExecutionContext::new(
                    VmProcessId::from_raw_for_test(1),
                    &mut self.runtime,
                ),
                request,
                export,
                args,
            )
            .expect("direct call")
    }

    fn resume(
        &mut self,
        owner: u64,
        request: u64,
        continuation: u64,
        values: Vec<i64>,
    ) -> Result<TvmControlFrame, String> {
        self.backend.resume_frame(
            &mut PureNativeExecutionContext::new(
                VmProcessId::from_raw_for_test(owner),
                &mut self.runtime,
            ),
            request,
            continuation,
            values,
        )
    }
}

fn transition(
    frame: TvmControlFrame,
    operation: TvmTransitionOperation,
    arguments: &[i64],
    captures: &[i64],
) -> (u64, u64) {
    let TvmControlFrame::Transition {
        request_id,
        owner_id,
        continuation_id,
        operation: actual,
        arguments: actual_arguments,
        values,
    } = frame
    else {
        panic!("expected transition, found {frame:?}");
    };
    assert_eq!(owner_id, 1);
    assert_ne!(request_id, 0);
    assert_ne!(continuation_id, 0);
    assert_eq!(actual, operation);
    assert_eq!(actual_arguments, arguments);
    assert_eq!(values, captures);
    (request_id, continuation_id)
}

fn success(frame: TvmControlFrame, request: u64, expected: i64) {
    assert_eq!(
        frame,
        TvmControlFrame::Success {
            request_id: request,
            owner_id: 1,
            value: expected
        }
    );
}

/// Replaces worker-wire assertions with calls to the actual loaded ABI-3 backend.
#[test]
fn compiled_boundary_preserves_transition_payloads_and_resume_authority() {
    use TvmTransitionOperation as Op;
    let mut fixture = Fixture::build();
    for (name, inputs, operation, arguments, injected) in [
        ("send_capture", vec![41], Op::Send, vec![1, 41], None),
        ("receive_capture", vec![41], Op::Receive, vec![], Some(1)),
        ("spawn_capture", vec![4, 41], Op::Spawn, vec![4], Some(2)),
        ("timer_capture", vec![41], Op::Timer, vec![3], None),
        ("link_capture", vec![2, 41], Op::Link, vec![2], None),
        (
            "monitor_capture",
            vec![2, 41],
            Op::Monitor,
            vec![2],
            Some(3),
        ),
        (
            "resource_capture",
            vec![7, 41],
            Op::Resource,
            vec![7],
            Some(4),
        ),
        (
            "cancellation_capture",
            vec![2, 41],
            Op::Cancellation,
            vec![2],
            None,
        ),
        ("failure_capture", vec![7, 41], Op::Failure, vec![7], None),
        (
            "scheduling_capture",
            vec![1, 41],
            Op::Scheduling,
            vec![1],
            None,
        ),
    ] {
        let args = inputs.into_iter().map(ReplValue::Int).collect::<Vec<_>>();
        let composed = format!("composed_{}", name.strip_suffix("_capture").unwrap());
        let delegated = format!("delegated_{}", name.strip_suffix("_capture").unwrap());
        for (entry, expected) in [
            (name, 42),
            (composed.as_str(), 43),
            (delegated.as_str(), 42),
        ] {
            fixture.reset();
            let frame = fixture.call(entry, &args);
            // A delegated intrinsic has no local capture: the caller's 41
            // lives in the VM-owned completion stack, not the parked callee.
            let captures: &[i64] = if entry == delegated { &[] } else { &[41] };
            let (request, continuation) =
                transition(frame, operation.clone(), &arguments, captures);
            let mut values = injected.into_iter().collect::<Vec<_>>();
            values.extend_from_slice(captures);
            success(
                fixture
                    .resume(1, request, continuation, values)
                    .expect(entry),
                request,
                expected,
            );
            fixture
                .runtime
                .ensure_idle()
                .expect("completed continuation released");
        }
    }
    fixture.reset();
    let frame = fixture.call("yield_capture", &[ReplValue::Int(41)]);
    let (request, continuation) = transition(frame, Op::Yield, &[], &[41]);
    for (owner, request, continuation) in [
        (2, request, continuation),
        (1, request + 1, continuation),
        (1, request, continuation ^ 1),
    ] {
        assert!(fixture
            .resume(owner, request, continuation, vec![41])
            .is_err());
        assert_eq!(fixture.runtime.pending_continuation_count(), 1);
    }
    assert!(fixture.runtime.ensure_idle().is_err());
    success(
        fixture
            .resume(1, request, continuation, vec![41])
            .expect("rightful resume"),
        request,
        42,
    );
    assert!(fixture.resume(1, request, continuation, vec![41]).is_err());
    fixture.runtime.ensure_idle().expect("completed owner");
    assert_capture_shapes(&mut fixture);
    branches::assert_branches(&mut fixture);
    conditions::assert_conditions(&mut fixture);
    condition_expr::assert_condition_expressions(&mut fixture);
    tail_calls::assert_tail_calls(&mut fixture);
    calls::assert_non_tail_calls(&mut fixture);
    multi_stage::assert_multi_stage_calls(&mut fixture);
    // Unload the library before removing its owned source/build directory.
    let Fixture {
        backend,
        runtime,
        directory,
    } = fixture;
    drop(backend);
    drop(runtime);
    directory.close();
}

fn assert_capture_shapes(fixture: &mut Fixture) {
    use TvmTransitionOperation::Yield;
    for (name, args, captures, expected) in [
        ("local_capture", vec![ReplValue::Int(21)], vec![42], 43),
        (
            "pair_capture",
            vec![ReplValue::Int(20), ReplValue::Int(22)],
            vec![20, 22],
            42,
        ),
        ("bool_capture", vec![ReplValue::Bool(true)], vec![1], 1),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &args);
        let (request, continuation) = transition(frame, Yield, &[], &captures);
        success(
            fixture
                .resume(1, request, continuation, captures)
                .expect(name),
            request,
            expected,
        );
        fixture.runtime.ensure_idle().expect("capture released");
    }
    for (name, distinct_points) in [
        ("repeated_capture", false),
        ("repeated_direct_capture", true),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &[ReplValue::Int(41)]);
        let (request, first) = transition(frame, Yield, &[], &[41]);
        let frame = fixture
            .resume(1, request, first, vec![41])
            .expect("first resume");
        let (same_request, second) = transition(frame, Yield, &[], &[42]);
        assert_eq!(same_request, request);
        // Re-entering a helper reuses its code identity; separate source yield
        // sites have distinct identities. Resume authority remains actor-owned.
        assert_eq!(first != second, distinct_points);
        success(
            fixture
                .resume(1, request, second, vec![42])
                .expect("second resume"),
            request,
            43,
        );
        fixture
            .runtime
            .ensure_idle()
            .expect("repeated capture released");
    }
    for (name, argument, captures, invalid) in [
        ("bool_capture", ReplValue::Bool(true), vec![1], vec![2]),
        ("yield_capture", ReplValue::Int(41), vec![41], vec![]),
    ] {
        fixture.reset();
        let frame = fixture.call(name, &[argument]);
        let (request, continuation) = transition(frame, Yield, &[], &captures);
        // The production reply driver validates scalar capture words before
        // parking/resuming them. The private backend accepts already checked
        // words; it is not the removed public worker-wire input boundary.
        let descriptor = fixture
            .backend
            .continuation(continuation)
            .expect("admitted continuation");
        let error = super::super::validate_continuation_captures(descriptor, &invalid)
            .expect_err("production capture admission rejects malformed words");
        assert!(error.contains("pure_native_continuation_type"), "{error}");
        assert_eq!(fixture.runtime.pending_continuation_count(), 1);
        let expected = captures[0];
        success(
            fixture
                .resume(1, request, continuation, captures)
                .expect("valid capture resume"),
            request,
            if name == "yield_capture" {
                expected + 1
            } else {
                expected
            },
        );
        fixture
            .runtime
            .ensure_idle()
            .expect("validated capture released");
    }
}
