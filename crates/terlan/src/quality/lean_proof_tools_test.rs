use super::*;

#[test]
fn proof_loader_identity_includes_loader_and_resolved_libraries() {
    let result = loader_dependencies(
        "linux-vdso.so.1 (0x7ff)\n libLean.so => /tools/lib/libLean.so (0x123)\n /lib64/ld-linux.so.2 (0x456)\n",
    ).unwrap();
    assert_eq!(
        result,
        BTreeSet::from([
            PathBuf::from("/tools/lib/libLean.so"),
            PathBuf::from("/lib64/ld-linux.so.2"),
        ])
    );
}

#[test]
fn proof_loader_identity_rejects_missing_relative_and_empty_dependencies() {
    for output in [
        "",
        "statically linked",
        "libLean.so => not found",
        "libLean.so => ./relative.so (0x123)",
        "unrecognized output",
    ] {
        assert!(loader_dependencies(output).is_err(), "{output}");
    }
}
