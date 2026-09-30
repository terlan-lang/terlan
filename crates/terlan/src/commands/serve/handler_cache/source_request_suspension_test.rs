//! Missing request optimization metadata must not imply synchronous execution.

use super::super::handler_cache_test_support::compile_native_handler_fixture;
use super::super::{AdmittedRequestProjection, AotHandlerRuntime, PrimaryRequestProjection};
use crate::runtime::native::http::RequestFieldProjection;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;

#[test]
fn unproven_handlers_suspend_and_synchronous_entry_rejects_external_waits() {
    let fixture = compile_native_handler_fixture(
        "source_request_suspension",
        "src/app/Waits.terl",
        "app_Waits",
        r#"module app.Waits.
import std.http.Response.
import std.io.File.
import std.vm.Process.
pub capability(): Response ->
    let exists = File.exists(".");
    if { exists -> Response.text("yes"); true -> Response.text("no") }.
pub receive(): String -> Process.receive_string().
pub timer(): Response -> Process.sleep(Process.timer(10)); Response.text("done").
pub ready(): Response -> Response.text("ready").
"#,
    );
    let mut runtime = AotHandlerRuntime::load("app.Waits".into(), &fixture.image, None)
        .expect("load handler without projection proof");
    assert!(runtime.direct_request_handler_may_suspend("app.Waits", "ready", 0));
    assert!(runtime.direct_request_handler_may_suspend("other", "ready", 0));
    runtime.primary_request_projection = Some(PrimaryRequestProjection {
        function: "ready".into(),
        arity: 0,
        projection: AdmittedRequestProjection {
            fields: RequestFieldProjection::Complete,
            scalar_entry: None,
            scalar_field: None,
            suspending: false,
        },
    });
    assert!(!runtime.direct_request_handler_may_suspend("app.Waits", "ready", 0));
    assert!(runtime.direct_request_handler_may_suspend("app.Waits", "ready", 1));
    assert!(runtime.direct_request_handler_may_suspend("app.Waits", "capability", 0));
    drop(runtime);

    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).expect("load shard");
    for function in ["capability", "receive", "timer"] {
        let export = format!("app.Waits.{function}");
        let owner = shard
            .spawn_fixed_owner_actor(&export, 0)
            .expect("spawn owner");
        let error = shard
            .call_on_admitted_fixed_owner(owner, &export, &[])
            .expect_err("external waits must not spin on the synchronous path");
        assert!(
            error.contains("execution_shard.external_wait"),
            "{function}: {error}"
        );
        assert!(
            shard.scheduler_class(owner).is_err(),
            "failed owner must be cleaned up"
        );
    }
    let owner = shard.spawn_fixed_owner_actor("app.Waits.ready", 0).unwrap();
    shard
        .call_on_admitted_fixed_owner(owner, "app.Waits.ready", &[])
        .expect("failed synchronous calls must not poison the shard");
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
