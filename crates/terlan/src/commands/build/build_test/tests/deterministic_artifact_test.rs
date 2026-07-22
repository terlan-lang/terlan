use super::*;

/// Builds one source into an isolated output directory and reads its native image.
fn build_deterministic_fixture(source_path: &Path, out_dir: &Path) -> Vec<u8> {
    let state = CliState {
        out_dir: out_dir.to_path_buf(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    fs::read(out_dir.join("vm/deterministic_module.tvm")).expect("read deterministic native image")
}

/// Emits byte-identical native images across output roots and source rewrites.
#[test]
fn deterministic_module_emits_reproducible_vm_artifact_bytes() {
    let dir = make_temp_dir("deterministic_module_artifact");
    let source_path = dir.join("deterministic_module.terl");
    let source = "module deterministic_module.\n\npub main(): Int -> add(40, 2).\n\nadd(left: Int, right: Int): Int -> left + right.\n";
    fs::write(&source_path, source).expect("write deterministic source fixture");

    let first = build_deterministic_fixture(&source_path, &dir.join("build-a"));
    fs::write(&source_path, source).expect("rewrite source without content changes");
    let second = build_deterministic_fixture(&source_path, &dir.join("build-b"));

    assert_eq!(first, second, "native images must be byte reproducible");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let inspection = crate::runtime::native_image::inspect_tvm_image(&first, &target.triple)
        .expect("inspect deterministic native image");
    assert_eq!(
        inspection.descriptor.identity.module,
        "deterministic_module"
    );
    assert_eq!(inspection.descriptor.exports.len(), 1);
    assert_eq!(
        inspection.descriptor.exports[0].name,
        "deterministic_module.main/0"
    );
    assert_eq!(inspection.descriptor.callables.len(), 2);
    assert!(inspection
        .descriptor
        .callables
        .iter()
        .any(|callable| callable.id == inspection.descriptor.exports[0].id));
    assert!(inspection
        .descriptor
        .callables
        .iter()
        .any(|callable| !inspection
            .descriptor
            .exports
            .iter()
            .any(|export| export.id == callable.id)));
}
