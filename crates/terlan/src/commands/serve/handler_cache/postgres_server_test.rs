//! Live source database calls must complete through both server actor owners.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant};

use crate::runtime::vm::protocol_task_executor::{
    next_protocol_task_route, with_protocol_task_for_test,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;
use crate::runtime::vm::ReplValue;

use super::super::invocation::AotHandlerInvocationStep;
use super::super::AotHandlerRuntime;

struct OwnerWake(std::thread::Thread);
impl Wake for OwnerWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

#[test]
#[ignore = "requires the explicit live Postgres server gate and packaged worker"]
fn source_postgres_resumes_protocol_and_generated_owners() {
    assert_eq!(
        std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref(),
        Ok("1")
    );
    assert!(std::env::var_os("TERLAN_TEST_AOT_CAPABILITY_PUMP").is_some());
    let database = crate::runtime::vm::postgres::docker_fixture_test::DockerPostgres::start()
        .expect("live database fixture");
    let url = database.url("terlan");
    let root = crate::support::test_fs::temp_path("serve", "postgres_server");
    let web = root.join("_build/web");
    std::fs::create_dir_all(root.join("src/app")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(
        root.join("terlan.toml"),
        "[package]\nname = \"postgres-server\"\nversion = \"0.0.9\"\nnamespace = \"app\"\n",
    )
    .unwrap();
    let source_path = root.join("src/app/Database.terl");
    std::fs::write(
        &source_path,
        r#"
module app.Database.
import std.db.Postgres.{connect, query_one, transaction}.
import std.core.Result.{Ok, Err}.
import std.core.Option.{Some}.
import std.core.Error.{new}.
import std.data.Json.
import type std.db.Postgres.{Connection}.
pub type Failed = Atom["fixture.failed"].
pub check(url: String): Bool ->
    let captured = "server callback";
    case connect({url: url}) {
        Ok(pool) -> transaction[String](pool, (connection) ->
            case query_one(connection, "SELECT $1::bigint AS number, '7'::jsonb AS document", [Json.int(7)]) {
                Ok(Some(row)) -> case row.int("number") {
                    Ok(7) -> case row.json("document") {
                        Ok(document) -> case Json.as_int(document) {
                            Ok(7) -> Ok(captured);
                            _ -> Err(new(Failed, "unexpected JSON value"))
                        };
                        _ -> Err(new(Failed, "missing JSON value"))
                    };
                    _ -> Err(new(Failed, "unexpected row"))
                };
                _ -> Err(new(Failed, "missing row"))
            }) == Ok(captured);
        Err(_) -> false
    }.
"#,
    )
    .unwrap();
    let fixture = crate::commands::build::vm_artifact::compile_serve_application(
        &web,
        &source_path,
        "app.Database",
    )
    .expect("compile complete server application");
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.Database".into(),
        &fixture.image.path,
        None,
        1,
    )
    .expect("load source database handler");
    let scheduler = VmSchedulerTopology::new(1)
        .unwrap()
        .schedulers()
        .next()
        .unwrap();
    let protocol = next_protocol_task_route(scheduler).unwrap();
    let mut future = Box::pin(invoke(&runtime, &url));
    let waker = Waker::from(Arc::new(OwnerWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let cancelled_route = next_protocol_task_route(scheduler).unwrap();
    let mut cancelled = Box::pin(invoke(&runtime, &url));
    assert!(
        with_protocol_task_for_test(cancelled_route, || cancelled.as_mut().poll(&mut context))
            .is_pending()
    );
    assert!(
        with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context)).is_pending()
    );
    with_protocol_task_for_test(cancelled_route, || drop(cancelled));
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match with_protocol_task_for_test(protocol, || future.as_mut().poll(&mut context)) {
            Poll::Ready(value) => {
                assert_eq!(value.unwrap(), ReplValue::Bool(true));
                break;
            }
            Poll::Pending => {
                assert!(
                    Instant::now() < deadline,
                    "database callback did not resume"
                );
                std::thread::park_timeout(Duration::from_millis(50));
            }
        }
    }
    drop(future);
    with_protocol_task_for_test(protocol, || {
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
    let completed = runtime
        .begin_request_invocation("app.Database", "check", vec![ReplValue::String(url)])
        .unwrap();
    assert!(matches!(
        completed,
        AotHandlerInvocationStep::Complete(ReplValue::Bool(true))
    ));
    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
}

async fn invoke(runtime: &AotHandlerRuntime, url: &str) -> Result<ReplValue, String> {
    let mut step = runtime.begin_request_invocation(
        "app.Database",
        "check",
        vec![ReplValue::String(url.to_string())],
    )?;
    loop {
        step = match step {
            AotHandlerInvocationStep::Complete(value) => return Ok(value),
            AotHandlerInvocationStep::CapabilityWaiting(wait) => wait.resume_from_worker().await?,
            other => panic!("unexpected database step: {other:?}"),
        };
    }
}
