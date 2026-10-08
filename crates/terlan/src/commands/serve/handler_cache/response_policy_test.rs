//! Source security policy executes through the real compiled HTTP response path.

#[path = "middleware_value_test.rs"]
mod middleware_value_test;
#[path = "response_cookie_test.rs"]
mod response_cookie_test;
#[path = "response_header_test.rs"]
mod response_header_test;
#[path = "response_json_test.rs"]
mod response_json_test;
#[path = "response_source_contract_test.rs"]
mod response_source_contract_test;
#[path = "response_sse_test.rs"]
mod response_sse_test;
#[path = "response_stream_test.rs"]
mod response_stream_test;
#[path = "session_cookie_policy_test.rs"]
mod session_cookie_policy_test;
#[path = "session_lifecycle_cookie_test.rs"]
mod session_lifecycle_cookie_test;
#[path = "session_provider_test.rs"]
mod session_provider_test;

use super::handler_cache_test_support::compile_native_handler_fixture;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

#[test]
fn response_builders_execute_declarations_with_provider_owned_defaults() {
    let fixture = compile_native_handler_fixture(
        "response_builder_defaults",
        "src/app/Builders.terl",
        "app_Builders",
        r#"module app.Builders.
import std.http.Response.
import type std.http.Response.{Response}.
pub literal(value: String, status: Int = 219): Response -> Response.text(value, status).
pub handle(kind: Int, body: String): Response ->
    case kind {
        0 -> literal(body);
        1 -> literal(body, 221);
        2 -> Response.html(body);
        3 -> Response.json_text(body, 201);
        _ -> Response.redirect(body)
    }.
"#,
    );
    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).unwrap();
    let owner = shard
        .spawn_fixed_owner_actor("app.Builders.handle", 2)
        .unwrap();
    for (input, expected_type, expected_status) in [
        (0, "text/plain; charset=utf-8", 219),
        (1, "text/plain; charset=utf-8", 221),
        (2, "text/html; charset=utf-8", 200),
        (3, "application/json; charset=utf-8", 201),
        (4, "text/plain; charset=utf-8", 302),
    ] {
        let result = shard
            .call_on_admitted_fixed_owner(
                owner,
                "app.Builders.handle",
                &[
                    ReplValue::Int(input),
                    ReplValue::String("dynamic-body".into()),
                ],
            )
            .unwrap();
        let value = result;
        let response =
            crate::commands::serve::handler::decode_owned_response(value, &fixture.root).unwrap();
        assert_eq!(response.content_type, expected_type);
        assert_eq!(response.status, expected_status);
        assert_eq!(
            response.headers[..2],
            [
                ("Cache-Control".into(), "no-cache".into()),
                ("X-Content-Type-Options".into(), "nosniff".into())
            ]
        );
        if input == 4 {
            assert!(response
                .body
                .as_bytes()
                .expect("finite response")
                .is_empty());
            assert_eq!(
                response.headers[2..],
                [("Location".into(), "dynamic-body".into())]
            );
        } else {
            assert_eq!(
                response.body.as_bytes().expect("finite response"),
                b"dynamic-body"
            );
            assert_eq!(response.headers.len(), 2);
        }
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_security_policy_covers_dynamic_options_and_hsts_boundaries() {
    let fixture = compile_native_handler_fixture(
        "source_security_policy",
        "src/app/Policy.terl",
        "app_Policy",
        r#"module app.Policy.
import std.http.Response.
import type std.http.Response.{Response}.
import std.http.Response.{SecurityHeaders, Deny, SameOrigin, NoReferrer, StrictOriginWhenCrossOrigin}.
pub handle(content: Bool, alternate: Bool, age: Int, subdomains: Bool): Response ->
    let policy = SecurityHeaders(
        content,
        if { alternate -> SameOrigin; true -> Deny },
        if { alternate -> NoReferrer; true -> StrictOriginWhenCrossOrigin },
        age,
        subdomains
    );
    Response.text("policy").with_header("X-Original", "kept").with_security_headers(policy).
"#,
    );
    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).unwrap();
    let owner = shard
        .spawn_fixed_owner_actor("app.Policy.handle", 4)
        .unwrap();
    for content in [false, true] {
        for alternate in [false, true] {
            for age in [i64::MIN, -1, 0, 1, 60, i64::MAX] {
                for subdomains in [false, true] {
                    let result = shard
                        .call_on_admitted_fixed_owner(
                            owner,
                            "app.Policy.handle",
                            &[
                                ReplValue::Bool(content),
                                ReplValue::Bool(alternate),
                                ReplValue::Int(age),
                                ReplValue::Bool(subdomains),
                            ],
                        )
                        .unwrap();
                    let value = result;
                    let response = crate::commands::serve::handler::decode_owned_response(
                        value,
                        &fixture.root,
                    )
                    .unwrap();
                    let mut headers = vec![
                        ("Cache-Control".into(), "no-cache".into()),
                        ("X-Content-Type-Options".into(), "nosniff".into()),
                        ("X-Original".into(), "kept".into()),
                        (
                            "X-Frame-Options".into(),
                            if alternate { "SAMEORIGIN" } else { "DENY" }.into(),
                        ),
                        (
                            "Referrer-Policy".into(),
                            if alternate {
                                "no-referrer"
                            } else {
                                "strict-origin-when-cross-origin"
                            }
                            .into(),
                        ),
                    ];
                    if content {
                        headers.push(("X-Content-Type-Options".into(), "nosniff".into()));
                    }
                    if age > 0 {
                        let suffix = if subdomains {
                            "; includeSubDomains"
                        } else {
                            ""
                        };
                        headers.push((
                            "Strict-Transport-Security".into(),
                            format!("max-age={age}{suffix}"),
                        ));
                    }
                    assert_eq!(
                        response.headers, headers,
                        "{content}/{alternate}/{age}/{subdomains}"
                    );
                    assert_eq!(response.status, 200);
                    assert_eq!(
                        response.body.as_bytes().expect("finite response"),
                        b"policy"
                    );
                }
            }
        }
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
