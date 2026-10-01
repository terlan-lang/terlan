use super::source_test_support::assert_source_checks;

#[test]
fn canonical_websocket_identity_tests_execute_with_package_query_decoder() {
    assert_source_checks(
        "websocket_identity_source",
        &include_str!("../../../../../../std/http/WebSocketIdentityTest.terl").replace(
            "module std.http.WebSocketIdentityTest.",
            "module websocket_identity_source.",
        ),
        &[
            "fresh_or_restored_identity_follows_source_policy",
            "last_duplicate_value_wins_and_partial_identity_is_rejected",
        ],
    );
}

#[test]
fn canonical_sse_value_tests_execute_without_native_helpers() {
    assert_source_checks(
        "sse_source",
        &include_str!("../../../../../../std/http/SseTest.terl")
            .replace("module std.http.SseTest.", "module sse_source."),
        &[
            "sse_event_metadata_builders_record_typed_descriptor",
            "sse_endpoint_records_stream_limits",
            "sse_metadata_replacement_preserves_other_fields_and_empty_values",
            "sse_descriptors_do_not_silently_normalize_unvalidated_input",
            "sse_response_encodes_in_order_with_source_owned_stream_defaults",
            "sse_empty_response_retains_default_status_and_empty_stream",
        ],
    );
}

#[test]
fn canonical_websocket_value_tests_execute_without_native_helpers() {
    assert_source_checks(
        "websocket_source",
        &include_str!("../../../../../../std/http/WebSocketTest.terl")
            .replace("module std.http.WebSocketTest.", "module websocket_source."),
        &[
            "websocket_text_records_typed_descriptor",
            "websocket_control_records_close_descriptor",
            "websocket_control_records_ping_and_pong_descriptors",
            "websocket_endpoint_records_channel_limits",
            "websocket_variants_remain_distinct_with_identical_payloads",
            "websocket_payloads_are_source_values_not_encoded_frames",
        ],
    );
}
