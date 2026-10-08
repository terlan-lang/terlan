//! Source composition through real package codecs and explicit scheduler replies.

use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::commands::serve::handler_cache::AotHandlerRuntime;
use crate::runtime::vm::package_native_helper::VmPackageNativeHelpers;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

#[test]
fn source_cookie_options_are_explicit_before_native_serialization() {
    let fixture = compile_native_handler_fixture(
        "source_cookie_options",
        "src/app/CookieOptions.terl",
        "app_CookieOptions",
        r#"module app.CookieOptions.
import std.http.Cookies.
pub handle(domain: String, age: Int, include_age: Bool, expires: String, policy: String): String ->
    Cookies.set_header_with_options("sid", "value", "/", domain, age, include_age, expires, false, false, policy).
"#,
    );
    let runtime = AotHandlerRuntime::load_with_shard_count(
        "app.CookieOptions".into(),
        &fixture.image,
        None,
        1,
    )
    .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for (domain, age, include_age, expires, policy, normalized, valid) in [
        ("", 42, false, "", "", "", true),
        ("example.com", 0, true, "", "LaX", "lax", true),
        (
            "",
            -1,
            true,
            "Thu, 01 Jan 1970 00:00:00 GMT",
            "STRICT",
            "strict",
            true,
        ),
        ("", i64::MAX, false, "", "NONE", "none", true),
        (" ", 0, false, "", "", "", true),
        ("", 0, false, "", " lax ", " lax ", false),
        ("", 0, false, "", "UNKNOWN", "unknown", false),
        ("bad\r\nInjected", 0, false, "", "", "", false),
        ("", 0, false, "not a date", "", "", false),
    ] {
        let step = runtime
            .begin_request_invocation(
                "app.CookieOptions",
                "handle",
                vec![
                    ReplValue::String(domain.into()),
                    ReplValue::Int(age),
                    ReplValue::Bool(include_age),
                    ReplValue::String(expires.into()),
                    ReplValue::String(policy.into()),
                ],
            )
            .unwrap();
        let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
            panic!("source options must reach the codec");
        };
        let request = invocation.request().unwrap();
        assert_eq!(request.operation, "std.http.cookies.encode");
        let optional_text = |text: &str| {
            crate::runtime::vm::native_value::from_native((!text.is_empty()).then_some(text).into())
        };
        assert_eq!(
            request.package_arguments.as_ref().unwrap(),
            &vec![
                ReplValue::String("sid".into()),
                ReplValue::String("value".into()),
                ReplValue::String("/".into()),
                optional_text(domain),
                crate::runtime::vm::native_value::from_native(include_age.then_some(age).into()),
                optional_text(expires),
                ReplValue::Bool(false),
                ReplValue::Bool(false),
                optional_text(normalized),
            ]
        );
        let result = helpers.call(1, request, &[]);
        if valid {
            let ReplValue::String(header) = result.unwrap() else {
                panic!("cookie text")
            };
            assert_eq!(header.contains("Max-Age="), include_age);
            assert_eq!(header.contains("Domain="), !domain.is_empty());
            assert_eq!(header.contains("Expires="), !expires.is_empty());
            assert_eq!(header.contains("SameSite="), !policy.is_empty());
            let AotHandlerInvocationStep::Complete(value) = invocation
                .resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                    header.clone(),
                )))
                .unwrap()
            else {
                panic!("source returns codec output")
            };
            assert_eq!(value, ReplValue::String(header));
        } else {
            let error = result.unwrap_err();
            assert!(invocation
                .resume(NativeBoundaryReplyTerm::Error {
                    code: "cookie.invalid".into(),
                    message: error.to_string(),
                    offset: 0,
                })
                .is_err());
        }
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_cookie_jar_preserves_incoming_snapshot_and_replays_mutations() {
    let fixture = compile_native_handler_fixture(
        "source_cookie_jar",
        "src/app/CookieJar.terl",
        "app_CookieJar",
        r#"module app.CookieJar.
import std.http.Request.
import std.http.Response.
import std.http.Cookies.
import std.core.Option.{with_default}.
import std.core.Unit.
import type std.http.Response.{Response}.
pub handle(request: Request): Response ->
    let jar = request.cookies();
    let snapshot = jar;
    let first = jar.set("session", "new");
    let second = jar.delete("session");
    jar.set("other", "last", "/private", true, true);
    let fresh = request.cookies();
    let body = case first == Unit and second == Unit and
        with_default(jar.get("missing"), "absent") == "absent" and
        with_default(request.cookie("session"), "missing") == "original" and
        fresh.headers() == [] and
        with_default(fresh.get("session"), "missing") == "original" {
        true -> with_default(jar.get("session"), "missing");
        false -> "invalid-command-result"
    };
    let response = Response.text(body);
    response.with_cookies(snapshot).with_cookies(jar).

pub invalid(request: Request, name: String, value: String, path: String, remove: Bool): Response ->
    let jar = request.cookies();
    case remove {
        true -> jar.delete(name, path);
        false -> jar.set(name, value, path)
    };
    Response.text("must not complete").
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.CookieJar".into(), &fixture.image, None, 1)
            .unwrap();
    let request = terlan_http_native::Request::from_parts_with_raw_query_metadata(
        "GET",
        "/",
        "",
        terlan_http_native::RequestMetadata {
            cookies: vec![
                ("session".into(), "original".into()),
                ("session".into(), "shadowed".into()),
            ],
            ..Default::default()
        },
    );
    let value =
        crate::commands::serve::handler::request_materialization::vm_request_descriptor_owned(
            request.into_parts(),
            terlan_http_native::RequestFieldProjection::Complete,
        );
    let mut step = runtime
        .begin_request_invocation("app.CookieJar", "handle", vec![value.clone()])
        .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    let mut headers = Vec::new();
    for _ in 0..3 {
        let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
            panic!("jar mutation must suspend for package validation");
        };
        assert_eq!(
            invocation.request().unwrap().operation,
            "std.http.cookies.encode"
        );
        let ReplValue::String(header) =
            helpers.call(1, invocation.request().unwrap(), &[]).unwrap()
        else {
            panic!("serialized cookie");
        };
        headers.push(("Set-Cookie".to_string(), header.clone()));
        step = invocation
            .resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                header,
            )))
            .unwrap();
    }
    let AotHandlerInvocationStep::Complete(response_value) = step else {
        panic!("source replay must complete without native jar operations");
    };
    let response =
        crate::commands::serve::handler::decode_owned_response(response_value, &fixture.root)
            .unwrap();
    assert_eq!(response.headers[2..], headers);
    assert_eq!(
        response.body.as_bytes().expect("finite response"),
        b"original"
    );
    for (name, content, path, remove) in [
        ("bad name", "valid", "/", false),
        ("$reserved", "valid", "/", false),
        ("embedded$dollar", "valid", "/", false),
        ("trailing$", "valid", "/", true),
        ("name\u{212a}", "valid", "/", false),
        ("session", "bad;value", "/", false),
        ("session", "valid", "relative", false),
        ("bad name", "", "/", true),
        ("session", "", "relative", true),
    ] {
        let step = runtime
            .begin_request_invocation(
                "app.CookieJar",
                "invalid",
                vec![
                    value.clone(),
                    ReplValue::String(name.into()),
                    ReplValue::String(content.into()),
                    ReplValue::String(path.into()),
                    ReplValue::Bool(remove),
                ],
            )
            .unwrap();
        let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
            panic!("invalid mutation must suspend for validation");
        };
        assert_eq!(
            invocation.request().unwrap().operation,
            "std.http.cookies.encode"
        );
        let error = helpers
            .call(1, invocation.request().unwrap(), &[])
            .unwrap_err();
        assert!(
            invocation
                .resume(NativeBoundaryReplyTerm::Error {
                    code: "cookie.invalid".into(),
                    message: error.to_string(),
                    offset: 0,
                })
                .is_err(),
            "invalid mutation must not return a response"
        );
    }
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_cookie_composition_suspends_for_package_codecs_and_resumes_in_order() {
    let fixture = compile_native_handler_fixture(
        "source_cookie_composition",
        "src/app/Cookie.terl",
        "app_Cookie",
        r#"module app.Cookie.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(name: String): Response ->
    let response = Response.text("cookies", 210);
    response.cookie(name, "one");
    response.cookie_with_options("full", "two", "/private", "example.com", 60, true, "", true, true, "Lax");
    response.delete_cookie("old");
    response.with_cookie("last", "three")
        .with_cookie_options("defaults", "four")
        .with_deleted_cookie("gone", "/private").
"#,
    );
    let runtime =
        AotHandlerRuntime::load_with_shard_count("app.Cookie".into(), &fixture.image, None, 1)
            .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    let mut step = runtime
        .begin_request_invocation(
            "app.Cookie",
            "handle",
            vec![ReplValue::String("first".into())],
        )
        .unwrap();
    let mut expected_headers = Vec::new();
    for _ in 0..6 {
        let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
            panic!("cookie serialization must use the package boundary");
        };
        let request = invocation.request().unwrap();
        assert_eq!(request.operation, "std.http.cookies.encode");
        let ReplValue::String(header) = helpers.call(1, request, &[]).unwrap() else {
            panic!("cookie codec must return a string");
        };
        expected_headers.push(("Set-Cookie".to_string(), header.clone()));
        step = invocation
            .resume(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Text(
                header,
            )))
            .unwrap();
    }
    let AotHandlerInvocationStep::Complete(value) = step else {
        panic!("all codec replies must complete the response");
    };
    let response =
        crate::commands::serve::handler::decode_owned_response(value, &fixture.root).unwrap();
    assert_eq!(response.status, 210);
    assert_eq!(response.headers[2..], expected_headers);
    assert_eq!(response.headers[2].1, "first=one; Path=/");
    assert_eq!(response.headers[5].1, "last=three; Path=/");
    assert_eq!(response.headers[6].1, "defaults=four; Path=/");
    for attribute in [
        "Path=/private",
        "Domain=example.com",
        "Max-Age=60",
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
    ] {
        assert!(response.headers[3].1.contains(attribute), "{attribute}");
    }
    assert!(response.headers[4].1.starts_with("old=;"));
    assert!(response.headers[7].1.contains("Path=/private"));

    // Invalid input must stop at the first codec, not append a header or continue.
    let step = runtime
        .begin_request_invocation(
            "app.Cookie",
            "handle",
            vec![ReplValue::String("bad\r\nname".into())],
        )
        .unwrap();
    let AotHandlerInvocationStep::CapabilityWaiting(invocation) = step else {
        panic!("invalid name still requires codec validation");
    };
    let error = helpers
        .call(1, invocation.request().unwrap(), &[])
        .unwrap_err();
    let result = invocation.resume(NativeBoundaryReplyTerm::Error {
        code: "cookie.invalid".into(),
        message: error.to_string(),
        offset: 0,
    });
    assert!(result.is_err(), "codec failure must not produce a response");
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
