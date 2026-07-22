//! Full-cycle evidence for request-owned native handler continuations.

use std::fs;

use crate::commands::serve::handler::HandlerResponse;
use crate::runtime::native_image::TvmBoundaryType;
use crate::support::test_fs;
use crate::{ColorChoice, DiagnosticFormat};

use super::*;

const SOURCE: &str = r#"module app.AsyncHandler.

import std.http.Response.
import std.vm.Process.
import type std.http.{Request, Response}.

pub delayed(_request: Request): Response ->
    Response.text(Process.receive_string()).
"#;

/// Compiles one real Terlan HTTP handler into an admitted native runtime.
fn runtime() -> (std::path::PathBuf, AotHandlerRuntime) {
    let root = test_fs::temp_path("serve", "aot_request_owned_invocation");
    let web_root = root.join("_build/web");
    fs::create_dir_all(&web_root).expect("create native handler output");
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "src/app/AsyncHandler.terl",
        SOURCE,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
        crate::validation::target_profile::TargetProfile::Vm,
    )
    .expect("compile async handler source");
    let (core, router) = crate::compiler::router::prepare_aot_router_module(&artifacts.core)
        .expect("prepare module");
    let image = crate::commands::build::vm_artifact::native_image::compile_serve_native_image(
        &web_root,
        "app_AsyncHandler",
        &core,
    )
    .expect("compile handler native image")
    .expect("handler produces native image");
    let runtime = AotHandlerRuntime::load("app.AsyncHandler".to_string(), &image, router)
        .expect("load handler runtime");
    (root, runtime)
}

/// Builds one managed request argument accepted by the generated handler.
fn request() -> ReplValue {
    let empty_map = || ReplValue::Map(Vec::new());
    ReplValue::Tuple(vec![
        ReplValue::Int(0),
        ReplValue::String("GET".to_string()),
        ReplValue::String("/delayed:".to_string()),
        empty_map(),
        ReplValue::String(String::new()),
        ReplValue::String(String::new()),
        empty_map(),
        empty_map(),
        empty_map(),
        ReplValue::Tuple(vec![empty_map(), ReplValue::List(Vec::new())]),
    ])
}

/// Starts one generated request and requires it to park on typed string I/O.
fn waiting(runtime: &AotHandlerRuntime) -> AotHandlerInvocation {
    match runtime
        .begin_request_invocation("app.AsyncHandler", "delayed", vec![request()])
        .expect("enter generated handler")
    {
        AotHandlerInvocationStep::Waiting(invocation) => invocation,
        AotHandlerInvocationStep::Complete(value) => {
            panic!("handler completed before I/O wake: {value:?}")
        }
    }
}

/// Proves exact typed wake ownership from generated handler entry to response.
#[test]
fn request_owned_handler_resumes_only_from_exact_typed_io_wake() {
    let (root, runtime) = runtime();
    let first = waiting(&runtime);
    let first_wait = first.wait().expect("first typed wait");
    assert_eq!(first_wait.boundary_type(), &TvmBoundaryType::String);

    let second = waiting(&runtime);
    let stale = first_wait.wake(ReplValue::String("ready".to_string()));
    let error = second
        .resume(stale.clone())
        .expect_err("foreign request wake must fail");
    assert!(error.contains("error[pure_native_io.identity]"), "{error}");

    let completed = first.resume(stale).expect("resume exact request wake");
    let AotHandlerInvocationStep::Complete(value) = completed else {
        panic!("single I/O handler must complete after wake")
    };
    let response = HandlerResponse::from_vm_response_with_package_root(&value, &root)
        .expect("decode generated response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ready");

    let wrong_type = waiting(&runtime);
    let wake = wrong_type
        .wait()
        .expect("typed wait")
        .wake(ReplValue::Int(7));
    let error = wrong_type
        .resume(wake)
        .expect_err("wrong typed payload must fail");
    assert!(error.contains("String"), "{error}");

    let error = runtime
        .execute_immediate_native("app.AsyncHandler", "delayed", vec![request()], &mut |_| {})
        .expect_err("immediate native callback must reject asynchronous I/O");
    assert!(
        error.contains("error[serve.aot.async_io_unavailable]"),
        "{error}"
    );

    fs::remove_dir_all(root).expect("cleanup native handler fixture");
}
