use super::*;

#[test]
fn skipped_c_symbols_are_sorted_and_cover_required_rejection_families() {
    let first = temp_dir("stable_first");
    let second = temp_dir("stable_second");
    generate_c_abi_bindings(&fixture_manifest(), &first).expect("first generation");
    generate_c_abi_bindings(&fixture_manifest(), &second).expect("second generation");

    let first_text =
        fs::read_to_string(first.join("bindings/skipped-symbols.json")).expect("first skips");
    let second_text =
        fs::read_to_string(second.join("bindings/skipped-symbols.json")).expect("second skips");
    assert_eq!(first_text, second_text);
    let skipped: Value = serde_json::from_str(&first_text).expect("parse skips");
    let reasons = skipped["skipped"]
        .as_array()
        .expect("skip array")
        .iter()
        .map(|entry| entry["reason"].as_str().expect("reason"))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        reasons,
        BTreeSet::from([
            "native_bindgen.c_abi_version_missing",
            "native_bindgen.c_borrowed_lifetime",
            "native_bindgen.c_missing_destructor",
            "native_bindgen.c_pointer_ownership_unknown",
            "native_bindgen.c_thread_local_error",
            "native_bindgen.c_unsupported_bitfield",
            "native_bindgen.c_unsupported_callback",
            "native_bindgen.c_unsupported_union",
            "native_bindgen.c_unsupported_variadic_function",
        ])
    );

    fs::remove_dir_all(first).expect("remove first output");
    fs::remove_dir_all(second).expect("remove second output");
}
