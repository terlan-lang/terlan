//! Source-created callbacks use production serving's actor and wake ownership.

use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::runtime::native::http::{Request, RequestFieldProjection};
use crate::runtime::native_image::managed::{
    ManagedClosureDescriptor, ManagedClosureImageGeneration,
};
use crate::runtime::vm::protocol_task_executor::{
    next_protocol_task_route, with_protocol_task_for_test,
};
use crate::runtime::vm::pure_native::PureNativeExecutionImage;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;
use crate::runtime::vm::{NativeClosureValue, ReplValue};
use crate::support::test_fs::TestDirectory;
use crate::{CliCommand, CliState};

use super::invocation::AotHandlerInvocationStep;
use super::AotHandlerRuntime;

#[test]
fn captured_callbacks_use_serving_admission_resume_and_cancellation() {
    let directory = TestDirectory::new("serve", "owned_callbacks");
    let source = directory.join("callbacks.terl");
    std::fs::write(
        &source,
        r#"
module callbacks.
import type std.http.Router.Handler.
import std.http.Response.
import std.vm.Process.
import std.http.Request.
type Callback = (String) -> String.
type Waiting = () -> String.
pub make(prefix: String): Callback -> (value: String) -> prefix + value.
pub yielded(prefix: String): Callback ->
    (value: String) ->
        Process.yield_now();
        prefix + value.
pub waiting(prefix: String): Waiting -> () -> prefix + Process.receive_string().
pub make_handler(prefix: String): Handler ->
    (request: Request) -> Response.text(prefix + request.path()).
pub expected(body: String): Response -> Response.text(body).
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
        ExitCode::SUCCESS
    );
    let image_path = output.join("vm/callbacks.tvm");
    let image = PureNativeExecutionImage::load(&image_path).unwrap();
    let mut producer = image.spawn_shard().unwrap();
    let prefix = ReplValue::String("captured:".into());
    let callback = producer
        .call("make", std::slice::from_ref(&prefix))
        .unwrap();
    let yielded = producer
        .call("yielded", std::slice::from_ref(&prefix))
        .unwrap();
    let waiting = producer
        .call("waiting", std::slice::from_ref(&prefix))
        .unwrap();
    let handler = producer.call("make_handler", &[prefix]).unwrap();
    drop(producer);
    drop(image);

    let protocol_runtime =
        AotHandlerRuntime::load_with_shard_count("callbacks".into(), &image_path, None, 1).unwrap();
    protocol_callbacks(&protocol_runtime, &callback, &waiting);
    assert!(protocol_runtime.generation.shards[0]
        .initialized()
        .is_none());
    drop(protocol_runtime);

    let runtime =
        AotHandlerRuntime::load_with_shard_count("callbacks".into(), &image_path, None, 1).unwrap();
    assert_eq!(runtime.callable_arity(&callback), Some(1));
    for value in [&callback, &yielded] {
        assert_eq!(
            runtime
                .execute_callable(
                    "callbacks",
                    value,
                    vec![ReplValue::String("value".into())],
                    &mut |_| {}
                )
                .unwrap(),
            ReplValue::String("captured:value".into())
        );
    }
    let request =
        crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned(
            Request::from_parts("GET", "/real", "").into_parts(),
            RequestFieldProjection::Complete,
        );
    let response = runtime
        .execute_callable("callbacks", &handler, vec![request], &mut |_| {})
        .unwrap();
    let expected = runtime
        .execute_immediate_native(
            "callbacks",
            "expected",
            vec![ReplValue::String("captured:/real".into())],
            &mut |_| {},
        )
        .unwrap();
    assert_eq!(response, expected);
    assert!(!format!("{callback:?}").contains("captured:"));
    let error = runtime
        .execute_callable("callbacks", &callback, vec![callback.clone()], &mut |_| {})
        .unwrap_err();
    assert!(error.contains("does not match `String`"), "{error}");
    assert!(!error.contains("captured:"), "{error}");

    // Invalid callers must fail before dispatch and release their routed actors.
    for arguments in [
        vec![],
        vec![ReplValue::Int(1)],
        vec![ReplValue::Unit, ReplValue::Unit],
    ] {
        assert!(runtime
            .execute_callable("callbacks", &callback, arguments, &mut |_| {})
            .is_err());
    }
    let ReplValue::Closure(original) = &callback else {
        panic!("owned closure")
    };
    for captures in [vec![], vec![ReplValue::Int(9)]] {
        let forged = ReplValue::Closure(Arc::new(NativeClosureValue {
            descriptor: Arc::clone(&original.descriptor),
            captures: captures.into_boxed_slice(),
        }));
        assert!(runtime
            .execute_callable(
                "callbacks",
                &forged,
                vec![ReplValue::String("bad".into())],
                &mut |_| {}
            )
            .is_err());
    }
    for (generation, id) in [
        (
            ManagedClosureImageGeneration::new([91; 32]).unwrap(),
            original.descriptor.callable_id(),
        ),
        (original.descriptor.generation(), u64::MAX),
    ] {
        let forged = ReplValue::Closure(Arc::new(NativeClosureValue {
            descriptor: Arc::new(
                ManagedClosureDescriptor::new(
                    generation,
                    id,
                    original.descriptor.parameters().to_vec(),
                    original.descriptor.results().to_vec(),
                    original.descriptor.captures().to_vec(),
                )
                .unwrap(),
            ),
            captures: original.captures.clone(),
        }));
        assert!(runtime
            .execute_callable(
                "callbacks",
                &forged,
                vec![ReplValue::String("bad".into())],
                &mut |_| {}
            )
            .is_err());
    }
    assert!(runtime
        .execute_callable(
            "foreign",
            &callback,
            vec![ReplValue::String("bad".into())],
            &mut |_| {}
        )
        .is_err());
    assert!(runtime
        .begin_closure_invocation(ReplValue::Tuple(vec![]), vec![])
        .is_err());

    let AotHandlerInvocationStep::Waiting(invocation) = runtime
        .begin_closure_invocation(waiting.clone(), vec![])
        .unwrap()
    else {
        panic!("captured callback must park on receive")
    };
    let wake = invocation
        .wait()
        .unwrap()
        .wake(ReplValue::String("resumed".into()));
    let AotHandlerInvocationStep::Complete(value) = invocation.resume(wake).unwrap() else {
        panic!("captured callback must finish after its wake")
    };
    assert_eq!(value, ReplValue::String("captured:resumed".into()));
    let error = runtime
        .execute_callable("callbacks", &waiting, vec![], &mut |_| {})
        .unwrap_err();
    assert!(
        error.to_string().contains("async_io_unavailable"),
        "{error}"
    );
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(Ordering::SeqCst) == 0));
    assert_eq!(
        runtime
            .execute_callable(
                "callbacks",
                &callback,
                vec![ReplValue::String("after errors".into())],
                &mut |_| {}
            )
            .unwrap(),
        ReplValue::String("captured:after errors".into())
    );
}

fn protocol_callbacks(runtime: &AotHandlerRuntime, callback: &ReplValue, waiting: &ReplValue) {
    let protocol = next_protocol_task_route(VmSchedulerId::primary()).unwrap();
    let value = with_protocol_task_for_test(protocol, || {
        runtime.execute_callable(
            "callbacks",
            callback,
            vec![ReplValue::String("protocol".into())],
            &mut |_| {},
        )
    })
    .unwrap();
    assert_eq!(value, ReplValue::String("captured:protocol".into()));
    let AotHandlerInvocationStep::Waiting(invocation) =
        with_protocol_task_for_test(protocol, || {
            runtime.begin_closure_invocation(waiting.clone(), vec![])
        })
        .unwrap()
    else {
        panic!("protocol callback must park")
    };
    let wake = invocation
        .wait()
        .unwrap()
        .wake(ReplValue::String("protocol wake".into()));
    let AotHandlerInvocationStep::Complete(value) =
        with_protocol_task_for_test(protocol, || invocation.resume(wake)).unwrap()
    else {
        panic!("protocol callback must complete")
    };
    assert_eq!(value, ReplValue::String("captured:protocol wake".into()));
    let AotHandlerInvocationStep::Waiting(invocation) =
        with_protocol_task_for_test(protocol, || {
            runtime.begin_closure_invocation(waiting.clone(), vec![])
        })
        .unwrap()
    else {
        panic!("callback must park before cancellation")
    };
    with_protocol_task_for_test(protocol, || invocation.cancel("cancelled".into())).unwrap();
    assert!(runtime
        .generation
        .active_actors
        .iter()
        .all(|count| count.load(Ordering::SeqCst) == 0));
}
