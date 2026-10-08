//! Compiled Terlan streaming values cross the production response bridge and package codec.

use crate::commands::serve::handler_cache::handler_cache_test_support::compile_native_handler_fixture;
use crate::commands::serve::response_rendering::serve_vm_stream_handler_response;
use crate::runtime::vm::pure_native::PureNativeExecutionShard;
use crate::runtime::vm::ReplValue;
use terlan_http_native::http1::write_http1_response;
use terlan_http_native::HttpResponseChunks;

#[test]
fn compiled_source_stream_reaches_package_codec_with_metadata_and_bounds() {
    let fixture = compile_native_handler_fixture(
        "source_stream_package_codec",
        "src/app/Stream.terl",
        "app_Stream",
        r#"module app.Stream.
import std.http.Response.
import type std.http.Response.{Response}.
pub handle(body: String, size: Int): Response ->
    Response.stream(["ab", "", body], 201, "text/plain", size, 1)
        .with_status(202)
        .with_header("Set-Cookie", "a=1")
        .with_header("Set-Cookie", "b=2").
"#,
    );
    let mut shard = PureNativeExecutionShard::load_image(&fixture.image).unwrap();
    let owner = shard
        .spawn_fixed_owner_actor("app.Stream.handle", 2)
        .unwrap();
    for size in [-1, 0, 1, 2, 3, 64] {
        let body = "xy\u{e9}\u{1f642}";
        let value = shard
            .call_on_admitted_fixed_owner(
                owner,
                "app.Stream.handle",
                &[ReplValue::String(body.into()), ReplValue::Int(size)],
            )
            .unwrap();
        let result = crate::commands::serve::handler::decode_owned_response(value, &fixture.root);
        if size <= 0 {
            assert!(result
                .unwrap_err()
                .contains("invalid Response.stream limits"));
            continue;
        }
        let response = serve_vm_stream_handler_response(result.unwrap(), false).unwrap();
        assert_eq!(response.status(), 202);
        let stream = response.extensions().get::<HttpResponseChunks>().unwrap();
        let mut cursor = stream.clone();
        let mut expected_body = Vec::new();
        for original in [b"ab".as_slice(), body.as_bytes()] {
            for expected in original.chunks(size as usize) {
                assert_eq!(cursor.next_chunk().unwrap().as_ref(), expected);
                expected_body.extend_from_slice(format!("{:x}\r\n", expected.len()).as_bytes());
                expected_body.extend_from_slice(expected);
                expected_body.extend_from_slice(b"\r\n");
            }
        }
        assert!(cursor.is_complete());
        expected_body.extend_from_slice(b"0\r\n\r\n");
        let mut wire = Vec::new();
        write_http1_response(&mut wire, &response, false).unwrap();
        let split = wire
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        let head = std::str::from_utf8(&wire[..split]).unwrap();
        assert!(head.starts_with("HTTP/1.1 202 Accepted\r\n"), "{head}");
        assert!(head.contains("Transfer-Encoding: chunked\r\n"), "{head}");
        assert!(
            !head.to_ascii_lowercase().contains("content-length:"),
            "{head}"
        );
        assert!(head.contains("set-cookie: a=1\r\n"), "{head}");
        assert!(head.contains("set-cookie: b=2\r\n"), "{head}");
        assert_eq!(&wire[split..], expected_body);
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
