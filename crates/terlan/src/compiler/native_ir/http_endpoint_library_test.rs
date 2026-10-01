//! Endpoint registration retains source callbacks, not just queue limits.

use super::source_constructor_test::check_sources;

#[test]
fn standard_channel_tests_execute_source_descriptors_through_imports() {
    for (source, entry) in [
        (include_str!("../../../../../std/http/SseTest.terl"),
         "sse_callbacks_are_retained_without_mutating_the_original() and sse_endpoint_records_stream_limits()"),
        (include_str!("../../../../../std/http/WebSocketTest.terl"),
         "websocket_callbacks_and_pairing_are_retained_without_overwrite() and websocket_endpoint_records_channel_limits()"),
    ] {
        let root = format!("{source}\npub check(): Bool -> {entry}.");
        check_sources(&[
            &root,
            include_str!("../../../../../std/http/Sse.terl"),
            include_str!("../../../../../std/http/WebSocket.terl"),
            include_str!("../../../../../std/http/Response.terl"),
            include_str!("../../../../../std/core/Option.terl"),
        ]);
    }
}

#[test]
fn sse_callback_sets_preserve_values_limits_and_duplicate_evidence() {
    let body = r#"
import std.vm.Process.
idle(): Unit -> Unit.
consume(_value: String): Unit -> Unit.
not_called(): Unit -> Process.yield_now().
not_consumed(_value: String): Unit -> Process.yield_now().
pub check(): Bool ->
    let original = endpoint_with_keep_alive(7, 251, 101);
    let first = original.callbacks(idle, consume, idle, idle, consume);
    let second = first.callbacks(not_called, not_consumed, not_called, not_called, not_consumed);
    let empty = case original.#callbacks { [] -> true; _ -> false };
    let stored = case first.#callbacks {
        [configured] ->
            let open = configured.open;
            let event_ready = configured.event_ready;
            let keep_alive = configured.keep_alive;
            let drain = configured.drain;
            let cancellation = configured.cancellation;
            open() == Unit and event_ready("event") == Unit
                and keep_alive() == Unit and drain() == Unit and cancellation("closed") == Unit;
        _ -> false
    };
    let duplicate = case second.#callbacks { [_, _] -> true; _ -> false };
    empty and stored and duplicate and second.max_pending_events == 7
        and second.max_event_bytes == 251 and second.keep_alive_ms == Some(101).
"#;
    for owner in ["std.http.Sse", "app.Events"] {
        let source = format!(
            "{}\n{body}",
            include_str!("../../../../../std/http/Sse.terl")
        )
        .replace("std.http.Sse", owner);
        check_sources(&[&source]);
    }
}

#[test]
fn websocket_policies_preserve_captured_callbacks_and_all_pairing_parameters() {
    let body = r#"
import std.core.Option.{Some}.
import std.core.Result.{Ok}.
idle(): Unit -> Unit.
consume(_value: String): Unit -> Unit.
check_basic(policy: Policy): Bool -> case policy {
    Callbacks(open, inbound, writable, close, cancel) ->
        open() == Unit and inbound("frame") == Unit and writable() == Unit
            and close() == Unit and cancel("closed") == Unit;
    _ -> false
}.
check_pairing(policy: Policy): Bool -> case policy {
    Pairing("wait", "one", "two", "left", inbound, cancel) ->
        inbound("frame") == "captured:frame" and cancel("closed") == Unit;
    _ -> false
}.
check_stateful(policy: Policy): Bool -> case policy {
    StatefulPairing("state-wait", "state-one", "state-two", "state-left", inbound, cancel) ->
        let delivers = case inbound("s", 1, "f", "a", "b") { {"sf", Some("a"), Some("b")} -> true; _ -> false };
        delivers and cancel("closed") == Unit;
    _ -> false
}.
check_restorable(policy: Policy): Bool -> case policy {
    RestorablePairing(waiting, peer_left, identity, "room-", 107, 13,
        matched, restored_view, inbound, cancel) ->
        let selected = identity("/ws?room=r&player=two");
        let resolves = case selected { Ok(Some({room, role})) -> room == "r" and role == 2; _ -> false };
        let delivers = case inbound("s", 2, "f", "a", "b") { {"sf", Some("a"), Some("b")} -> true; _ -> false };
        resolves and waiting() == "captured:wait" and peer_left() == "captured:left"
            and matched("r", 1, "a", "b") == "rab"
            and restored_view("s", "r", 2, "a", "b") == "srab"
            and delivers
            and cancel("closed") == Unit;
    _ -> false
}.
pub check(): Bool ->
    let prefix = "captured:";
    let original = endpoint(11, 503);
    let basic = original.callbacks(idle, consume, idle, idle, consume);
    let paired = basic.paired_callbacks("wait", "one", "two", "left",
        (value: String) -> prefix + value, consume);
    let stateful = paired.stateful_paired_callbacks("state-wait", "state-one", "state-two", "state-left",
        (state: String, role: Int, frame: String, first: String, second: String) -> {state + frame, first, second}, consume);
    let restored = stateful.restorable_stateful_paired_callbacks(
        () -> prefix + "wait", () -> prefix + "left",
        "room", "player", "room-", "one", "two", 107, 13,
        (room: String, role: Int, first: String, second: String) -> room + first + second,
        (state: String, room: String, role: Int, first: String, second: String) -> state + room + first + second,
        (state: String, role: Int, frame: String, first: String, second: String) -> {state + frame, first, second}, consume);
    let empty = case original.#policies { [] -> true; _ -> false };
    let basic_unchanged = case basic.#policies { [Callbacks(_, _, _, _, _)] -> true; _ -> false };
    let policies = case restored.#policies {
        [basic_policy, paired_policy, stateful_policy, restored_policy] ->
            check_basic(basic_policy) and check_pairing(paired_policy)
                and check_stateful(stateful_policy) and check_restorable(restored_policy);
        _ -> false
    };
    empty and basic_unchanged and policies and restored.max_pending_frames == 11
        and restored.max_frame_bytes == 503.
"#;
    for owner in ["std.http.WebSocket", "app.Sockets"] {
        let source = format!(
            "{}\n{body}",
            include_str!("../../../../../std/http/WebSocket.terl")
        )
        .replace("module std.http.WebSocket.", &format!("module {owner}."));
        check_sources(&[
            &source,
            include_str!("../../../../../std/http/WebSocketIdentity.terl"),
            include_str!("../../../../../std/core/Option.terl"),
            include_str!("../../../../../std/core/Result.terl"),
            include_str!("../../../../../std/core/String.terl"),
            "module std.net.Uri. pub query_pairs(query: String): List[{String, String}] -> [{\"room\", \"r\"}, {\"player\", \"two\"}].",
        ]);
    }
}

#[test]
fn websocket_delivery_policy_is_executed_source_even_when_provider_is_renamed() {
    let provider = include_str!("../../../../../std/http/WebSocket.terl");
    let body = r#"
consume(_value: String): Unit -> Unit.
pub check(): Bool ->
    let prefix = "captured:";
    let endpoint = endpoint(4, 128).stateful_paired_callbacks("wait", "one", "two", "left",
        (state: String, role: Int, frame: String, first: String, second: String) ->
            {prefix + state + frame, first, second}, consume);
    case endpoint.#policies {
        [StatefulPairing(_, _, _, _, inbound, _)] ->
            let silent = case inbound("s", 1, "f", "", "") { {"captured:sf", None, None} -> true; _ -> false };
            let first = case inbound("s", 2, "f", "first", "") { {"captured:sf", Some("first"), None} -> true; _ -> false };
            let second = case inbound("s", 1, "f", "", "second") { {"captured:sf", None, Some("second")} -> true; _ -> false };
            let both = case inbound("s", 2, "f", " ", "text") { {"captured:sf", Some(" "), Some("text")} -> true; _ -> false };
            silent and first and second and both;
        _ -> false
    }.
"#;
    for owner in ["std.http.WebSocket", "app.SourceDelivery"] {
        let provider = provider.replace("module std.http.WebSocket.", &format!("module {owner}."));
        check_sources(&[
            &format!("{provider}\n{body}"),
            include_str!("../../../../../std/core/Option.terl"),
        ]);
        // Changing only package source must change delivery, including empty frames.
        let changed = provider.replace("\"\" -> None", "\"\" -> Some(payload)");
        assert_ne!(changed, provider);
        let expected = body.replace("None", "Some(\"\")");
        check_sources(&[
            &format!("{changed}\n{expected}"),
            include_str!("../../../../../std/core/Option.terl"),
        ]);
    }
}
