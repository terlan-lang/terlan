use super::json_string;

/// Encodes a command JSON string through the shared serde-backed helper.
///
/// Inputs:
/// - String content containing quotes, backslashes, newlines, tabs, and a
///   control character that the old hand-written helper did not cover.
///
/// Output:
/// - Test passes when the value round-trips through `serde_json`.
///
/// Transformation:
/// - Ensures command JSON string generation delegates escaping to the JSON
///   library rather than local replacement chains.
#[test]
fn json_string_round_trips_control_characters() {
    let raw = "quote \" slash \\ newline\n tab\t control\u{0008}";
    let encoded = json_string(raw);
    let decoded: String = serde_json::from_str(&encoded).expect("decode json string");

    assert_eq!(decoded, raw);
    assert!(encoded.contains("\\b"));
}
