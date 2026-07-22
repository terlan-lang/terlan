use super::*;

#[test]
fn contract_metadata_is_explicit_bounded_and_single_shot() {
    let contract = NativeAdapterAbiContract::current();
    let metadata = contract
        .render_metadata("x86_64-unknown-linux-gnu", "system_v")
        .expect("render canonical metadata");
    for field in [
        "adapter_abi_version = 1",
        "execution_context = \"explicit\"",
        "ownership = \"opaque_handles\"",
        "capability_lifetimes = \"explicit\"",
        "resource_lifetimes = \"execution_context_scoped\"",
        "max_frame_bytes = 1048576",
        "max_transfer_bytes = 16777216",
        "status_model = \"status_values\"",
        "callback_reentrancy = \"forbidden\"",
        "async_completion = \"single_shot\"",
    ] {
        assert!(metadata.contains(field), "missing `{field}` in {metadata}");
    }
}

#[test]
fn supported_target_matrix_has_one_canonical_calling_convention() {
    for (target, expected) in [
        ("x86_64-unknown-linux-gnu", "system_v"),
        ("aarch64-unknown-linux-gnu", "system_v"),
        ("x86_64-apple-darwin", "system_v"),
        ("aarch64-apple-darwin", "apple_aarch64"),
        ("x86_64-pc-windows-msvc", "windows_fastcall"),
        ("aarch64-pc-windows-msvc", "windows_fastcall"),
    ] {
        assert_eq!(calling_convention_for_target(target), Ok(expected));
    }
}

#[test]
fn contract_identity_rejects_unsupported_targets_and_ambiguous_components() {
    assert!(calling_convention_for_target("riscv64-unknown-linux-gnu").is_err());
    assert!(calling_convention_for_target("x86_64-unknown-freebsd").is_err());
    let contract = NativeAdapterAbiContract::current();
    for (target, convention) in [
        ("", "system_v"),
        ("x86_64:forged", "system_v"),
        ("x86_64-unknown-linux-gnu", "system_v\nforged"),
    ] {
        assert!(contract.cache_identity(target, convention).is_err());
    }
}
