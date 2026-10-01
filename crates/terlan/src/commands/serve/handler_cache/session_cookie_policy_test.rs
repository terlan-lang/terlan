//! Session response policy executes source and suspends for the package cookie codec.

use super::super::{AotHandlerGeneration, AotHandlerRuntime};
use super::compile_native_handler_fixture;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::runtime::vm::http_session::{self, VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn session_cookie_policy_omits_empty_replays_pending_and_serializes_expiration() {
    let fixture = compile_native_handler_fixture(
        "source_session_cookie_policy",
        "src/app/SessionCookies.terl",
        "app_SessionCookies",
        r#"module app.SessionCookies.
import std.http.Session.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(session: Session): Response ->
    Session.with_response(
        Response.text("session", 209).with_header("X-Before", "kept")
            .with_header("Set-Cookie", "existing=one"), session
    ).with_header("X-After", "kept").
pub expire(session: Session): Response ->
    session.expire();
    handle(session).
"#,
    );
    let mut state = VmHttpSessionRuntime::new("cookie-policy", 100).unwrap();
    let handle = http_session::current(&mut state, None).unwrap().session;
    let pending = handle.managed_id().to_owned();
    let sessions = VmHttpSessionService::new(state);
    let runtime = AotHandlerRuntime {
        module: "app.SessionCookies".into(),
        generation: Arc::new(
            AotHandlerGeneration::load_with_shard_count(&fixture.image, sessions.clone(), 1)
                .unwrap(),
        ),
        router: None,
        primary_request_projection: None,
        request_projections: HashMap::new(),
    };
    let session = |cookie: &str| ReplValue::Record {
        name: "Session".into(),
        fields: vec![
            (
                "identity".into(),
                ReplValue::String(handle.managed_id().into()),
            ),
            ("pending_identity".into(), ReplValue::String(cookie.into())),
        ],
    };
    let mut helpers = VmPackageNativeHelpers::default();
    let protocol = crate::runtime::vm::protocol_task_executor::next_protocol_task_route(
        crate::runtime::vm::scheduler_topology::VmSchedulerId::primary(),
    )
    .unwrap();
    let protocol_step =
        crate::runtime::vm::protocol_task_executor::with_protocol_task_for_test(protocol, || {
            runtime.begin_request_invocation("app.SessionCookies", "handle", vec![session("")])
        })
        .unwrap();
    assert!(matches!(
        protocol_step,
        AotHandlerInvocationStep::Complete(_)
    ));
    assert!(
        runtime.generation.shards[0].initialized().is_none(),
        "granted context must execute on the protocol owner without starting a second owner"
    );
    for invalid in ["bad;value", "bad\r\nvalue", "bad\0value"] {
        let step = runtime
            .begin_request_invocation("app.SessionCookies", "handle", vec![session(invalid)])
            .unwrap();
        let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
            panic!("pending identity must be validated by the package codec");
        };
        let request = invocation.request().unwrap();
        assert_eq!(
            request.operation,
            "std.http.cookies.set_header_with_options"
        );
        let error = helpers.call(1, request, &[]).unwrap_err();
        assert!(
            invocation
                .resume(NativeBoundaryReplyTerm::Error {
                    code: "cookie.invalid".into(),
                    message: error.to_string(),
                    offset: 0,
                })
                .is_err(),
            "invalid cookie input must never complete a response"
        );
        assert!(sessions
            .with_storage(|state| state.is_live(&handle))
            .unwrap());
    }
    let deletion = "terlan_session=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT";
    for (function, cookie, expired) in [
        ("handle", "", false),
        ("handle", pending.as_str(), false),
        ("handle", pending.as_str(), false),
        ("expire", pending.as_str(), true),
        ("handle", pending.as_str(), true),
        ("handle", "", true),
    ] {
        let mut step = runtime
            .begin_request_invocation("app.SessionCookies", function, vec![session(cookie)])
            .unwrap();
        let expected_cookie = if expired {
            deletion.to_string()
        } else {
            format!("terlan_session={cookie}; HttpOnly; SameSite=Lax; Path=/")
        };
        if expired || !cookie.is_empty() {
            let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
                panic!("session cookies must use the package codec");
            };
            let request = invocation.request().unwrap();
            assert_eq!(
                request.operation,
                "std.http.cookies.set_header_with_options"
            );
            let ReplValue::String(header) = helpers.call(1, request, &[]).unwrap() else {
                panic!("cookie codec must return text");
            };
            assert_eq!(header, expected_cookie);
            step = invocation
                .resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                    header,
                )))
                .unwrap();
        }
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("omitted cookies require no codec; emitted cookies resume after one codec");
        };
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &fixture.root)
                .unwrap();
        let mut expected = vec![
            ("X-Before".into(), "kept".into()),
            ("Set-Cookie".into(), "existing=one".into()),
        ];
        if expired || !cookie.is_empty() {
            expected.push(("Set-Cookie".into(), expected_cookie));
        }
        expected.push(("X-After".into(), "kept".into()));
        assert_eq!(response.headers, expected);
        assert_eq!(response.status, 209);
        assert_eq!(response.body.as_bytes(), b"session");
    }
    let step = runtime
        .begin_request_invocation("app.SessionCookies", "handle", vec![session(&pending)])
        .unwrap();
    let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
        panic!("expired session must still require codec validation");
    };
    assert!(
        invocation
            .resume(NativeBoundaryReplyTerm::Error {
                code: "cookie.invalid".into(),
                message: "codec failure".into(),
                offset: 0,
            })
            .is_err(),
        "codec failure must not return a response"
    );
    assert!(sessions
        .with_storage(|state| state.snapshots().is_empty())
        .unwrap());
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
