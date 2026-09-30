//! JSON response composition crosses the JSON package boundary, not its storage layout.

use super::compile_native_handler_fixture;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::runtime::vm::pure_native::repl_value_to_boundary_term;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

#[test]
fn source_json_response_serializes_live_resources_before_building_response() {
    let fixture = compile_native_handler_fixture(
        "source_json_response",
        "src/app/JsonResponse.terl",
        "app_JsonResponse",
        r#"module app.JsonResponse.
import std.data.Json.
import std.core.Result.{Ok, Err}.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(text: String, status: Int): Response ->
    case Json.parse(text) {
        Ok(value) -> Response.json(value, status).with_header("X-Source", "kept");
        Err(_) -> Response.text("invalid JSON", 400)
    }.
pub string(text: String): Response -> Response.json(Json.string(text)).
"#,
    );
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.JsonResponse".into(),
        &fixture.image,
        None,
        1,
    )
    .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for (function, input, expected, status) in [
        ("handle", "null", "null", 201),
        ("handle", "[1,true,null]", "[1,true,null]", 202),
        (
            "handle",
            r#"{ "z": 1, "a": "value" }"#,
            r#"{"a":"value","z":1}"#,
            203,
        ),
        ("handle", "9223372036854775807", "9223372036854775807", 200),
        ("string", "quote\"\n\\\0", r#""quote\"\n\\\u0000""#, 200),
        ("string", "", r#""""#, 200),
        ("string", "\u{e9}\u{1f642}", "\"\u{e9}\u{1f642}\"", 200),
        ("handle", "{", "invalid JSON", 400),
        ("handle", "null trailing", "invalid JSON", 400),
    ] {
        let mut args = vec![ReplValue::String(input.into())];
        if function == "handle" {
            args.push(ReplValue::Int(status));
        }
        let mut step = runtime
            .begin_request_invocation("app.JsonResponse", function, args)
            .unwrap();
        let operations = [
            if function == "handle" {
                "parse"
            } else {
                "string"
            },
            "to_string",
        ];
        let operations = if status == 400 {
            &operations[..1]
        } else {
            &operations[..]
        };
        for operation in operations {
            step = resume_json_step(step, &mut helpers, operation);
        }
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("response composition must not invoke an HTTP JSON resource operation");
        };
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &fixture.root)
                .unwrap();
        assert_eq!(response.status, status as u16);
        assert_eq!(
            response.content_type,
            if status == 400 {
                "text/plain; charset=utf-8"
            } else {
                "application/json; charset=utf-8"
            }
        );
        assert_eq!(response.body.as_bytes(), expected.as_bytes());
        let has_header = function == "handle" && status != 400;
        assert_eq!(response.headers.len(), usize::from(has_header));
        if has_header {
            assert_eq!(response.headers[0], ("X-Source".into(), "kept".into()));
        }
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_request_json_and_result_predicates_replace_retired_vm_operations() {
    let fixture = compile_native_handler_fixture(
        "source_request_json_result",
        "src/app/RequestJson.terl",
        "app_RequestJson",
        r#"module app.RequestJson.
import std.core.Atom.
import std.core.Result.
import std.core.Result.{Ok, Err}.
import std.http.Request.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    let parsed = request.body_json();
    case parsed {
        Ok(value) -> Response.json(value, if { Result.is_ok(parsed) -> 201; true -> 500 });
        Err(reason) -> Response.text(Atom.to_string(reason.code) + ":" + reason.message,
            if { Result.is_ok(parsed) -> 500; true -> 400 })
    }.
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.RequestJson".into(), &fixture.image, None, 1)
            .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for (input, canonical) in [
        (
            r#"{"enabled":true,"count":2}"#,
            Some(r#"{"count":2,"enabled":true}"#),
        ),
        ("null", Some("null")),
        ("", None),
        ("{", None),
        ("[1,]", None),
        ("{} trailing", None),
        ("\0", None),
        ("[1,true,null]", Some("[1,true,null]")),
    ] {
        let request = crate::terlan_native::http::Request::from_parts("POST", "/", input);
        let request =
            crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned(
                request.into_parts(),
                crate::runtime::native::http::RequestFieldProjection::Complete,
            );
        let step = runtime
            .begin_request_invocation("app.RequestJson", "handle", vec![request])
            .unwrap();
        let mut step = resume_json_step(step, &mut helpers, "parse");
        if canonical.is_some() {
            step = resume_json_step(step, &mut helpers, "to_string");
        }
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("Result predicates and HTTP error mapping must execute as source");
        };
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &fixture.root)
                .unwrap();
        match canonical {
            Some(expected) => {
                assert_eq!(response.status, 201);
                assert_eq!(response.content_type, "application/json; charset=utf-8");
                assert_eq!(response.body.as_bytes(), expected.as_bytes());
            }
            None => {
                let error = crate::terlan_native::json::parse(input).unwrap_err();
                assert_eq!(response.status, 400);
                assert_eq!(response.content_type, "text/plain; charset=utf-8");
                assert_eq!(
                    response.body.as_bytes(),
                    format!("http.body_json:{}", error.message()).as_bytes()
                );
            }
        }
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

fn resume_json_step(
    step: AotHandlerInvocationStep,
    helpers: &mut VmPackageNativeHelpers,
    operation: &str,
) -> AotHandlerInvocationStep {
    let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
        panic!("JSON operation must use its package resource boundary");
    };
    let request = invocation.request().unwrap();
    assert_eq!(request.operation, format!("std.data.json.{operation}"));
    let value = helpers.call(1, request, &[]).unwrap();
    invocation
        .resume(NativeBoundaryReplyTerm::Ok(
            repl_value_to_boundary_term(value).unwrap(),
        ))
        .unwrap()
}
