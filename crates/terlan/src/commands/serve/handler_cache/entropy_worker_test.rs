//! Trusted authority must not turn OS entropy into owner-thread blocking work.

use crate::commands::serve::handler_cache::{
    invocation::AotHandlerInvocationStep, AotHandlerRuntime,
};
use crate::runtime::vm::{
    protocol_task_executor::{next_protocol_task_route, with_protocol_task_for_test},
    scheduler_topology::VmSchedulerId,
    ReplValue,
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

#[test]
fn entropy_uses_workers_on_both_serving_owners_even_when_trusted() {
    let Ok(mode) = std::env::var("TERLAN_TEST_ENTROPY_WORKER") else {
        let worker = super::capability_worker_path().unwrap();
        let missing = worker.with_file_name("missing-entropy-worker");
        assert!(!missing.exists());
        for (mode, worker) in [("live", &worker), ("missing", &missing)] {
            let name = concat!(
                module_path!(),
                "::entropy_uses_workers_on_both_serving_owners_even_when_trusted"
            );
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    name.strip_prefix("terlan::").unwrap(),
                    "--nocapture",
                ])
                .env("TERLAN_TEST_ENTROPY_WORKER", mode)
                .env("TERLAN_TEST_AOT_CAPABILITY_PUMP", "1")
                .env("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES", "1")
                .env("TERLAN_NATIVE_WORKER", worker);
            let output = terlan_process_owner::ProcessControl::new(Duration::from_secs(120))
                .capture_stdout_result(&mut command, 1_048_576, |_| Ok(()))
                .unwrap();
            assert!(
                output.outcome.is_ok(),
                "{mode}: {:?}\n{}",
                output.outcome,
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"));
        }
        return;
    };
    assert_eq!(
        std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").unwrap(),
        "1"
    );
    let fixture =
        crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture(
            &format!("entropy_worker_{mode}"),
            "src/app/Entropy.terl",
            "app_Entropy",
            "module app.Entropy. import std.crypto.Entropy. import std.vm.Bytes.
         pub check(): Int -> Entropy.bytes(32).length().",
        );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Entropy".into(), &fixture.image, None, 1)
            .unwrap();
    let route = next_protocol_task_route(VmSchedulerId::primary()).unwrap();
    let mut future = Box::pin(invoke(&runtime));
    let mut context = Context::from_waker(Waker::noop());
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        match with_protocol_task_for_test(route, || future.as_mut().poll(&mut context)) {
            Poll::Ready(result) => break result,
            Poll::Pending => {
                assert!(Instant::now() < deadline, "entropy worker did not resume");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    check_result(result, &mode);
    drop(future);
    // The ordinary shard owner must honor the same scheduling catalog.
    let result = runtime
        .begin_request_invocation("app.Entropy", "check", vec![])
        .map(|step| {
            let AotHandlerInvocationStep::Complete(value) = step else {
                panic!("automatic worker must settle the invocation");
            };
            value
        });
    check_result(result, &mode);
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

fn check_result(result: Result<ReplValue, String>, mode: &str) {
    if mode == "live" {
        assert_eq!(result.unwrap(), ReplValue::Int(32));
    } else {
        let error = result.unwrap_err();
        assert!(error.contains("missing runtime"), "{error}");
    }
}

async fn invoke(runtime: &AotHandlerRuntime) -> Result<ReplValue, String> {
    let mut step = runtime.begin_request_invocation("app.Entropy", "check", vec![])?;
    loop {
        step = match step {
            AotHandlerInvocationStep::Complete(value) => return Ok(value),
            AotHandlerInvocationStep::CapabilityWaiting(wait) => wait.resume_from_worker().await?,
            other => panic!("unexpected entropy step: {other:?}"),
        };
    }
}
