use super::*;
use crate::runtime::vm::package_native_helper::{execute_call, VmPackageNativeHelpers};
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};
use std::sync::{Arc, Mutex};
use terlan_runtime_abi::{
    BoundaryError, ErrorDomain, FromNativeValue, NativeContextBinding, NativeServices, NativeValue,
};

struct Fixture {
    shard: PureNativeExecutionShard,
    state: Arc<Mutex<i64>>,
    directory: TestDirectory,
}

impl Fixture {
    fn new() -> Self {
        let directory = TestDirectory::new("native-context", "capability");
        let source = directory.join("context.terl");
        std::fs::write(
            &source,
            r#"module context.
import std.vm.{Process, Message}.
import type std.vm.Process.{Process}.
@compiler.native {fixture.context.bump}
invoke(value: Int): Int -> native.
pub handle(value: Int): Int -> invoke(value) + 1.
pub spawn_51(): Unit ->
    let message = Process.receive[Process[Int]]();
    let parent = Message.unwrap[Process[Int]](message);
    let value = invoke(30);
    Process.send[Int](parent, Message.wrap[Int](value));
    Unit.
pub spawned(): Int ->
    let child = Process.spawn[Process[Int]](Process.entry[Process[Int]](51));
    let parent = Process.current[Int]();
    Process.send[Process[Int]](child, Message.wrap[Process[Int]](parent));
    let response = Process.receive[Int]();
    Message.unwrap[Int](response).
"#,
        )
        .unwrap();
        let output = directory.join("build");
        assert_eq!(
            crate::commands::build::run(
                CliCommand {
                    verb: Some("build".into()),
                    args: vec![source.display().to_string()]
                },
                CliState {
                    out_dir: output.clone(),
                    ..CliState::default()
                },
            ),
            std::process::ExitCode::SUCCESS
        );
        let state = Arc::new(Mutex::new(0));
        let mut services = NativeServices::default();
        services
            .register_context(
                Arc::clone(&state),
                [NativeContextBinding::new(
                    "fixture.context.bump",
                    1,
                    |state, args| {
                        let value = i64::from_native(&args[0])?;
                        *state += 1;
                        if value < 0 {
                            return Err(BoundaryError::message(
                                ErrorDomain::NativeBoundary,
                                "test callback",
                                "failed after mutation",
                            ));
                        }
                        Ok(NativeValue::Int(value + *state))
                    },
                )],
            )
            .unwrap();
        let shard =
            crate::runtime::vm::pure_native::PureNativeExecutionImage::load_with_native_services(
                &output.join("vm/context.tvm"),
                services,
            )
            .unwrap()
            .spawn_shard()
            .unwrap();
        Self {
            directory,
            state,
            shard,
        }
    }

    fn begin(
        &mut self,
        value: i64,
    ) -> (VmProcessId, PureNativeSuspension, PureNativeCapabilityWait) {
        let (owner, mut execution) = self
            .shard
            .begin_call("handle", &[ReplValue::Int(value)])
            .unwrap();
        loop {
            let PureNativeExecution::Suspended(suspension) = execution else {
                panic!("native declaration must suspend")
            };
            if suspension.operation()
                == crate::runtime::native_image::control::TvmTransitionOperation::Capability
            {
                let wait = self
                    .shard
                    .begin_capability_call(owner, &suspension)
                    .unwrap();
                assert!(self.shard.has_native_service(&wait));
                return (owner, *suspension, wait);
            }
            execution = self.shard.resume_call(owner, *suspension).unwrap();
        }
    }

    fn count(&self) -> i64 {
        *self.state.lock().unwrap()
    }
}

#[test]
fn native_service_capability_executes_compiled_source_and_shares_only_granted_contexts() {
    let mut fixture = Fixture::new();
    let mut fork = fixture.shard.fork_empty().unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    assert_eq!(
        execute_call(
            &mut fixture.shard,
            &mut helpers,
            "handle",
            &[ReplValue::Int(10)]
        )
        .unwrap(),
        ReplValue::Int(12)
    );
    assert_eq!(
        execute_call(&mut fork, &mut helpers, "handle", &[ReplValue::Int(20)]).unwrap(),
        ReplValue::Int(23)
    );
    assert_eq!(fixture.count(), 2);
    assert_eq!(
        execute_call(&mut fixture.shard, &mut helpers, "spawned", &[]).unwrap(),
        ReplValue::Int(33)
    );
    assert_eq!(fixture.count(), 3);
    let mut ungranted =
        PureNativeExecutionShard::load_image(&fixture.directory.join("build/vm/context.tvm"))
            .unwrap();
    let (owner, execution) = ungranted
        .begin_call("handle", &[ReplValue::Int(1)])
        .unwrap();
    let PureNativeExecution::Suspended(suspension) = execution else {
        panic!("capability")
    };
    let wait = ungranted.begin_capability_call(owner, &suspension).unwrap();
    assert!(!ungranted.has_native_service(&wait));
    assert!(ungranted
        .resume_native_service_call(owner, *suspension, wait)
        .unwrap_err()
        .contains("native_service.unavailable"));
    assert_eq!(fixture.count(), 3);
}

#[test]
fn native_service_capability_rejects_stale_foreign_cancelled_and_replayed_waits_before_mutation() {
    let mut fixture = Fixture::new();
    let (owner, suspension, wait) = fixture.begin(1);
    let mut foreign = fixture.shard.fork_empty().unwrap();
    assert!(foreign
        .resume_native_service_call(owner, suspension, wait)
        .unwrap_err()
        .contains("capability_generation"));
    fixture.shard.cancel_call(owner, "foreign attempt").unwrap();
    let (owner, suspension, wait) = fixture.begin(1);
    let mut stale = wait.clone();
    stale.epoch = VmShardEpoch::new(wait.epoch.as_u64() + 1).unwrap();
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, stale)
        .unwrap_err()
        .contains("capability_generation"));
    fixture.shard.cancel_call(owner, "stale attempt").unwrap();
    let (owner, suspension, wait) = fixture.begin(1);
    let mut wrong = wait.clone();
    wrong.request_id += 1;
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wrong)
        .unwrap_err()
        .contains("capability_continuation"));
    assert_eq!(fixture.count(), 0);
    fixture.shard.cancel_call(owner, "wrong request").unwrap();
    let (owner, suspension, wait) = fixture.begin(1);
    fixture
        .shard
        .commit_epoch_operation(wait.completion)
        .unwrap();
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .is_err());
    assert_eq!(fixture.count(), 0);
    fixture
        .shard
        .cancel_call(owner, "already dispatched")
        .unwrap();
    let (owner, suspension, wait) = fixture.begin(1);
    let result = fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .unwrap();
    assert!(matches!(
        result,
        PureNativeExecution::Complete(ReplValue::Int(3))
    ));
    assert_eq!(fixture.count(), 1);
    fixture.shard.finish_completed_call(owner).unwrap();
    let (owner, suspension, wait) = fixture.begin(1);
    fixture
        .shard
        .cancel_call(owner, "test cancellation")
        .unwrap();
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .unwrap_err()
        .contains("continuation_stale"));
    assert_eq!(fixture.count(), 1);
}

#[test]
fn native_service_capability_errors_clean_up_and_failed_mutations_are_not_replayed() {
    let mut fixture = Fixture::new();
    for malformed in 0..5 {
        let (owner, suspension, mut wait) = fixture.begin(1);
        match malformed {
            0 => wait.request.package_arguments = None,
            1 => wait.request.package_arguments = Some(vec![]),
            2 => wait.request.package_arguments = Some(vec![ReplValue::Bool(true)]),
            3 => wait.request.operation = "fixture.context.missing".into(),
            _ => wait.request.package_arguments = Some(vec![ReplValue::Float("invalid".into())]),
        }
        assert!(fixture
            .shard
            .resume_native_service_call(owner, suspension, wait)
            .is_err());
        assert_eq!(fixture.count(), 0);
        assert_eq!(fixture.shard.execution.pending_continuation_count(), 0);
    }
    let (owner, suspension, wait) = fixture.begin(-1);
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .unwrap_err()
        .contains("failed after mutation"));
    assert_eq!(fixture.count(), 1);
    assert_eq!(fixture.shard.execution.pending_continuation_count(), 0);
    assert_eq!(fixture.count(), 1);
    let (owner, suspension, mut wait) = fixture.begin(1);
    wait.request.result_type = TvmBoundaryType::Bool;
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .is_err());
    assert_eq!(fixture.count(), 2);
    assert_eq!(fixture.shard.execution.pending_continuation_count(), 0);
    let (owner, suspension, wait) = fixture.begin(1);
    fixture
        .shard
        .actors
        .exit_actor(owner, VmExitReason::Killed)
        .unwrap();
    assert!(fixture
        .shard
        .resume_native_service_call(owner, suspension, wait)
        .unwrap_err()
        .contains("stale native continuation"));
    assert_eq!(
        fixture.count(),
        2,
        "actor exit must revoke authority before callback execution"
    );
    assert_eq!(fixture.shard.execution.pending_continuation_count(), 0);
}
