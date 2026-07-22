use super::{deserialize_json_with_depth_limit, MAX_JSON_NESTING_DEPTH};

#[test]
fn deep_json_accepts_artifact_depth_and_rejects_hostile_depth() {
    let accepted = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    deserialize_json_with_depth_limit::<serde_json::Value>(accepted.as_bytes())
        .expect("artifact-sized nesting should deserialize");

    let rejected = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_NESTING_DEPTH + 1),
        "]".repeat(MAX_JSON_NESTING_DEPTH + 1)
    );
    let error = deserialize_json_with_depth_limit::<serde_json::Value>(rejected.as_bytes())
        .expect_err("hostile nesting should fail");
    assert!(error.contains("JSON nesting exceeds compiler limit"));
}
