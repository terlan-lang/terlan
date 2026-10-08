//! Response behavior formerly exercised on an unused Rust mirror now runs source.

use super::super::handler_cache_test_support::compile_native_handler_fixture;
use crate::commands::serve::response_rendering::serve_vm_stream_handler_response;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;
use terlan_http_native::source_descriptor::{response, SourceResponseBody};

#[test]
fn source_builders_mutation_and_wire_conversion_share_one_response_contract() {
    let fixture = compile_native_handler_fixture(
        "source_response_contract",
        "src/app/ResponseContract.terl",
        "app_ResponseContract",
        r#"module app.ResponseContract.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(kind: Int, body: String, status: Int, content_type: String, name: String, value: String): Response ->
    let response = case kind {
        0 -> Response.text(body);
        1 -> Response.html(body);
        2 -> Response.json_text(body);
        3 -> Response.redirect(body);
        4 -> Response.file(body, content_type = content_type);
        _ -> Response.stream([body, "", "tail"], 200, content_type, 4, 2)
    };
    response.status(status);
    response.header("Set-Cookie", "a=1");
    response.header("Set-Cookie", "b=2");
    response.header(name, value);
    response.
"#,
    );
    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).unwrap();
    let owner = shard
        .spawn_fixed_owner_actor("app.ResponseContract.handle", 6)
        .unwrap();
    let mut run = |kind, status, body: &str, content_type: &str, name: &str, value: &str| {
        shard
            .call_on_admitted_fixed_owner(
                owner,
                "app.ResponseContract.handle",
                &[
                    ReplValue::Int(kind),
                    ReplValue::String(body.into()),
                    ReplValue::Int(status),
                    ReplValue::String(content_type.into()),
                    ReplValue::String(name.into()),
                    ReplValue::String(value.into()),
                ],
            )
            .unwrap()
    };
    for (kind, content_type) in [
        (0, "text/plain; charset=utf-8"),
        (1, "text/html; charset=utf-8"),
        (2, "application/json; charset=utf-8"),
        (3, "text/plain; charset=utf-8"),
    ] {
        for status in [200, 201, 202, 301, 302, 599] {
            for body in ["", "<main>Hello</main>", "{not-reparsed}", "/login"] {
                let value = run(kind, status, body, "unused", "x-terlan", "yes");
                let admitted =
                    crate::commands::serve::handler::decode_owned_response(value, &fixture.root)
                        .unwrap();
                let wire = serve_vm_stream_handler_response(admitted, false).unwrap();
                assert_eq!(wire.status().as_u16(), status as u16);
                assert_eq!(wire.headers()[http::header::CONTENT_TYPE], content_type);
                assert_eq!(wire.headers()["x-terlan"], "yes");
                assert_eq!(wire.headers()["cache-control"], "no-cache");
                assert_eq!(wire.headers()["x-content-type-options"], "nosniff");
                assert_eq!(
                    wire.headers()
                        .get_all(http::header::SET_COOKIE)
                        .iter()
                        .map(|header| header.to_str().unwrap())
                        .collect::<Vec<_>>(),
                    ["a=1", "b=2"]
                );
                let expected = if kind == 3 { "" } else { body };
                assert_eq!(wire.body().as_ref(), expected.as_bytes());
                assert_eq!(
                    wire.headers()[http::header::CONTENT_LENGTH],
                    expected.len().to_string()
                );
                if kind == 3 {
                    assert_eq!(wire.headers()[http::header::LOCATION], body);
                }
            }
        }
    }
    // File construction records intent only; admission must not open this path.
    let file = response(run(
        4,
        206,
        "missing/report.txt",
        "text/plain",
        "x-terlan",
        "yes",
    ))
    .unwrap();
    assert_eq!(file.status, 206);
    assert_eq!(file.content_type, "text/plain");
    assert!(matches!(file.body, SourceResponseBody::File(path) if path == "missing/report.txt"));
    assert_eq!(
        file.headers[2..],
        [
            ("Set-Cookie".into(), "a=1".into()),
            ("Set-Cookie".into(), "b=2".into()),
            ("x-terlan".into(), "yes".into())
        ]
    );

    // The compiled Terlan builder supplies intent; the package admits file I/O.
    let bytes = [0, 255, 10, 42];
    std::fs::write(fixture.root.join("body.txt"), bytes).unwrap();
    for (supplied, expected) in [
        ("", "text/plain; charset=utf-8"),
        ("application/custom", "application/custom"),
    ] {
        let value = run(4, 206, "body.txt", supplied, "x-terlan", "file");
        let admitted =
            crate::commands::serve::handler::decode_owned_response(value, &fixture.root).unwrap();
        let wire = serve_vm_stream_handler_response(admitted, false).unwrap();
        assert_eq!(wire.status().as_u16(), 206);
        assert_eq!(wire.headers()[http::header::CONTENT_TYPE], expected);
        assert_eq!(wire.headers()[http::header::CONTENT_LENGTH], "4");
        assert_eq!(wire.headers()["x-terlan"], "file");
        assert_eq!(wire.body().as_ref(), bytes);
    }
    for path in ["../outside", "missing.txt"] {
        let value = run(4, 200, path, "", "x-terlan", "file");
        assert!(
            crate::commands::serve::handler::decode_owned_response(value, &fixture.root).is_err()
        );
    }

    let stream = response(run(
        5,
        207,
        "abcdef",
        "application/custom",
        "x-terlan",
        "yes",
    ))
    .unwrap();
    assert_eq!(stream.status, 207);
    assert_eq!(stream.content_type, "application/custom");
    let SourceResponseBody::Stream(mut stream) = stream.body else {
        panic!("bounded stream")
    };
    for expected in ["abcd", "ef", "tail"] {
        assert_eq!(stream.next_chunk().unwrap(), expected);
    }
    assert!(stream.next_chunk().is_none());

    for kind in 0..=5 {
        for status in [i64::MIN, -1, 0, 99, 600, 999, 1000, 65536, i64::MAX] {
            let error =
                response(run(kind, status, "body", "text/plain", "x-terlan", "yes")).unwrap_err();
            assert!(
                error.message().contains("outside HTTP range"),
                "{kind}/{status}: {error:?}"
            );
        }
    }
    for invalid in ["text/plain\r\nInjected: yes", "text/plain\0"] {
        for kind in [4, 5] {
            assert!(response(run(kind, 200, "body", invalid, "x-terlan", "yes"))
                .unwrap_err()
                .message()
                .contains("invalid response content type"));
        }
        assert!(response(run(0, 200, "body", "unused", "x-value", invalid))
            .unwrap_err()
            .message()
            .contains("response header"));
    }
    for name in [
        "bad header",
        "Content-Type",
        "content-type",
        "CONTENT-LENGTH",
    ] {
        assert!(
            response(run(0, 200, "body", "unused", name, "application/custom"))
                .unwrap_err()
                .message()
                .contains("response header")
        );
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
