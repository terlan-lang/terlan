//! Package resources use the same actor cleanup in both serving schedulers.

use super::super::{invocation::AotHandlerInvocationStep, AotHandlerRuntime};
use crate::runtime::vm::{
    protocol_task_executor::{next_protocol_task_route, with_protocol_task_for_test},
    scheduler_topology::VmSchedulerTopology,
    ReplValue,
};
use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

struct OwnerWake(std::thread::Thread);
impl Wake for OwnerWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

#[test]
fn source_resource_calls_resume_and_retire_both_serving_owners() {
    // Isolate the automatic-pump flag from tests that inspect parked continuations.
    if std::env::var("TERLAN_TEST_AOT_CAPABILITY_PUMP").as_deref() != Ok("1") {
        let name = concat!(
            module_path!(),
            "::source_resource_calls_resume_and_retire_both_serving_owners"
        );
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                name.strip_prefix("terlan::").unwrap(),
                "--nocapture",
            ])
            .env("TERLAN_TEST_AOT_CAPABILITY_PUMP", "1")
            .env_remove("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES");
        let output = terlan_process_owner::ProcessControl::new(Duration::from_secs(120))
            .capture_stdout_result(&mut command, 1_048_576, |_| Ok(()))
            .expect("resource lifecycle test must complete within its process deadline");
        assert!(
            output.outcome.is_ok(),
            "{:?}\n{}",
            output.outcome,
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        return;
    }
    assert_ne!(
        std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref(),
        Ok("1")
    );
    let root = crate::support::test_fs::temp_path("serve", "resource_server");
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"resource-server\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    let source = root.join("src/app/Resource.terl");
    std::fs::write(
        &source,
        r#"
module app.Resource.
import std.data.Json.
pub check(): String ->
    let values = Json.array();
    let alias = values;
    values.push(Json.int(1));
    values.push(values);
    Json.to_string(alias).
"#,
    )
    .unwrap();
    let fixture = crate::commands::build::vm_artifact::compile_serve_application(
        &web,
        &source,
        "app.Resource",
    )
    .unwrap();
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.Resource".into(),
        &fixture.image.path,
        None,
        1,
    )
    .unwrap();
    let scheduler = VmSchedulerTopology::new(1)
        .unwrap()
        .schedulers()
        .next()
        .unwrap();
    let route = next_protocol_task_route(scheduler).unwrap();
    let cancelled_route = next_protocol_task_route(scheduler).unwrap();
    let waker = Waker::from(Arc::new(OwnerWake(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut cancelled = Box::pin(invoke(&runtime));
    assert!(
        with_protocol_task_for_test(cancelled_route, || cancelled.as_mut().poll(&mut cx))
            .is_pending()
    );
    with_protocol_task_for_test(cancelled_route, || drop(cancelled));
    let mut future = Box::pin(invoke(&runtime));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match with_protocol_task_for_test(route, || future.as_mut().poll(&mut cx)) {
            Poll::Ready(value) => {
                assert_eq!(value.unwrap(), ReplValue::String("[1,[1]]".into()));
                break;
            }
            Poll::Pending => {
                assert!(Instant::now() < deadline, "resource call did not resume");
                std::thread::park_timeout(Duration::from_millis(10));
            }
        }
    }
    drop(future);
    with_protocol_task_for_test(route, || {
        crate::runtime::vm::protocol_task_executor::with_existing_current_protocol_resource::<
            super::ProtocolCapabilityDispatcher,
            _,
        >(runtime.generation.identity, |dispatcher| {
            assert!(dispatcher.resource_owners.is_empty());
            assert!(dispatcher.native_pending.is_empty());
            Ok(())
        })
        .unwrap();
    });
    for _ in 0..20 {
        let step = runtime
            .begin_request_invocation("app.Resource", "check", vec![])
            .unwrap();
        assert!(
            matches!(step, AotHandlerInvocationStep::Complete(ReplValue::String(value)) if value == "[1,[1]]")
        );
    }
    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
}

async fn invoke(runtime: &AotHandlerRuntime) -> Result<ReplValue, String> {
    let mut step = runtime.begin_request_invocation("app.Resource", "check", vec![])?;
    loop {
        step = match step {
            AotHandlerInvocationStep::Complete(value) => return Ok(value),
            AotHandlerInvocationStep::CapabilityWaiting(wait) => wait.resume_from_worker().await?,
            other => panic!("unexpected resource step: {other:?}"),
        };
    }
}
