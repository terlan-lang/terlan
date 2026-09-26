//! Compiled Effect terminal states must reach the VM, not resume native success.

use std::process::ExitCode;

use crate::runtime::vm::{
    actor::VmActorRuntime,
    process::{VmExitReason, VmProcessSource, VmProcessState},
    ReplValue,
};
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

use super::{PureNativeBoundary, PureNativeExecutionContext, PureNativeExecutionRuntime};

#[test]
fn compiled_effect_failure_and_cancellation_terminate_the_owner() {
    assert_terminal_entries(
        r#"
module effect_terminal.
import std.core.Effect.
failed(): Effect[Int] -> Effect.fail("typed terminal value").
cancelled(): Effect[Int] -> Effect.cancelled().
numeric(): Effect[Int] -> Effect.fail(42).
must_not_run(value: Int): Int -> value div 0.
pub fail_text(): Int -> Effect.run(Effect.map(failed(), must_not_run)).
pub fail_number(): Int -> Effect.run(numeric()).
pub cancel(): Int -> Effect.run(Effect.map(cancelled(), must_not_run)).
"#,
        &[
            (
                "fail_text",
                VmExitReason::TypedError {
                    boundary_type: crate::runtime::native_image::TvmBoundaryType::String,
                    value: Box::new(ReplValue::String("typed terminal value".into())),
                },
            ),
            (
                "fail_number",
                VmExitReason::TypedError {
                    boundary_type: crate::runtime::native_image::TvmBoundaryType::Int,
                    value: Box::new(ReplValue::Int(42)),
                },
            ),
            ("cancel", VmExitReason::Killed),
        ],
    );
}

#[test]
fn compiled_effect_comprehension_terminal_guards_exit_the_owner() {
    let source =
        include_str!("../../../../../../tests/language/EffectfulComprehensionFailureTest.terl")
            .replace(
                "module tests.language.EffectfulComprehensionFailureTest.",
                "module effect_terminal.",
            );
    assert_terminal_entries(
        &source,
        &[
            (
                "propagates_typed_guard_failure",
                VmExitReason::TypedError {
                    boundary_type: crate::runtime::native_image::TvmBoundaryType::String,
                    value: Box::new(ReplValue::String("denied".into())),
                },
            ),
            ("propagates_guard_cancellation", VmExitReason::Killed),
        ],
    );
}

fn assert_terminal_entries(source_text: &str, entries: &[(&str, VmExitReason)]) {
    let directory = TestDirectory::new("effect-execution", "terminal");
    let source = directory.join("effect_terminal.terl");
    std::fs::write(&source, source_text).unwrap();
    let output = directory.join("build");
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".into()),
                args: vec![
                    source.display().to_string(),
                    "--target".into(),
                    "terlan-vm".into()
                ]
            },
            CliState {
                out_dir: output.clone(),
                ..CliState::default()
            },
        ),
        ExitCode::SUCCESS
    );
    let (mut boundary, managed) =
        PureNativeBoundary::load_image(&output.join("vm/effect_terminal.tvm")).unwrap();
    for (entry, expected) in entries {
        let mut actors = VmActorRuntime::default();
        let owner = actors.spawn_root(VmProcessSource::new("effect_terminal", *entry, 0));
        let mut execution = PureNativeExecutionRuntime::from_managed(managed.fork_empty());
        let mut context = PureNativeExecutionContext::new(owner, &mut execution);
        let error = boundary
            .call_for_actor(&mut actors, &mut context, entry, &[])
            .expect_err("terminal Effect must not resume its mapper or return a success");
        assert!(
            !error.contains("typed terminal value"),
            "diagnostic must not expose the payload"
        );
        context.release_owner();
        assert_eq!(
            actors.processes().get(owner).unwrap().state,
            VmProcessState::Exited(expected.clone())
        );
        assert_eq!(actors.pending_native_continuation_count(), 0);
    }
}
