use super::*;

const SOURCE: &str =
    include_str!("../../../../runtime/vm/fixtures/annotation_isolation_parity.terl");

/// Keeps annotation metadata from rewriting the public native image boundary.
#[test]
fn annotation_isolation_parity_preserves_vm_artifact_function_inventory() {
    let dir = make_temp_dir("annotation_isolation_artifact");
    let source_path = dir.join("annotation_isolation_parity.terl");
    let out_dir = dir.join("build");
    fs::write(&source_path, SOURCE).expect("write annotation-isolation fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    let cmd = CliCommand {
        verb: Some("build".to_string()),
        args: vec![source_path.display().to_string()],
    };

    assert_eq!(run(cmd, state), ExitCode::SUCCESS);
    let image = fs::read(out_dir.join("vm/annotation_isolation_parity.tvm"))
        .expect("read annotation-isolation native image");
    let target = crate::runtime::native_image::host_tvm_target().expect("host TVM target");
    let inspection = crate::runtime::native_image::inspect_tvm_image(&image, &target.triple)
        .expect("inspect annotation-isolation native image");
    assert_eq!(
        inspection
            .descriptor
            .exports
            .iter()
            .map(|export| export.name.as_str())
            .collect::<Vec<_>>(),
        ["annotation_isolation_parity.run/0"]
    );
}
