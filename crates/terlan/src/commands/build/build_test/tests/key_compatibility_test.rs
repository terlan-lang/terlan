use super::*;

/// Emits a sealed native image whose descriptor does not require a signing key.
#[test]
fn key_compatibility_emits_keyless_vm_debug_metadata() {
    let dir = make_temp_dir("keyless_vm_debug_metadata");
    let source_path = dir.join("key_compatibility.terl");
    let out_dir = dir.join("build");
    fs::write(
        &source_path,
        "module key_compatibility.\n\npub main(): Bool -> true.\n",
    )
    .expect("write key compatibility source fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let image = fs::read(out_dir.join("vm/key_compatibility.tvm"))
        .expect("read key compatibility native image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let inspection = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect key compatibility native image");
    assert!(inspection.descriptor.signature.is_none());
    assert_ne!(inspection.descriptor.integrity.code_digest, [0; 32]);
    assert_ne!(
        inspection.descriptor.integrity.immutable_data_digest,
        [0; 32]
    );
}
