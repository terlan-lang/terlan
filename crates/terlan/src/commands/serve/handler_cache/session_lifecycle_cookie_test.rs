//! Request lookup and rotation expose identities; source owns cookie encoding.

use super::compile_native_handler_fixture;
use crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned;
use crate::commands::serve::handler::HandlerResponse;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::native::http::{Request, RequestFieldProjection, RequestMetadata};
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

#[test]
fn source_lookup_reuse_rotation_and_stale_replacement_use_cookie_package() {
    let fixture = compile_native_handler_fixture(
        "session_lifecycle_cookie",
        "src/app/SessionLifecycle.terl",
        "app_SessionLifecycle",
        r#"module app.SessionLifecycle.
import std.http.Request.
import std.http.Response.
import std.http.Session.
import std.core.Option.{with_default}.
import type std.http.Response.{Response}.
pub handle(request: Request, rotate: Bool): Response ->
    let session = Session.current(request);
    let previous = with_default(session.get("value"), "new");
    session.set("value", "kept");
    let active = if { rotate -> session.rotate(); true -> session };
    Session.with_response(Response.text(previous), active).
"#,
    );
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.SessionLifecycle".into(),
        &fixture.image,
        None,
        1,
    )
    .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    let mut identities = std::collections::BTreeMap::<&str, String>::new();
    for (incoming, rotate, body, replacement) in [
        (Some("s1"), false, "new", Some("s1")),
        (Some("s1"), false, "kept", None),
        (Some(" \ts1\n"), false, "kept", None),
        (Some("\u{2003}s1\u{2003}"), false, "kept", None),
        (Some("s1"), true, "kept", Some("s2")),
        (Some("s2"), false, "kept", None),
        (Some("s1"), false, "new", Some("s3")),
        (Some("unrecognized"), false, "new", Some("s4")),
        (Some(""), false, "new", Some("s5")),
        (Some(" \t\n"), false, "new", Some("s6")),
        (None, false, "new", Some("s7")),
    ] {
        let request = Request::from_parts_with_raw_query_metadata(
            "GET",
            "/",
            "",
            RequestMetadata {
                cookies: incoming
                    .map(|value| {
                        let actual = identities.get(value.trim()).map_or_else(
                            || value.to_string(),
                            |identity| value.replace(value.trim(), identity),
                        );
                        vec![("terlan_session".into(), actual)]
                    })
                    .unwrap_or_default(),
                ..Default::default()
            },
        );
        let mut step = runtime
            .begin_request_invocation(
                "app.SessionLifecycle",
                "handle",
                vec![
                    vm_request_descriptor_owned(
                        request.into_parts(),
                        RequestFieldProjection::Complete,
                    ),
                    ReplValue::Bool(rotate),
                ],
            )
            .unwrap();
        let mut expected = Vec::new();
        if let Some(label) = replacement {
            let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
                panic!("new or rotated identity must suspend for source cookie encoding");
            };
            let request = invocation.request().unwrap();
            assert_eq!(
                request.operation,
                "std.http.cookies.set_header_with_options"
            );
            let ReplValue::String(header) = helpers.call(1, request, &[]).unwrap() else {
                panic!("cookie codec must return a string");
            };
            let identity = terlan_http_native::parse_request_cookie_header(&header)
                .into_iter()
                .find(|(name, _)| name == "terlan_session")
                .unwrap()
                .1;
            assert_eq!(identity.len(), 43);
            assert!(!identities.values().any(|previous| previous == &identity));
            assert_eq!(
                header,
                format!("terlan_session={identity}; HttpOnly; SameSite=Lax; Path=/")
            );
            identities.insert(label, identity);
            expected.push(("Set-Cookie".into(), header.clone()));
            step = invocation
                .resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                    header,
                )))
                .unwrap();
        }
        let AotHandlerInvocationStep::Complete(value) = step else {
            panic!("reused sessions need no header; new cookies need only one codec reply");
        };
        let response =
            HandlerResponse::from_owned_vm_response_with_package_root(value, &fixture.root)
                .unwrap();
        assert_eq!(response.body.as_bytes(), body.as_bytes());
        assert_eq!(response.status, 200);
        assert_eq!(response.headers, expected);
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
