//! Source response methods retain metadata; package admission validates it.

use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::commands::serve::response_rendering::serve_vm_stream_handler_response;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;

#[test]
fn compiled_response_headers_use_package_admission_without_name_substitution() {
    let fixture = compile_native_handler_fixture(
        "source_header_admission",
        "src/app/Headers.terl",
        "app_Headers",
        r#"module app.Headers.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(name: String, value: String): Response ->
    Response.text("source body", 218)
        .with_header("Set-Cookie", "a=1")
        .with_header("Set-Cookie", "b=2")
        .with_header(name, value).
"#,
    );
    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).unwrap();
    let owner = shard
        .spawn_fixed_owner_actor("app.Headers.handle", 2)
        .unwrap();
    for (name, value, valid) in [
        ("X-Test", "unchanged", true),
        ("Cache-Control", "public, max-age=60", true),
        ("cAcHe-CoNtRoL", "private, no-store", true),
        ("CACHE-CONTROL", "no-cache", true),
        ("X-Frame-Options", "DENY", true),
        ("X-Test", "", true),
        ("X-Test", "\t", true),
        ("", "value", false),
        ("bad header", "value", false),
        ("Content-Type", "text/plain", false),
        ("CONTENT-LENGTH", "4", false),
        ("connection", "close", false),
        ("tRaNsFeR-EnCoDiNg", "chunked", false),
        ("Trailer", "X-Trailer", false),
        ("X-Test", "bad\0value", false),
        ("X-Test", "bad\u{7f}value", false),
        ("X-Test", "bad\r\nInjected: yes", false),
    ] {
        let expected_cache = if name.eq_ignore_ascii_case("cache-control") {
            value
        } else {
            "no-cache"
        };
        let value = shard
            .call_on_admitted_fixed_owner(
                owner,
                "app.Headers.handle",
                &[
                    ReplValue::String(name.into()),
                    ReplValue::String(value.into()),
                ],
            )
            .unwrap();
        let result = crate::commands::serve::handler::decode_owned_response(value, &fixture.root);
        if !valid {
            assert!(
                result
                    .unwrap_err()
                    .starts_with("error[serve_handler]: response header"),
                "{name}"
            );
            continue;
        }
        let response = serve_vm_stream_handler_response(result.unwrap(), false).unwrap();
        assert_eq!(response.status(), 218);
        assert_eq!(response.body().as_ref(), b"source body");
        assert_eq!(response.headers()[http::header::CONTENT_LENGTH], "11");
        assert_eq!(response.headers()["cache-control"], expected_cache);
        assert_eq!(
            response.headers().get_all("cache-control").iter().count(),
            1
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(
            response
                .headers()
                .get_all("x-content-type-options")
                .iter()
                .count(),
            1
        );
        assert_eq!(
            response
                .headers()
                .get_all(http::header::SET_COOKIE)
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["a=1", "b=2"]
        );
        assert!(response.headers().contains_key(name));
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
