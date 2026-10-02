//! SSE response policy executes as source, with only event framing crossing the package boundary.

use super::compile_native_handler_fixture;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::commands::serve::response_rendering::serve_vm_stream_handler_response;
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::runtime::vm::pure_native::repl_value_to_boundary_term;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use terlan_http_native::{http1::write_http1_response, HttpResponseChunks};

#[test]
fn source_sse_response_uses_package_codec_and_generic_stream_transport() {
    let fixture = compile_native_handler_fixture(
        "source_sse_response",
        "src/app/Events.terl",
        "app_Events",
        r#"module app.Events.
import std.http.Sse.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(data: String): Response ->
    Sse.response([Sse.data(data).with_id("42").with_name("update").with_retry_ms(1500),
        Sse.data("end")], 201).with_header("X-Source", "kept").
pub empty(): Response -> Sse.response([]).
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Events".into(), &fixture.image, None, 1)
            .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for (function, input, normalized, expected_data) in [
        ("empty", "", "", ""),
        ("handle", "one\r\ntwo", "one\ntwo", "data: one\ndata: two\n"),
        ("handle", "one\rtwo", "one\ntwo", "data: one\ndata: two\n"),
        ("handle", "\r\r\n", "\n\n", "data: \ndata: \ndata: \n"),
        (
            "handle",
            "a\r\nb\rc\n",
            "a\nb\nc\n",
            "data: a\ndata: b\ndata: c\ndata: \n",
        ),
        (
            "handle",
            "\r\nevent: injected",
            "\nevent: injected",
            "data: \ndata: event: injected\n",
        ),
        (
            "handle",
            "\n\nevent: injected",
            "\n\nevent: injected",
            "data: \ndata: \ndata: event: injected\n",
        ),
        (
            "handle",
            "\u{e9}\u{1f642}\r\n\0",
            "\u{e9}\u{1f642}\n\0",
            "data: \u{e9}\u{1f642}\ndata: \0\n",
        ),
        ("handle", "", "", ""),
    ] {
        let empty = function == "empty";
        let args = if empty {
            vec![]
        } else {
            vec![ReplValue::String(input.into())]
        };
        let mut step = runtime
            .begin_request_invocation("app.Events", function, args)
            .unwrap();
        for index in 0..if empty { 0 } else { 2 } {
            let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
                panic!("event framing must reach the package boundary");
            };
            let request = invocation.request().unwrap();
            assert_eq!(request.operation, "std.http.sse.encode_event");
            let arguments = request.package_arguments.as_ref().unwrap();
            assert_eq!(arguments.len(), 4);
            assert_eq!(
                arguments[3],
                ReplValue::String(if index == 0 { normalized } else { "end" }.into()),
                "newline policy must execute in source before the codec: {input:?}"
            );
            let value = helpers.call(1, request, &[]).unwrap();
            step = invocation
                .resume(NativeBoundaryReplyTerm::Ok(
                    repl_value_to_boundary_term(value).unwrap(),
                ))
                .unwrap();
        }
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("response construction must not invoke a reserved HTTP response operation");
        };
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &fixture.root)
                .unwrap();
        assert_eq!(response.status, if empty { 200 } else { 201 });
        assert_eq!(response.content_type, "text/event-stream; charset=utf-8");
        if !empty {
            assert_eq!(response.headers, [("X-Source".into(), "kept".into())]);
        }
        let response = serve_vm_stream_handler_response(response, false).unwrap();
        let mut stream = response
            .extensions()
            .get::<HttpResponseChunks>()
            .unwrap()
            .clone();
        if !empty {
            assert_eq!(
                stream.next_chunk().unwrap().as_ref(),
                format!("id: 42\nevent: update\nretry: 1500\n{expected_data}\n").as_bytes()
            );
            assert_eq!(stream.next_chunk().unwrap().as_ref(), b"data: end\n\n");
        }
        assert!(stream.is_complete());
        let mut wire = Vec::new();
        write_http1_response(&mut wire, &response, false).unwrap();
        assert!(wire.ends_with(b"0\r\n\r\n"));
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
