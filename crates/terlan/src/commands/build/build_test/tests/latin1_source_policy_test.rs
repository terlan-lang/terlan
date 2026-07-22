use super::*;

/// Keeps build inference on the canonical UTF-8 source-reader policy.
#[test]
fn latin1_source_build_inference_rejects_non_utf8_input() {
    let dir = make_temp_dir("latin1_source_policy");
    let source_path = dir.join("latin1_source.terl");
    let mut source =
        b"// coding: latin-1\nmodule latin1_source.\n\npub value(): String -> \"".to_vec();
    let invalid_offset = source.len();
    source.extend_from_slice(&[0xe5, 0xe4, 0xf6]);
    source.extend_from_slice(b"\".\n");
    fs::write(&source_path, source).expect("write Latin-1 build fixture");

    let error = infer_build_target_profile(&source_path)
        .expect_err("build inference must reject non-UTF-8 source");

    assert_eq!(
        error,
        format!(
            "terlc build target inference failed: failed to read {}: Terlan source files must be UTF-8; invalid byte sequence starts at byte {invalid_offset} (line 4, column 25)",
            source_path.display()
        )
    );
}
