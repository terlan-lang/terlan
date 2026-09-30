//! Channel values follow package source, not reserved module or builder names.

use super::source_constructor_test::check_sources;

#[test]
fn renamed_sse_response_uses_provider_body_defaults_and_stream_policy() {
    let provider = include_str!("../../../../../std/http/Sse.terl")
        .replace("std.http.Sse", "app.Sse")
        .replace("@compiler.native {std.http.sse.encode_event}\n", "")
        .replace("    native.", "    \"package framing\".")
        .replace("status: Int = 200", "status: Int = 219")
        .replace("16384", "17");
    check_sources(&[
        r#"module sse_response_authority.
import app.Sse.
import std.http.Response.
pub check(): Bool ->
    Sse.response([Sse.data("ignored")])
        == Response.stream(["package framing"], 219, "text/event-stream; charset=utf-8", 17, 128).
"#,
        &provider,
        include_str!("../../../../../std/http/Response.terl"),
    ]);
}

#[test]
fn renamed_sse_provider_and_changed_body_determine_event_contents() {
    let provider = include_str!("../../../../../std/http/Sse.terl");
    let caller = r#"
module sse_source_authority.
import std.http.Sse.
import std.core.Option.{Some}.
pub check(): Bool ->
    let event = Sse.data("payload").with_id("id").with_name("name").with_retry_ms(42);
    event.data == "payload" and event.id == Some("id")
        and event.event == Some("name") and event.retry_ms == Some(42).
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Sse", "app.Sse"),
        &provider.replace("std.http.Sse", "app.Sse"),
    ]);
    check_sources(&[
        &caller.replace(
            "event.data == \"payload\"",
            "event.data == \"source override\"",
        ),
        &provider.replace("data: value", "data: \"source override\""),
    ]);
}

#[test]
fn renamed_websocket_provider_and_changed_body_determine_frame_variant() {
    let provider = include_str!("../../../../../std/http/WebSocket.terl");
    let caller = r#"
module websocket_source_authority.
import std.http.WebSocket.
import std.http.WebSocket.{TextFrame}.
pub check(): Bool ->
    case WebSocket.text("payload") { TextFrame(value) -> value == "payload"; _ -> false }.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.WebSocket", "app.WebSocket"),
        &provider.replace("std.http.WebSocket", "app.WebSocket"),
    ]);
    let changed_provider = provider.replace("TextFrame(value).", "PongFrame(value).");
    assert_ne!(
        changed_provider, provider,
        "source mutation must replace the builder"
    );
    check_sources(&[
        &caller
            .replace("{TextFrame}", "{PongFrame}")
            .replace("TextFrame(value)", "PongFrame(value)"),
        &changed_provider,
    ]);
}
